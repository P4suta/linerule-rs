//! Integration: `HotkeyMap::DEFAULT` round-trips through the chord parser and
//! `KeyCode → VK` mapping. Every registered chord must parse and yield a
//! non-zero VK, else `RegisterHotKey` rejects it at runtime.

use linerule_core::HotkeyMap;
use linerule_core::input::chord::{self, Direction, KeyCode};
use linerule_core::input::win32_vk::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, chord_to_win32};

/// Every default chord must parse and produce a non-zero VK.
#[test]
fn every_default_chord_parses_and_produces_nonzero_vk() {
    let map = HotkeyMap::DEFAULT;
    let cases: [(&str, &str); 9] = [
        ("cycle_mode", map.cycle_mode),
        ("cycle_effect", map.cycle_effect),
        ("toggle_on_off", map.toggle_on_off),
        ("thicker", map.thicker),
        ("thinner", map.thinner),
        ("more_opaque", map.more_opaque),
        ("less_opaque", map.less_opaque),
        ("toggle_hud", map.toggle_hud),
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
        // All default chords use Ctrl+Alt; catches a dropped modifier.
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

/// `parse → display → parse` must yield the same `ChordSpec`.
#[test]
fn every_default_chord_display_round_trips() {
    let map = HotkeyMap::DEFAULT;
    for spec in [
        map.cycle_mode,
        map.cycle_effect,
        map.toggle_on_off,
        map.thicker,
        map.thinner,
        map.more_opaque,
        map.less_opaque,
        map.toggle_hud,
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

/// Bump/opacity chords must use arrow keys: OEM keys (`[`/`]`/`=`/`-`) are
/// layout/IME-sensitive on Windows and deliver different VKs (e.g. JIS keyboard
/// × English IME), so `RegisterHotKey` misses.
#[test]
fn bump_and_opacity_default_chords_are_layout_independent_arrow_keys() {
    let map = HotkeyMap::DEFAULT;
    let expected: [(&str, &str, KeyCode); 4] = [
        ("thicker", map.thicker, KeyCode::Arrow(Direction::Up)),
        ("thinner", map.thinner, KeyCode::Arrow(Direction::Down)),
        (
            "more_opaque",
            map.more_opaque,
            KeyCode::Arrow(Direction::Right),
        ),
        (
            "less_opaque",
            map.less_opaque,
            KeyCode::Arrow(Direction::Left),
        ),
    ];
    for (name, spec, expected_key) in expected {
        let parsed = chord::parse(spec).unwrap_or_else(|e| panic!("{name} parse `{spec}`: {e}"));
        assert_eq!(
            parsed.key, expected_key,
            "{name} `{spec}`: expected arrow key {expected_key:?}, got {:?}",
            parsed.key
        );
    }
}

/// Distinct chords must produce distinct (mods, vk) pairs.
#[test]
fn default_chords_are_pairwise_distinct() {
    let map = HotkeyMap::DEFAULT;
    let labeled = [
        ("cycle_mode", map.cycle_mode),
        ("cycle_effect", map.cycle_effect),
        ("toggle_on_off", map.toggle_on_off),
        ("thicker", map.thicker),
        ("thinner", map.thinner),
        ("more_opaque", map.more_opaque),
        ("less_opaque", map.less_opaque),
        ("toggle_hud", map.toggle_hud),
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
