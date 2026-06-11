//! Safe wrappers over `RegisterHotKey` / `UnregisterHotKey`.
//!
//! `RegisterHotKey` delivers `WM_HOTKEY` even for a `WS_EX_LAYERED +
//! WS_EX_TRANSPARENT + WS_EX_NOACTIVATE` HWND, so the overlay HWND is the target
//! directly instead of a separate message-only HWND.

#![allow(
    unsafe_code,
    reason = "FFI boundary; RegisterHotKey/UnregisterHotKey are unsafe in the windows crate."
)]

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, RegisterHotKey, UnregisterHotKey,
};

use crate::error::{PlatformError, Result};

/// `MOD_NOREPEAT` flag value (redeclared as a constant since windows-rs exposes
/// it only via `HOT_KEY_MODIFIERS`). OR-ing it into `RegisterHotKey`'s
/// `fsModifiers` suppresses auto-repeat `WM_HOTKEY` firing.
const MOD_NOREPEAT: u32 = 0x4000;

/// Safe wrapper over `RegisterHotKey(hwnd, id, modifiers, vk)`.
///
/// `repeatable = false` auto-adds `MOD_NOREPEAT` to suppress repeat firing while
/// held — for toggle actions (CycleMode / ToggleOnOff / Quit).
///
/// `repeatable = true` omits `MOD_NOREPEAT`, so `WM_HOTKEY` repeats at the key
/// repeat rate — for continuous-adjust actions (BumpThickness / BumpOpacity).
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

/// Safe wrapper over `UnregisterHotKey(hwnd, id)`. Failure is only logged.
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
