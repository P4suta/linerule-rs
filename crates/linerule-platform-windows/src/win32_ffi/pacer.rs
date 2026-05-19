//! `DwmFlush` + `PostMessageW` の薄い safe wrapper。
//!
//! Phase F の `render_clock.rs` から別 thread で `DwmFlush` を呼んで vsync を
//! 待ち、`PostMessageW(target_hwnd, WM_APP_TICK, 0, 0)` で UI thread を起こす。

#![allow(
    unsafe_code,
    reason = "FFI 境界。DwmFlush / PostMessageW は windows crate でも unsafe。ADR-0003。"
)]

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::error::{Result, Win32Error};

/// `DwmFlush()` の薄い safe wrapper。次の vsync まで block する。
pub fn dwm_flush() -> Result<()> {
    // SAFETY: DwmFlush は引数なしの blocking call。
    unsafe { DwmFlush() }.map_err(|e| Win32Error::BadHr {
        operation: "DwmFlush",
        hr: e.code().0,
    })
}

/// `PostMessageW(hwnd, msg, 0, 0)` の薄い safe wrapper。
///
/// # Errors
/// `PostMessageW` が FALSE を返したとき。
pub fn post_message(hwnd: HWND, msg: u32) -> Result<()> {
    // SAFETY: hwnd valid (overlay window or hotkey host), msg は WM_APP_*
    unsafe { PostMessageW(Some(hwnd), msg, WPARAM(0), LPARAM(0)) }.map_err(|e| Win32Error::BadHr {
        operation: "PostMessageW",
        hr: e.code().0,
    })
}
