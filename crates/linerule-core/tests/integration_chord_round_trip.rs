//! Integration: `HotkeyMap::DEFAULT` round-trip through chord parser and
//! `KeyCode → VK` mapping.
//!
//! Catches the regression class that bit us at v0.2.x release: every chord
//! the binary registers at startup must actually be parseable *and* every
//! resulting VK must be a non-zero Win32 virtual-key code. If either fails,
//! `RegisterHotKey` rejects the chord at runtime and the overlay silently
//! refuses to listen for that action.

use linerule_core::HotkeyMap;
use linerule_core::input::chord;
use linerule_core::input::win32_vk::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, chord_to_win32};

/// Every default chord must parse and produce a non-zero VK.
#[test]
fn every_default_chord_parses_and_produces_nonzero_vk() {
    let map = HotkeyMap::DEFAULT;
    let cases: [(&str, &str); 7] = [
        ("cycle_mode", map.cycle_mode),
        ("toggle_visible", map.toggle_visible),
        ("thicker", map.thicker),
        ("thinner", map.thinner),
        ("more_opaque", map.more_opaque),
        ("less_opaque", map.less_opaque),
        ("quit", map.quit),
    ];
    for (name, spec) in cases {
        let parsed = chord::parse(spec)
            .unwrap_or_else(|e| panic!("default chord `{name}` = `{spec}` failed to parse: {e}"));
        let (mods, vk) = chord_to_win32(parsed);
        assert_ne!(
            vk, 0,
            "{name} `{spec}`: vk must be non-zero (RegisterHotKey rejects vk=0)"
        );
        // All default chords use Ctrl+Alt; assert that at minimum so a slip
        // in HotkeyMap::DEFAULT (e.g. dropping a modifier) is caught here.
        assert!(
            mods & MOD_CONTROL != 0,
            "{name}: expected Ctrl in modifier set, got {mods:#x}"
        );
        assert!(
            mods & MOD_ALT != 0,
            "{name}: expected Alt in modifier set, got {mods:#x}"
        );
        // Modifiers must be a subset of the four legal flags.
        let legal_mask = MOD_ALT | MOD_CONTROL | MOD_SHIFT | MOD_WIN;
        assert_eq!(
            mods & !legal_mask,
            0,
            "{name}: mods set unknown bits ({mods:#x})"
        );
    }
}

/// Round-trip property: `parse(s)` → `display()` → `parse(...)` must yield
/// the same `ChordSpec` for every default chord.
#[test]
fn every_default_chord_display_round_trips() {
    let map = HotkeyMap::DEFAULT;
    for spec in [
        map.cycle_mode,
        map.toggle_visible,
        map.thicker,
        map.thinner,
        map.more_opaque,
        map.less_opaque,
        map.quit,
    ] {
        let parsed = chord::parse(spec).unwrap_or_else(|e| panic!("parse `{spec}`: {e}"));
        let printed = parsed.display();
        let reparsed = chord::parse(&printed)
            .unwrap_or_else(|e| panic!("reparse `{printed}` (from `{spec}`): {e}"));
        assert_eq!(
            parsed, reparsed,
            "round-trip failed: `{spec}` → `{printed}` → ChordSpec differs"
        );
    }
}

/// Distinct chord keys must produce distinct (mods, vk) pairs so that the
/// hotkey host can disambiguate them.
#[test]
fn default_chords_are_pairwise_distinct() {
    let map = HotkeyMap::DEFAULT;
    let labeled = [
        ("cycle_mode", map.cycle_mode),
        ("toggle_visible", map.toggle_visible),
        ("thicker", map.thicker),
        ("thinner", map.thinner),
        ("more_opaque", map.more_opaque),
        ("less_opaque", map.less_opaque),
        ("quit", map.quit),
    ];
    let mut keys: Vec<(&str, (u32, u32))> = labeled
        .iter()
        .map(|(name, s)| {
            (
                *name,
                chord_to_win32(chord::parse(s).expect("default parses")),
            )
        })
        .collect();
    keys.sort_by_key(|(_, k)| *k);
    for w in keys.windows(2) {
        assert_ne!(
            w[0].1, w[1].1,
            "duplicate (mods, vk) for `{}` and `{}`",
            w[0].0, w[1].0
        );
    }
}
