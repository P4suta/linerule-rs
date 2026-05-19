//! ★ FFI 境界 — `linerule-platform-windows` 内で唯一 `unsafe` を含むファイル。
//!
//! windows crate の Win32 / COM API は実質すべて `unsafe fn`。本ファイルは
//! それらを薄く safe にラップし、他のモジュール
//! (`overlay_window.rs`, `wndproc.rs`, `monitor_info.rs`, `windows_app.rs`, ...)
//! はここから safe 関数だけを呼ぶ。各 `unsafe { ... }` ブロックの直前に
//! `// SAFETY: ...` コメントを必須化する。詳細方針は ADR-0003 参照。

#![allow(
    unsafe_code,
    reason = "FFI 境界。windows crate の Win32 / COM API は全部 unsafe fn。\
              他の全モジュールは #![forbid(unsafe_code)] で、本ファイルが\
              唯一の集約点。ADR-0003 参照。"
)]

use core::ptr::NonNull;
use std::panic::{AssertUnwindSafe, catch_unwind};

use windows::Win32::Foundation::{HINSTANCE, HMODULE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{HMONITOR, MONITORINFO, MonitorFromPoint};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA,
    GetMessageW, GetSystemMetrics, GetWindowLongPtrW, MSG, PostQuitMessage, RegisterClassExW,
    SM_CXSCREEN, SM_CYSCREEN, SetWindowLongPtrW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_NCCREATE, WNDCLASSEXW, WNDPROC,
};
use windows::core::PCWSTR;

use crate::error::{Result, Win32Error, decode_last_error};
use crate::overlay_state::OverlayWndState;

// ---- module handle ---------------------------------------------------------

/// 現在のプロセスの module handle を取得する。`CreateWindowExW` や
/// `RegisterClassExW` の `hInstance` 引数に渡す。
pub fn module_handle() -> Result<HINSTANCE> {
    // SAFETY: PCWSTR::null() で current process の HMODULE を取得する標準呼び出し。
    let h: HMODULE =
        unsafe { GetModuleHandleW(PCWSTR::null()) }.map_err(|e| Win32Error::BadHr {
            operation: "GetModuleHandleW",
            hr: e.code().0,
        })?;
    Ok(HINSTANCE(h.0))
}

// ---- class registration ----------------------------------------------------

/// `RegisterClassExW` の薄い safe wrapper。成功時 class atom を返す。
pub fn register_class(name: PCWSTR, wnd_proc: WNDPROC) -> Result<u16> {
    let h_instance = module_handle()?;
    let wc = WNDCLASSEXW {
        cbSize: u32::try_from(core::mem::size_of::<WNDCLASSEXW>()).unwrap_or(u32::MAX),
        lpfnWndProc: wnd_proc,
        hInstance: h_instance,
        lpszClassName: name,
        ..Default::default()
    };

    // SAFETY: `wc` は完全初期化、ポインタ引数も valid。失敗時 0 を返す。
    let atom = unsafe { RegisterClassExW(&wc) };
    if atom == 0 {
        return Err(last_error("RegisterClassExW"));
    }
    Ok(atom)
}

// ---- window lifecycle ------------------------------------------------------

/// `CreateWindowExW` の薄い safe wrapper。
///
/// `create_param` は WM_NCCREATE 経由で WndProc に届く `*mut OverlayWndState`
/// （`Box::into_raw` の結果）を期待する。
#[allow(
    clippy::too_many_arguments,
    reason = "Win32 API の引数構造をそのまま反映するため。グルーピングは型では行わない方が呼び出し側で読みやすい"
)]
pub fn create_window(
    ex_style: WINDOW_EX_STYLE,
    class_name: PCWSTR,
    title: PCWSTR,
    style: WINDOW_STYLE,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    create_param: *mut OverlayWndState,
) -> Result<HWND> {
    let h_instance = module_handle()?;
    // SAFETY: 引数はすべて valid 範囲。失敗時 null HWND を返す。
    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            class_name,
            title,
            style,
            x,
            y,
            width,
            height,
            None,
            None,
            Some(h_instance),
            Some(create_param.cast()),
        )
    }
    .map_err(|e| Win32Error::BadHr {
        operation: "CreateWindowExW",
        hr: e.code().0,
    })?;
    if hwnd.0.is_null() {
        return Err(Win32Error::NullHandle {
            operation: "CreateWindowExW",
        });
    }
    Ok(hwnd)
}

