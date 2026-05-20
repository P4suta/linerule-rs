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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::chord::Letter;

    fn letter(b: u8) -> Letter {
        Letter::from_ascii(b).expect("ASCII letter")
    }

    fn spec(modifiers: Modifiers, key: KeyCode) -> ChordSpec {
        ChordSpec::new(modifiers, key)
    }

    // ---- key_to_vk -------------------------------------------------------

    #[test]
    fn letter_a_through_z_uppercase_maps_to_0x41_through_0x5a() {
        for b in b'A'..=b'Z' {
            let vk = key_to_vk(KeyCode::Letter(letter(b)));
            assert_eq!(vk, u32::from(b), "letter {} → vk {:#x}", b as char, vk);
        }
    }

    #[test]
    fn letter_a_through_z_lowercase_folds_to_uppercase_vk() {
        for b in b'a'..=b'z' {
            // Lowercase folds to uppercase: b'a' (0x61) → 0x41, etc.
            let vk = key_to_vk(KeyCode::Letter(letter(b)));
            assert_eq!(
                vk,
                u32::from(b - 32),
                "lowercase {} folds to uppercase VK {:#x}",
                b as char,
                vk
            );
        }
    }

    #[test]
    fn punctuation_keys_map_to_vk_oem() {
        assert_eq!(key_to_vk(KeyCode::BracketLeft), 0xDB);
        assert_eq!(key_to_vk(KeyCode::BracketRight), 0xDD);
        assert_eq!(key_to_vk(KeyCode::Minus), 0xBD);
        assert_eq!(key_to_vk(KeyCode::Equal), 0xBB);
    }

    #[test]
    fn arrow_keys_map_to_vk_arrow_table() {
        // The Win32 docs ordering is Left=0x25, Up=0x26, Right=0x27, Down=0x28.
        assert_eq!(key_to_vk(KeyCode::Arrow(Direction::Left)), 0x25);
        assert_eq!(key_to_vk(KeyCode::Arrow(Direction::Up)), 0x26);
        assert_eq!(key_to_vk(KeyCode::Arrow(Direction::Right)), 0x27);
        assert_eq!(key_to_vk(KeyCode::Arrow(Direction::Down)), 0x28);
    }

    // ---- chord_to_win32 --------------------------------------------------

    #[test]
    fn no_modifier_yields_zero_mods() {
        let (mods, _) = chord_to_win32(spec(Modifiers::empty(), KeyCode::Letter(letter(b'A'))));
        assert_eq!(mods, 0);
    }

    #[test]
    fn each_modifier_maps_to_its_win32_flag() {
        let (m, _) = chord_to_win32(spec(Modifiers::ALT, KeyCode::Letter(letter(b'A'))));
        assert_eq!(m, MOD_ALT);
        let (m, _) = chord_to_win32(spec(Modifiers::CTRL, KeyCode::Letter(letter(b'A'))));
        assert_eq!(m, MOD_CONTROL);
        let (m, _) = chord_to_win32(spec(Modifiers::SHIFT, KeyCode::Letter(letter(b'A'))));
        assert_eq!(m, MOD_SHIFT);
        let (m, _) = chord_to_win32(spec(Modifiers::META, KeyCode::Letter(letter(b'A'))));
        assert_eq!(m, MOD_WIN);
    }

    #[test]
    fn all_sixteen_modifier_combinations_produce_correct_flag_set() {
        // Enumerate every possible (CTRL? ALT? SHIFT? META?) combination and
        // assert that the OR'd MOD_* flags match exactly. This is the bit
        // invariant we rely on at the RegisterHotKey boundary.
        for bits in 0u8..16u8 {
            let mods = Modifiers::from_bits_truncate(bits);
            let expected = (u32::from(mods.contains(Modifiers::ALT)) * MOD_ALT)
                | (u32::from(mods.contains(Modifiers::CTRL)) * MOD_CONTROL)
                | (u32::from(mods.contains(Modifiers::SHIFT)) * MOD_SHIFT)
                | (u32::from(mods.contains(Modifiers::META)) * MOD_WIN);
            let (got, _) = chord_to_win32(spec(mods, KeyCode::Letter(letter(b'A'))));
            assert_eq!(got, expected, "modifiers {bits:#b}");
        }
    }

    #[test]
    fn ctrl_alt_r_matches_default_cycle_mode_chord() {
        let (mods, vk) = chord_to_win32(spec(
            Modifiers::CTRL | Modifiers::ALT,
            KeyCode::Letter(letter(b'R')),
        ));
        assert_eq!(mods, MOD_CONTROL | MOD_ALT);
        assert_eq!(vk, 0x52); // 'R'
    }

    // ---- Letter sanity ---------------------------------------------------

    #[test]
    fn letter_from_ascii_rejects_non_letters() {
        for b in 0u8..=255 {
            let v = Letter::from_ascii(b);
            assert_eq!(v.is_some(), b.is_ascii_alphabetic(), "byte {b:#x}");
        }
    }

    #[test]
    fn letter_as_u8_is_always_uppercase_ascii() {
        for b in b'a'..=b'z' {
            let l = Letter::from_ascii(b).unwrap();
            assert!(
                l.as_u8().is_ascii_uppercase(),
                "lowercase {} folded to non-uppercase {:#x}",
                b as char,
                l.as_u8()
            );
        }
    }
}
