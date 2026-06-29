//! Safe wrappers over `RegisterHotKey` / `UnregisterHotKey`.
//!
//! `WM_HOTKEY` is delivered even to a `WS_EX_LAYERED|WS_EX_TRANSPARENT|WS_EX_NOACTIVATE`
//! HWND, so the overlay HWND is the target directly (no message-only HWND needed).

#![allow(
    unsafe_code,
    reason = "FFI boundary; RegisterHotKey/UnregisterHotKey are unsafe in the windows crate."
)]

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, RegisterHotKey, UnregisterHotKey,
};

use crate::error::{PlatformError, Result};

/// `MOD_NOREPEAT`; redeclared since windows-rs exposes it only via `HOT_KEY_MODIFIERS`.
const MOD_NOREPEAT: u32 = 0x4000;

/// Safe wrapper over `RegisterHotKey`.
///
/// `repeatable = false` adds `MOD_NOREPEAT` (suppresses repeat while held);
/// `repeatable = true` lets `WM_HOTKEY` repeat at the key repeat rate.
///
/// # Errors
/// When `RegisterHotKey` returns FALSE (e.g. duplicate registration).
pub fn register_hotkey(
    hwnd: HWND,
    id: i32,
    modifiers: u32,
    vk: u32,
    repeatable: bool,
) -> Result<()> {
    let mods = if repeatable {
        modifiers
    } else {
        modifiers | MOD_NOREPEAT
    };
    let m = HOT_KEY_MODIFIERS(mods);
    // SAFETY: hwnd is valid (overlay HWND); id / modifiers / vk are plain ints.
    unsafe { RegisterHotKey(Some(hwnd), id, m, vk) }.map_err(|e| PlatformError::BadHr {
        operation: "RegisterHotKey",
        hr: e.code().0,
    })
}

/// Safe wrapper over `UnregisterHotKey`.
///
/// # Errors
/// When `UnregisterHotKey` returns FALSE.
pub fn unregister_hotkey(hwnd: HWND, id: i32) -> Result<()> {
    // SAFETY: hwnd / id are valid.
    unsafe { UnregisterHotKey(Some(hwnd), id) }.map_err(|e| PlatformError::BadHr {
        operation: "UnregisterHotKey",
        hr: e.code().0,
    })
}