/// `DestroyWindow` の薄い safe wrapper。失敗してもプログラムは続行する想定
/// （Drop から呼ばれるため）。
pub fn destroy_window(hwnd: HWND) -> Result<()> {
    // SAFETY: hwnd は OverlayWindow が所有する有効 HWND。
    unsafe { DestroyWindow(hwnd) }.map_err(|e| Win32Error::BadHr {
        operation: "DestroyWindow",
        hr: e.code().0,
    })
}

// ---- GWLP_USERDATA (instance state) ----------------------------------------

/// WM_NCCREATE で `Box::into_raw` した `*mut OverlayWndState` を `GWLP_USERDATA`
/// に格納する。
pub fn set_userdata(hwnd: HWND, ptr: *mut OverlayWndState) {
    // SAFETY: hwnd は valid、ptr は呼び出し側が所有する Box::into_raw 由来か null。
    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize) };
}

/// `GWLP_USERDATA` に格納された `*mut OverlayWndState` を `NonNull` で取り出す。
/// まだ設定されていない（WM_NCCREATE 前）ときは `None`。
pub fn get_userdata(hwnd: HWND) -> Option<NonNull<OverlayWndState>> {
    // SAFETY: 単純な GWLP_USERDATA 読み出し。null チェックで安全化。
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut OverlayWndState;
    NonNull::new(raw)
}

/// `GWLP_USERDATA` を 0 にクリアし、保存していた Box を回収して所有権を返す。
/// WM_NCDESTROY の中で 1 度だけ呼ぶ。
pub fn take_userdata(hwnd: HWND) -> Option<Box<OverlayWndState>> {
    // SAFETY: SetWindowLongPtrW で 0 を入れ、直前の値を読み出す（atomic swap 相当）。
    let raw = unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) } as *mut OverlayWndState;
    if raw.is_null() {
        return None;
    }
    // SAFETY: WM_NCCREATE で `Box::into_raw` した値を WM_NCDESTROY で 1 度だけ回収。
    Some(unsafe { Box::from_raw(raw) })
}

/// CreateWindowExW が失敗したときの Box 解放専用 helper。WM_NCCREATE が呼ばれて
/// いないため `GWLP_USERDATA` には届かない値を、呼び出し側から直接渡して drop する。
pub fn drop_userdata_raw(ptr: *mut OverlayWndState) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: 呼び出し側が `Box::into_raw` の結果を渡し、二重解放しないことを保証。
    drop(unsafe { Box::from_raw(ptr) });
}

/// `NonNull<OverlayWndState>` を `&OverlayWndState` に変換する。
/// WndProc 1 回の dispatch 中のみ valid な参照を返す。
pub fn state_ref<'a>(ptr: NonNull<OverlayWndState>) -> &'a OverlayWndState {
    // SAFETY: ptr は WM_NCCREATE で確立した stable address。WndProc は単一 UI
    // thread からのみ呼ばれ、dispatch 関数が return するまで生きている。
    unsafe { ptr.as_ref() }
}

// ---- message dispatch ------------------------------------------------------

/// `DefWindowProcW` の薄い safe wrapper。
pub fn def_window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: 標準的な Win32 メッセージ転送。引数は WndProc から受け取った
    // ものをそのまま流す。
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// `PostQuitMessage(exit_code)` の薄い safe wrapper。
pub fn post_quit(exit_code: i32) {
    // SAFETY: 単純な POST。失敗しない。
    unsafe { PostQuitMessage(exit_code) };
}

/// メッセージポンプを 1 周。
///
/// 戻り値:
/// - `Some(true)`: メッセージを処理した。続行可能。
/// - `Some(false)`: `WM_QUIT` を受信。ループを抜ける。
/// - `None`: `GetMessageW` が -1 を返した（API エラー）。
pub fn pump_one() -> Option<bool> {
    let mut msg = MSG::default();
    // SAFETY: msg は zero-init の out param、他は default 引数。
    let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
    match r.0 {
        0 => Some(false),
        -1 => None,
        _ => {
            // SAFETY: msg は GetMessageW が成功時に初期化済み。
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            Some(true)
        },
    }
}

// ---- cursor position -------------------------------------------------------

/// `GetCursorPos` の薄い safe wrapper。
pub fn cursor_pos() -> Result<linerule_core::Point<linerule_core::Logical>> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut pt = POINT::default();
    // SAFETY: pt は zero-init の out param
    unsafe { GetCursorPos(&mut pt) }.map_err(|e| Win32Error::BadHr {
        operation: "GetCursorPos",
        hr: e.code().0,
    })?;
    Ok(linerule_core::Point::new(pt.x, pt.y))
}

