//! Default shortcut parsing and Win32 mapping contract.

#![allow(
    clippy::expect_used,
    reason = "integration test asserts the completeness of built-in shortcut defaults"
)]

use linerule_core::{
    ChordError, Command, Direction, HotkeyBindings, KeyCode, MOD_ALT, MOD_CONTROL, MOD_SHIFT,
    MOD_WIN, chord_to_win32, parse_chord,
};

fn defaults() -> Vec<(Command, String)> {
    let bindings = HotkeyBindings::default();
    Command::ALL
        .into_iter()
        .map(|command| {
            (
                command,
                bindings.get(command).expect("complete defaults").to_owned(),
            )
        })
        .collect()
}

#[test]
fn every_default_chord_parses_and_produces_nonzero_vk() {
    for (command, spec) in defaults() {
        let parsed = parse_chord(&spec).expect("default chord parses");
        let (modifiers, vk) = chord_to_win32(parsed);
        assert_ne!(vk, 0, "{command:?} `{spec}` has vk=0");
        assert_ne!(modifiers & MOD_CONTROL, 0, "{command:?} must use Ctrl");
        assert_ne!(modifiers & MOD_ALT, 0, "{command:?} must use Alt");
        let legal = MOD_ALT | MOD_CONTROL | MOD_SHIFT | MOD_WIN;
        assert_eq!(modifiers & !legal, 0, "{command:?} has unknown bits");
    }
}

#[test]
fn every_default_chord_display_round_trips() {
    for (_, spec) in defaults() {
        let parsed = parse_chord(&spec).expect("default parses");
        let printed = parsed.display();
        assert_eq!(parse_chord(&printed).expect("display reparses"), parsed);
    }
}

#[test]
fn bump_defaults_are_layout_independent_arrow_keys() {
    let bindings = HotkeyBindings::default();
    for (command, expected) in [
        (Command::Thicker, KeyCode::Arrow(Direction::Up)),
        (Command::Thinner, KeyCode::Arrow(Direction::Down)),
        (Command::MoreOpaque, KeyCode::Arrow(Direction::Right)),
        (Command::LessOpaque, KeyCode::Arrow(Direction::Left)),
    ] {
        let parsed = parse_chord(bindings.get(command).expect("default exists")).expect("parses");
        assert_eq!(parsed.key, expected, "{command:?}");
    }
}

#[test]
fn default_chords_are_pairwise_distinct() {
    let mut mapped = defaults()
        .into_iter()
        .map(|(command, spec)| {
            (
                command,
                chord_to_win32(parse_chord(&spec).expect("default parses")),
            )
        })
        .collect::<Vec<_>>();
    mapped.sort_by_key(|(_, key)| *key);
    for adjacent in mapped.windows(2) {
        assert_ne!(
            adjacent[0].1, adjacent[1].1,
            "duplicate defaults: {:?} and {:?}",
            adjacent[0].0, adjacent[1].0
        );
    }
}

#[test]
fn malformed_chord_boundaries_return_typed_errors() {
    assert!(matches!(parse_chord("  "), Err(ChordError::Empty)));
    assert!(matches!(
        parse_chord("Ctrl++R"),
        Err(ChordError::EmptyToken { position: 1 })
    ));
    assert!(matches!(
        parse_chord("Ctrl+1"),
        Err(ChordError::UnknownPart { .. })
    ));
    assert!(matches!(
        parse_chord("Ctrl+R+H"),
        Err(ChordError::MultipleKeys { .. })
    ));
    assert!(matches!(parse_chord("Ctrl+Alt"), Err(ChordError::NoKey)));
}
