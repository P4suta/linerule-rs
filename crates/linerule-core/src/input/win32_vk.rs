//! Pure data mapping from [`ChordSpec`](crate::input::chord::ChordSpec) to the
//! `(modifiers, vk)` integer pair that `RegisterHotKey` on Win32 expects.
//!
//! Lives in `linerule-core` (not in `linerule-platform-windows`) because:
//! - it depends only on `linerule-core` ADTs (no `windows` crate);
//! - keeping it here lets the Linux CI runner unit-test the mapping without
//!   `#![cfg(windows)]` gating the whole `platform-windows` crate;
//! - it is the single source of truth used by both the integration test that
//!   walks [`HotkeyMap::DEFAULT`](crate::input::hotkey_map::HotkeyMap) and the
//!   real `RegisterHotKey` call in the platform layer.
//!
//! Constants match the values documented at
//! <https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-registerhotkey>
//! and the Win32 virtual-key code table at
//! <https://learn.microsoft.com/windows/win32/inputdev/virtual-key-codes>.

use crate::input::chord::{ChordSpec, Direction, KeyCode, Modifiers};

/// `RegisterHotKey` `fsModifiers` flag for the `Alt` key.
pub const MOD_ALT: u32 = 0x0001;
/// `RegisterHotKey` `fsModifiers` flag for the `Ctrl` key.
pub const MOD_CONTROL: u32 = 0x0002;
/// `RegisterHotKey` `fsModifiers` flag for the `Shift` key.
pub const MOD_SHIFT: u32 = 0x0004;
/// `RegisterHotKey` `fsModifiers` flag for the `Win` key.
pub const MOD_WIN: u32 = 0x0008;

/// Translate a [`ChordSpec`] into the `(fsModifiers, vk)` pair expected by
/// `RegisterHotKey`.
///
/// The function is total: every variant of [`KeyCode`] and every combination
/// of [`Modifiers`] flags has a deterministic mapping.
#[must_use]
pub const fn chord_to_win32(chord: ChordSpec) -> (u32, u32) {
    let mut mods = 0u32;
    if chord.modifiers.contains(Modifiers::ALT) {
        mods |= MOD_ALT;
    }
    if chord.modifiers.contains(Modifiers::CTRL) {
        mods |= MOD_CONTROL;
    }
    if chord.modifiers.contains(Modifiers::SHIFT) {
        mods |= MOD_SHIFT;
    }
    if chord.modifiers.contains(Modifiers::META) {
        mods |= MOD_WIN;
    }
    let vk = key_to_vk(chord.key);
    (mods, vk)
}

/// Translate a [`KeyCode`] into its Win32 virtual-key code (a `u32` in the
/// range `0x00..=0xFE`).
#[must_use]
pub const fn key_to_vk(key: KeyCode) -> u32 {
    match key {
        KeyCode::Letter(letter) => letter.as_u8() as u32,
        KeyCode::BracketLeft => 0xDB,  // VK_OEM_4
        KeyCode::BracketRight => 0xDD, // VK_OEM_6
        KeyCode::Minus => 0xBD,        // VK_OEM_MINUS
        KeyCode::Equal => 0xBB,        // VK_OEM_PLUS
        KeyCode::Arrow(Direction::Up) => 0x26,
        KeyCode::Arrow(Direction::Down) => 0x28,
        KeyCode::Arrow(Direction::Left) => 0x25,
        KeyCode::Arrow(Direction::Right) => 0x27,
    }
}
