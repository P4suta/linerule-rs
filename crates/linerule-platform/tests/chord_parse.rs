//! Exhaustive tests for the cross-platform chord parser.
//!
//! Runs on every host (Linux dev container included) — `chord::parse`
//! is OS-independent by design. The Windows adapter
//! (`spec_to_hotkey`) is not exercised here; that's covered by the
//! Windows-only smoke tests in CI.

use linerule_platform::chord::{ChordError, ChordSpec, KeyCode, Modifiers, parse};

/// Helper that takes a `(ctrl, alt, shift, meta)` tuple instead of
/// four separate booleans so clippy's `fn_params_excessive_bools`
/// stays satisfied.
fn mods((ctrl, alt, shift, meta): (bool, bool, bool, bool)) -> Modifiers {
    Modifiers {
        ctrl,
        alt,
        shift,
        meta,
    }
}

fn spec(modifiers: Modifiers, key: KeyCode) -> ChordSpec {
    ChordSpec { modifiers, key }
}

// ---------------------------------------------------------------------------
// happy-path: every shape we ship as a default
// ---------------------------------------------------------------------------

#[test]
fn parses_default_cycle_chord() {
    assert_eq!(
        parse("Ctrl+Alt+R").unwrap(),
        spec(mods((true, true, false, false)), KeyCode::Letter(b'R')),
    );
}

#[test]
fn parses_default_quit_chord() {
    assert_eq!(
        parse("Ctrl+Alt+Q").unwrap(),
        spec(mods((true, true, false, false)), KeyCode::Letter(b'Q')),
    );
}

#[test]
fn parses_bracket_left() {
    assert_eq!(
        parse("Ctrl+Alt+[").unwrap(),
        spec(mods((true, true, false, false)), KeyCode::BracketLeft),
    );
}

#[test]
fn parses_bracket_right() {
    assert_eq!(
        parse("Ctrl+Alt+]").unwrap(),
        spec(mods((true, true, false, false)), KeyCode::BracketRight),
    );
}

#[test]
fn parses_minus() {
    assert_eq!(
        parse("Ctrl+Alt+-").unwrap(),
        spec(mods((true, true, false, false)), KeyCode::Minus),
    );
}

#[test]
fn parses_equal() {
    assert_eq!(
        parse("Ctrl+Alt+=").unwrap(),
        spec(mods((true, true, false, false)), KeyCode::Equal),
    );
}

#[test]
fn parses_arrow_keys_via_aliases() {
    assert_eq!(parse("Ctrl+Alt+Up").unwrap().key, KeyCode::ArrowUp,);
    assert_eq!(parse("Ctrl+Alt+ArrowDown").unwrap().key, KeyCode::ArrowDown,);
    assert_eq!(parse("Ctrl+Alt+left").unwrap().key, KeyCode::ArrowLeft,);
    assert_eq!(
        parse("Ctrl+Alt+ARROWRIGHT").unwrap().key,
        KeyCode::ArrowRight,
    );
}

// ---------------------------------------------------------------------------
// case-folding & whitespace tolerance
// ---------------------------------------------------------------------------

#[test]
fn lowercase_modifiers_accepted() {
    assert_eq!(
        parse("ctrl+alt+r").unwrap(),
        spec(mods((true, true, false, false)), KeyCode::Letter(b'R')),
    );
}

#[test]
fn uppercase_modifiers_accepted() {
    assert_eq!(
        parse("CTRL+ALT+R").unwrap(),
        spec(mods((true, true, false, false)), KeyCode::Letter(b'R')),
    );
}

#[test]
fn whitespace_around_parts_tolerated() {
    assert_eq!(
        parse("  Ctrl + Alt + R  ").unwrap(),
        spec(mods((true, true, false, false)), KeyCode::Letter(b'R')),
    );
}

#[test]
fn modifier_aliases_normalise() {
    // control == ctrl
    assert_eq!(
        parse("control+alt+r").unwrap().modifiers,
        mods((true, true, false, false)),
    );
    // option == alt (macOS-style)
    assert_eq!(
        parse("option+r").unwrap().modifiers,
        mods((false, true, false, false))
    );
    // win / cmd / meta / super are all "meta"
    assert_eq!(
        parse("win+r").unwrap().modifiers,
        mods((false, false, false, true))
    );
    assert_eq!(
        parse("cmd+r").unwrap().modifiers,
        mods((false, false, false, true))
    );
    assert_eq!(
        parse("super+r").unwrap().modifiers,
        mods((false, false, false, true))
    );
    assert_eq!(
        parse("meta+r").unwrap().modifiers,
        mods((false, false, false, true))
    );
}