// ---- monitor info ----------------------------------------------------------

/// `GetSystemMetrics(SM_CXSCREEN)` の薄い safe wrapper。
pub fn screen_width() -> i32 {
    // SAFETY: GetSystemMetrics は引数チェックなしで読み出すだけ。
    unsafe { GetSystemMetrics(SM_CXSCREEN) }
}

/// `GetSystemMetrics(SM_CYSCREEN)` の薄い safe wrapper。
pub fn screen_height() -> i32 {
    // SAFETY: 同上。
    unsafe { GetSystemMetrics(SM_CYSCREEN) }
}

// ---- WndProc entry point ---------------------------------------------------

/// linerule overlay の WndProc 本体。
///
/// `RegisterClassExW` の `lpfnWndProc` に渡す `unsafe extern "system" fn` の型を
/// 満たすため、本関数だけは declaration に `unsafe` キーワードを持つ。本体内では
/// - WM_NCCREATE で `GWLP_USERDATA` に instance state を仕込み
/// - その他は `crate::wndproc::dispatch` (safe) に委譲し
/// - dispatch 中の panic を `catch_unwind` で吸収して `DefWindowProcW` に
///   フォールバックする
///
/// `dispatch` は `#![forbid(unsafe_code)]` の wndproc.rs にあり、追加の unsafe を
/// 発生させない。
pub unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_NCCREATE {
        // SAFETY: WM_NCCREATE 時の lparam は CREATESTRUCTW* (Win32 仕様)。
        let cs = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        let raw = cs.lpCreateParams.cast::<OverlayWndState>();
        set_userdata(hwnd, raw);
        return def_window_proc(hwnd, msg, wparam, lparam);
    }

    let result = catch_unwind(AssertUnwindSafe(|| {
        crate::wndproc::dispatch(hwnd, msg, wparam, lparam)
    }));

    match result {
        Ok(Some(lresult)) => lresult,
        Ok(None) => def_window_proc(hwnd, msg, wparam, lparam),
        Err(_panic) => {
            // panic は飲み込み、プロセスは生かす（読書ツールが落ちないこと）。
            def_window_proc(hwnd, msg, wparam, lparam)
        },
    }
}

// ---- ex-style snapshot helpers ---------------------------------------------

/// `GWL_EXSTYLE` (= -20) を `GetWindowLongPtrW` で取り出す薄い safe wrapper。
pub fn get_ex_style(hwnd: HWND) -> isize {
    use windows::Win32::UI::WindowsAndMessaging::GWL_EXSTYLE;
    // SAFETY: GWL_EXSTYLE 単純読み出し。
    unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) }
}

// ---- last-error helpers ----------------------------------------------------

/// `GetLastError()` を読み取り [`Win32Error::LastError`] を構築する。
pub fn last_error(operation: &'static str) -> Win32Error {
    use windows::Win32::Foundation::GetLastError;
    // SAFETY: GetLastError は副作用なく直近の thread-local エラーを返す。
    let code = unsafe { GetLastError() }.0;
    Win32Error::LastError {
        operation,
        code,
        symbol: decode_last_error(code),
    }
}

// ---- monitor info ----------------------------------------------------------

/// `MonitorFromPoint(0, 0, MONITOR_DEFAULTTOPRIMARY)` の薄い safe wrapper。
pub fn primary_monitor() -> HMONITOR {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTOPRIMARY;
    // SAFETY: MonitorFromPoint は座標と flag を受け取り HMONITOR を返す read-only API。
    unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) }
}

/// `GetMonitorInfoW` の薄い safe wrapper。
pub fn get_monitor_info(hmonitor: HMONITOR) -> Result<MONITORINFO> {
    use windows::Win32::Graphics::Gdi::GetMonitorInfoW;
    let mut info = MONITORINFO {
        cbSize: u32::try_from(core::mem::size_of::<MONITORINFO>()).unwrap_or(u32::MAX),
        ..Default::default()
    };
    // SAFETY: info.cbSize が正しく設定されており、hmonitor は valid を期待。
    let ok = unsafe { GetMonitorInfoW(hmonitor, &mut info) };
    if !ok.as_bool() {
        return Err(last_error("GetMonitorInfoW"));
    }
    Ok(info)
}
