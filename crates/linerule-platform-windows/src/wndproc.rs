//! WndProc dispatch ロジック (forbid(unsafe_code))。
//!
//! 実 FFI 入口 `unsafe extern "system" fn overlay_wnd_proc` は `win32_ffi.rs`
//! 側にあり、本ファイルは `dispatch()` 関数として呼び出される。WM_NCCREATE で
//! `GWLP_USERDATA` に Box を仕込む処理も `win32_ffi.rs` 側にあるため、本
//! ファイルでは取り出した state ref を使うだけで `unsafe` は出現しない。

#![forbid(unsafe_code)]

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    WM_DESTROY, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_NCDESTROY, WM_NCHITTEST, WM_RBUTTONDOWN,
};

use crate::messages::HTTRANSPARENT;
use crate::win32_ffi;

/// WM_NCCREATE / WM_NCDESTROY 以外のメッセージを dispatch する純粋関数。
///
/// 戻り値:
/// - `Some(LRESULT)`: 当該メッセージを処理した。返り値はそのまま `WndProc` の戻り値になる。
/// - `None`: 処理せず `DefWindowProcW` にフォールバックすることを呼び出し側に依頼。
#[must_use]
pub fn dispatch(hwnd: HWND, msg: u32, _wparam: WPARAM, _lparam: LPARAM) -> Option<LRESULT> {
    let state_ptr = win32_ffi::get_userdata(hwnd)?;
    let state = win32_ffi::state_ref(state_ptr);

    match msg {
        WM_NCHITTEST => {
            if let Some(count) = state.tick_nchit() {
                tracing::trace!(
                    parent: state.span(),
                    count,
                    "WM_NCHITTEST -> HTTRANSPARENT"
                );
            }
            Some(LRESULT(HTTRANSPARENT as isize))
        },
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            let count = state.tick_click();
            tracing::warn!(
                parent: state.span(),
                msg = format_args!("{msg:#06x}").to_string(),
                count,
                "click reached overlay (click-through failed)"
            );
            None
        },
        WM_DESTROY => {
            win32_ffi::post_quit(0);
            Some(LRESULT(0))
        },
        WM_NCDESTROY => {
            // `Box<OverlayWndState>` を取り戻して drop。win32_ffi 側で
            // GWLP_USERDATA を 0 に戻し Box::from_raw する。
            let _ = win32_ffi::take_userdata(hwnd);
            None
        },
        _ => None,
    }
}
