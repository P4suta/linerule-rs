//! Diagnostic helper: reads `GetWindowLongPtrW(GWL_EXSTYLE)` and logs whether
//! key ex-style flags (`WS_EX_LAYERED`, `WS_EX_TRANSPARENT`,
//! `WS_EX_NOREDIRECTIONBITMAP`, etc.) are set, for checkpoints during overlay
//! window setup.

#![forbid(unsafe_code)]

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    WS_EX_LAYERED, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
};

use crate::win32_ffi;

/// Read the HWND's ex-style flags and log presence of the key flags.
pub fn capture(hwnd: HWND, label: &'static str) {
    let ex = win32_ffi::get_ex_style(hwnd);
    let bits: u32 = u32::try_from(ex).unwrap_or(u32::MAX);

    let layered = bits & WS_EX_LAYERED.0 != 0;
    let transparent = bits & WS_EX_TRANSPARENT.0 != 0;
    let noredir = bits & WS_EX_NOREDIRECTIONBITMAP.0 != 0;
    let toolwindow = bits & WS_EX_TOOLWINDOW.0 != 0;
    let topmost = bits & WS_EX_TOPMOST.0 != 0;

    tracing::debug!(
        target: "ExStyleSnapshot",
        label,
        ex_style = format_args!("{bits:#010x}").to_string(),
        layered,
        transparent,
        noredir,
        toolwindow,
        topmost,
        "ex-style snapshot"
    );
}
