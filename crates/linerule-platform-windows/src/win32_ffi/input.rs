//! Safe wrapper over `SendInput` for synthesizing modifier+key chords.
//!
//! Drives `RegisterHotKey` hotkeys from a separate input source (only works on
//! an interactive desktop session).

#![allow(
    unsafe_code,
    reason = "FFI boundary; SendInput is unsafe in the windows crate."
)]

use windows::Win32::Foundation::GetLastError;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_SHIFT,
};

use crate::error::{PlatformError, Result, decode_last_error};

// `RegisterHotKey` `fsModifiers` flags; duplicated to avoid a value dependency.
const MOD_ALT: u32 = 0x0001;
const MOD_CONTROL: u32 = 0x0002;
const MOD_SHIFT: u32 = 0x0004;
const MOD_WIN: u32 = 0x0008;

/// Synthesize a modifier+key chord as one atomic `SendInput` batch (modifier
/// downs, key down+up, modifier ups in reverse) so it can't interleave with
/// other system input. `modifier_flags` uses `RegisterHotKey` `fsModifiers` bits.
///
/// # Errors
/// When `SendInput` injects fewer events than requested (input blocked), tagged
/// with `GetLastError`.
#[allow(
    clippy::cast_possible_truncation,
    reason = "vk is a Win32 virtual-key code in 0x00..=0xFE, always fits u16"
)]
pub fn send_chord(modifier_flags: u32, vk: u32) -> Result<()> {
    let mut modifiers: Vec<VIRTUAL_KEY> = Vec::with_capacity(4);
    if modifier_flags & MOD_CONTROL != 0 {
        modifiers.push(VK_CONTROL);
    }
    if modifier_flags & MOD_ALT != 0 {
        modifiers.push(VK_MENU);
    }
    if modifier_flags & MOD_SHIFT != 0 {
        modifiers.push(VK_SHIFT);
    }
    if modifier_flags & MOD_WIN != 0 {
        modifiers.push(VK_LWIN);
    }

    let key = VIRTUAL_KEY(vk as u16);
    let extended = is_extended(vk);

    let mut inputs: Vec<INPUT> = Vec::with_capacity(modifiers.len() * 2 + 2);
    for &m in &modifiers {
        inputs.push(key_event(m, false, false));
    }
    inputs.push(key_event(key, false, extended));
    inputs.push(key_event(key, true, extended));
    for &m in modifiers.iter().rev() {
        inputs.push(key_event(m, true, false));
    }

    // SAFETY: valid non-empty `INPUT` slice with correct element size; read synchronously.
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent as usize == inputs.len() {
        return Ok(());
    }
    // SAFETY: plain getter, no preconditions.
    let code = unsafe { GetLastError() }.0;
    Err(PlatformError::LastError {
        operation: "SendInput",
        code,
        symbol: decode_last_error(code),
    })
}

/// Build one keyboard `INPUT` event for `vk` (down or up, optionally extended).
fn key_event(vk: VIRTUAL_KEY, up: bool, extended: bool) -> INPUT {
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Whether `vk` is an extended key (arrow cluster) needing `KEYEVENTF_EXTENDEDKEY`.
const fn is_extended(vk: u32) -> bool {
    // VK_LEFT..=VK_DOWN.
    matches!(vk, 0x25..=0x28)
}
