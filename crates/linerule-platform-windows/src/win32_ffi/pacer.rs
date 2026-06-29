//! Safe wrappers over `DwmFlush` + `PostMessageW`.
//!
//! A worker thread `DwmFlush`es for vsync, then `PostMessageW`s to wake the UI thread.

#![allow(
    unsafe_code,
    reason = "FFI boundary; DwmFlush/PostMessageW are unsafe in the windows crate."
)]

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::error::{PlatformError, Result};

/// Safe wrapper over `DwmFlush()`. Blocks until the next vsync.
pub fn dwm_flush() -> Result<()> {
    // SAFETY: DwmFlush is an argument-free blocking call.
    unsafe { DwmFlush() }.map_err(|e| PlatformError::BadHr {
        operation: "DwmFlush",
        hr: e.code().0,
    })
}

/// Safe wrapper over `PostMessageW(hwnd, msg, 0, 0)`.
///
/// # Errors
/// When `PostMessageW` returns FALSE.
pub fn post_message(hwnd: HWND, msg: u32) -> Result<()> {
    // SAFETY: hwnd valid (overlay window or hotkey host); msg is a WM_APP_*.
    unsafe { PostMessageW(Some(hwnd), msg, WPARAM(0), LPARAM(0)) }.map_err(|e| {
        PlatformError::BadHr {
            operation: "PostMessageW",
            hr: e.code().0,
        }
    })
}
