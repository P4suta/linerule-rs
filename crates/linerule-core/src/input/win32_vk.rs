//! Map [`ChordSpec`] to the `(modifiers, vk)` pair `RegisterHotKey` expects.
//!
//! Kept in `linerule-core` (no `windows` dep) so it is testable off-Windows.

use crate::input::chord::{ChordSpec, Direction, KeyCode, Letter, Modifiers};

/// `RegisterHotKey` `fsModifiers` flag for the `Alt` key.
pub const MOD_ALT: u32 = 0x0001;
/// `RegisterHotKey` `fsModifiers` flag for the `Ctrl` key.
pub const MOD_CONTROL: u32 = 0x0002;
/// `RegisterHotKey` `fsModifiers` flag for the `Shift` key.
pub const MOD_SHIFT: u32 = 0x0004;
/// `RegisterHotKey` `fsModifiers` flag for the `Win` key.
pub const MOD_WIN: u32 = 0x0008;

/// Translate a [`ChordSpec`] into the `(fsModifiers, vk)` pair for `RegisterHotKey`.
///
/// # Examples
///
/// ```
/// use linerule_core::{
///     ChordSpec, KeyCode, Letter, Modifiers, MOD_ALT, MOD_CONTROL, chord_to_win32,
/// };
/// let chord = ChordSpec::new(
///     Modifiers::CTRL | Modifiers::ALT,
///     KeyCode::Letter(Letter::from_ascii(b'R').unwrap()),
/// );
/// let (mods, vk) = chord_to_win32(chord);
/// assert_eq!(mods, MOD_CONTROL | MOD_ALT);
/// assert_eq!(vk, 0x52); // ASCII 'R'
/// ```
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

/// Translate a [`KeyCode`] into its Win32 virtual-key code (`0x00..=0xFE`).
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

/// Translate `RegisterHotKey` modifier flags and a supported virtual key back
/// into the canonical owned chord representation.
#[must_use]
pub fn chord_from_win32(modifiers: u32, vk: u32) -> Option<ChordSpec> {
    // The Win32 modifier constants are distinct one-bit masks. Addition makes
    // that invariant explicit and avoids an OR/XOR-equivalent mutation.
    let known_modifiers = MOD_ALT + MOD_CONTROL + MOD_SHIFT + MOD_WIN;
    if modifiers & !known_modifiers != 0 {
        return None;
    }
    let parsed_modifiers = [
        (MOD_CONTROL, Modifiers::CTRL),
        (MOD_ALT, Modifiers::ALT),
        (MOD_SHIFT, Modifiers::SHIFT),
        (MOD_WIN, Modifiers::META),
    ]
    .into_iter()
    .filter_map(|(mask, parsed)| (modifiers & mask != 0).then_some(parsed))
    .fold(Modifiers::empty(), Modifiers::union);
    let key = match vk {
        0x41..=0x5A => KeyCode::Letter(Letter::from_ascii(u8::try_from(vk).ok()?)?),
        0xDB => KeyCode::BracketLeft,
        0xDD => KeyCode::BracketRight,
        0xBD => KeyCode::Minus,
        0xBB => KeyCode::Equal,
        0x26 => KeyCode::Arrow(Direction::Up),
        0x28 => KeyCode::Arrow(Direction::Down),
        0x25 => KeyCode::Arrow(Direction::Left),
        0x27 => KeyCode::Arrow(Direction::Right),
        _ => return None,
    };
    Some(ChordSpec::new(parsed_modifiers, key))
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
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

    #[test]
    fn reverse_mapping_round_trips_every_supported_key_and_modifier() {
        for bits in 0u8..16 {
            let modifiers = Modifiers::from_bits_truncate(bits);
            for key in [
                KeyCode::Letter(letter(b'A')),
                KeyCode::BracketLeft,
                KeyCode::BracketRight,
                KeyCode::Minus,
                KeyCode::Equal,
                KeyCode::Arrow(Direction::Up),
                KeyCode::Arrow(Direction::Down),
                KeyCode::Arrow(Direction::Left),
                KeyCode::Arrow(Direction::Right),
            ] {
                let chord = ChordSpec::new(modifiers, key);
                let (win32_modifiers, vk) = chord_to_win32(chord);
                assert_eq!(chord_from_win32(win32_modifiers, vk), Some(chord));
            }
        }
    }

    #[test]
    fn reverse_mapping_rejects_unknown_keys_and_modifier_bits() {
        assert_eq!(chord_from_win32(0, 0), None);
        assert_eq!(chord_from_win32(0x1000, 0x41), None);
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