// ---------------------------------------------------------------------------
// modifier set permutations
// ---------------------------------------------------------------------------

#[test]
fn no_modifiers_is_legal() {
    assert_eq!(
        parse("R").unwrap(),
        spec(mods((false, false, false, false)), KeyCode::Letter(b'R'))
    );
}

#[test]
fn shift_alone_is_legal() {
    assert_eq!(
        parse("Shift+H").unwrap(),
        spec(mods((false, false, true, false)), KeyCode::Letter(b'H')),
    );
}

#[test]
fn all_four_modifiers() {
    assert_eq!(
        parse("Ctrl+Alt+Shift+Meta+R").unwrap().modifiers,
        mods((true, true, true, true)),
    );
}

#[test]
fn duplicate_modifiers_collapse() {
    assert_eq!(
        parse("Ctrl+Ctrl+Ctrl+R").unwrap().modifiers,
        mods((true, false, false, false)),
    );
}

// ---------------------------------------------------------------------------
// every letter is reachable
// ---------------------------------------------------------------------------

#[test]
fn every_ascii_letter_round_trips() {
    for byte in b'A'..=b'Z' {
        let chord = format!("Ctrl+{}", byte as char);
        let parsed = parse(&chord).unwrap_or_else(|e| panic!("{chord:?} should parse: {e}"));
        assert_eq!(
            parsed.key,
            KeyCode::Letter(byte),
            "letter {} mismatch",
            byte as char
        );
    }
}

#[test]
fn lowercase_letters_canonicalise_to_uppercase() {
    assert_eq!(parse("ctrl+r").unwrap().key, KeyCode::Letter(b'R'));
    assert_eq!(parse("ctrl+a").unwrap().key, KeyCode::Letter(b'A'));
    assert_eq!(parse("ctrl+z").unwrap().key, KeyCode::Letter(b'Z'));
}

// ---------------------------------------------------------------------------
// failure modes
// ---------------------------------------------------------------------------

#[test]
fn empty_string_is_error() {
    assert_eq!(parse(""), Err(ChordError::Empty));
    assert_eq!(parse("   "), Err(ChordError::Empty));
}

#[test]
fn modifiers_without_key_is_error() {
    assert!(matches!(parse("Ctrl+Alt"), Err(ChordError::NoKey(_))));
    assert!(matches!(parse("Shift"), Err(ChordError::NoKey(_))));
}

#[test]
fn dangling_separator_is_tolerated() {
    // Trailing `+` produces an empty part which the parser skips.
    assert_eq!(parse("Ctrl+Alt+R+").unwrap().key, KeyCode::Letter(b'R'));
    // Leading `+` likewise.
    assert_eq!(parse("+Ctrl+R").unwrap().key, KeyCode::Letter(b'R'));
}

#[test]
fn two_main_keys_is_error() {
    assert!(matches!(
        parse("Ctrl+R+S"),
        Err(ChordError::MultipleKeys(_))
    ));
    assert!(matches!(parse("A+B"), Err(ChordError::MultipleKeys(_))));
}

#[test]
fn unknown_key_is_error_with_part_context() {
    let err = parse("Ctrl+xyz").unwrap_err();
    match err {
        ChordError::UnknownPart { chord, part } => {
            assert_eq!(chord, "Ctrl+xyz");
            assert_eq!(part, "xyz");
        }
        other => panic!("expected UnknownPart, got {other:?}"),
    }
}

#[test]
fn empty_after_split_is_not_an_unknown_part() {
    // The middle `++` should NOT report UnknownPart for "" — the
    // parser's `if p.is_empty() { continue; }` guards this.
    assert_eq!(parse("Ctrl++R").unwrap().key, KeyCode::Letter(b'R'));
}

// ---------------------------------------------------------------------------
// Modifiers::any helper
// ---------------------------------------------------------------------------

#[test]
fn modifiers_any_is_false_for_default() {
    assert!(!Modifiers::default().any());
}

#[test]
fn modifiers_any_is_true_when_any_bit_set() {
    assert!(mods((true, false, false, false)).any());
    assert!(mods((false, true, false, false)).any());
    assert!(mods((false, false, true, false)).any());
    assert!(mods((false, false, false, true)).any());
}
