//! ★ FFI 境界 — Accessibility hook (`SetWinEventHook` / `UnhookWinEvent`) と
//! z-order 再 assert (`SetWindowPos`)。
//!
//! `linerule-platform-windows` 内で `unsafe` を含む副集約。前景アプリ変更を
//! 監視して overlay の z-order を最前面に再 assert するために使う。callback
//! 本体 (`extern "system" fn`) もここに局在化し、`catch_unwind` で OS thread
//! への panic 漏洩を防ぐ。
//!
//! 設計のキー:
//! - `WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS` で OS が自プロセスの
//!   前景化通知を抑制してくれるので、callback 側で HWND 比較する必要がない。
//! - HWND は `!Send` だが `AtomicIsize` 越しの isize 表現で thread 越えする
//!   (既存 `render_clock.rs` の HWND 越境パターンと同じ)。
//! - callback 内では `PostMessageW(WM_APP_REASSERT_TOPMOST)` だけ。実 SetWindowPos
//!   は UI thread 側 (`wndproc::dispatch`) で行う。`PostMessageW` は thread-safe。

#![allow(
    unsafe_code,
    reason = "FFI 境界。SetWinEventHook / UnhookWinEvent / SetWindowPos / \
              PostMessageW は windows crate の unsafe fn。本ファイルが集約点。"
)]

use core::ffi::c_void;
use core::sync::atomic::{AtomicIsize, Ordering};
use std::panic::{AssertUnwindSafe, catch_unwind};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_SYSTEM_FOREGROUND, HWND_TOPMOST, PostMessageW, SET_WINDOW_POS_FLAGS, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

use crate::error::{PlatformError, Result};
use crate::messages::WM_APP_REASSERT_TOPMOST;

/// callback (OS hook thread) から UI thread の overlay HWND に向けて
/// `PostMessageW` するために、overlay HWND を atomically 共有する。`HWND` は
/// `!Send` なので isize 経由で hop する（render_clock.rs と同じパターン）。
/// 0 = uninstalled / no target。
static TARGET_HWND: AtomicIsize = AtomicIsize::new(0);

/// 前景アプリ変更通知を register する。`target` は通知を受ける overlay HWND。
/// 戻り値の `HWINEVENTHOOK` は [`unhook_win_event`] で必ず解除すること
/// (RAII は `crate::foreground_hook::ForegroundHook` 側)。
///
/// `WINEVENT_SKIPOWNPROCESS` を指定するので自プロセス由来のイベントは OS が
/// 抑制する。callback 側での HWND 比較は不要。
pub fn set_foreground_hook(target: HWND) -> Result<HWINEVENTHOOK> {
    TARGET_HWND.store(target.0 as isize, Ordering::SeqCst);
    // SAFETY: 引数は全て Windows SDK の正規範囲。callback は static fn pointer
    // (lifetime 不要)。0/0 = 全プロセス全 thread を監視。
    let hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_foreground_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };
    if hook.0.is_null() {
        TARGET_HWND.store(0, Ordering::SeqCst);
        return Err(PlatformError::NullHandle {
            operation: "SetWinEventHook",
        });
    }
    Ok(hook)
}

/// register した hook を解除する。`Drop` から呼ばれる前提なので、失敗しても
/// プログラム継続のために `Result` で返すだけ。
pub fn unhook_win_event(hook: HWINEVENTHOOK) -> Result<()> {
    // SAFETY: hook は set_foreground_hook 由来。null は呼び出し側で除外。
    let ok = unsafe { UnhookWinEvent(hook) };
    TARGET_HWND.store(0, Ordering::SeqCst);
    if !ok.as_bool() {
        return Err(PlatformError::BadHr {
            operation: "UnhookWinEvent",
            hr: 0,
        });
    }
    Ok(())
}

/// `SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE)`。
/// overlay の z-order を最前面に戻す。focus は奪わない。位置/サイズは不変。
pub fn reassert_topmost(hwnd: HWND) -> Result<()> {
    let flags: SET_WINDOW_POS_FLAGS = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
    // SAFETY: hwnd は OverlayWindow 由来の valid HWND。HWND_TOPMOST は WinAPI 定数。
    unsafe { SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, flags) }.map_err(|e| {
        PlatformError::BadHr {
            operation: "SetWindowPos(HWND_TOPMOST)",
            hr: e.code().0,
        }
    })
}

/// `SetWinEventHook` の callback。OS hook thread から呼ばれる。
/// `WINEVENT_SKIPOWNPROCESS` 指定済みなので、自プロセス由来のイベントは届かない。
/// 本体は `PostMessageW` 1 本のみ。`SetWindowPos` 等の重い処理は UI thread に委ねる。
extern "system" fn on_foreground_event(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _object_id: i32,
    _child_id: i32,
    _thread_id: u32,
    _time: u32,
) {
    // OS callback への panic 漏洩を防ぐ (win32_ffi/core.rs::overlay_wnd_proc 同様)。
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let raw = TARGET_HWND.load(Ordering::SeqCst);
        if raw == 0 {
            return;
        }
        let target = HWND(raw as *mut c_void);
        // SAFETY: PostMessageW は thread-safe (Microsoft 仕様)。target は
        // AtomicIsize で生存中の overlay HWND。失敗しても visual 影響無し
        // (hook thread から tracing するのは避ける)。
        let _ =
            unsafe { PostMessageW(Some(target), WM_APP_REASSERT_TOPMOST, WPARAM(0), LPARAM(0)) };
    }));
}

// テストはコンパイル時保証 (WINEVENTPROC シグネチャ整合) と messages.rs 側の
// `WM_APP_REASSERT_TOPMOST` 帯テストでカバー。SetWinEventHook / SetWindowPos
// の実呼び出しは Windows native 環境必須で、global static `TARGET_HWND` に
// 副作用を持つため unit test には適さない。
