//! Property-based tests for `linerule-core`.
//!
//! Each test states an invariant (idempotency, commutativity, round-trip)
//! and asks `proptest` to falsify it across thousands of random inputs.
//! These complement the per-module unit tests by exercising parameter
//! spaces that example-based tests can't enumerate.

use linerule_core::{Mode, Opacity, OverlayAction, State, Thickness, input::chord, state::reduce};
use proptest::prelude::*;

fn any_mode() -> impl Strategy<Value = Mode> {
    prop_oneof![
        Just(Mode::Off),
        Just(Mode::Horizontal),
        Just(Mode::Vertical),
    ]
}

fn any_state() -> impl Strategy<Value = State> {
    (any_mode(), any::<bool>()).prop_map(|(mode, visible)| State {
        mode,
        visible,
        ..State::DEFAULT
    })
}

proptest! {
    /// `CycleMode` applied three times is the identity on the mode field.
    #[test]
    fn cycle_mode_has_period_three(mode in any_mode()) {
        let s = State { mode, ..State::DEFAULT };
        let (a, _) = reduce::apply(s, OverlayAction::CycleMode);
        let (b, _) = reduce::apply(a, OverlayAction::CycleMode);
        let (c, _) = reduce::apply(b, OverlayAction::CycleMode);
        prop_assert_eq!(c.mode, mode);
    }

    /// `ToggleVisible` applied twice is the identity on the visibility field.
    #[test]
    fn toggle_visible_is_involutive(state in any_state()) {
        let (a, _) = reduce::apply(state, OverlayAction::ToggleVisible);
        let (b, _) = reduce::apply(a, OverlayAction::ToggleVisible);
        prop_assert_eq!(b.visible, state.visible);
    }

    /// `BumpThickness` is a no-op while the mode is `Off`.
    #[test]
    fn bump_thickness_is_inert_in_off_mode(delta in -1024_i32..1024) {
        let s = State { mode: Mode::Off, ..State::DEFAULT };
        let (next, d) = reduce::apply(s, OverlayAction::BumpThickness(delta));
        prop_assert_eq!(next, s);
        prop_assert!(!d.is_any());
    }

    /// `BumpThickness` in an active mode either changes thickness or saturates
    /// — never returns a different mode or visibility.
    #[test]
    fn bump_thickness_only_touches_config(state in any_state(), delta in -1024_i32..1024) {
        if matches!(state.mode, Mode::Off) {
            return Ok(());
        }
        let (next, _) = reduce::apply(state, OverlayAction::BumpThickness(delta));
        prop_assert_eq!(next.mode, state.mode);
        prop_assert_eq!(next.visible, state.visible);
        prop_assert_eq!(next.config.opacity, state.config.opacity);
        prop_assert_eq!(next.config.mask_color, state.config.mask_color);
    }

    /// Opacity saturating arithmetic is monotonic and stays in range.
    #[test]
    fn opacity_saturating_arithmetic_is_bounded(start in 1_u8..=255, delta in -1024_i32..1024) {
        let o = Opacity::try_new(start).unwrap();
        let n = o.saturating_add(delta);
        prop_assert!(n.get() >= 1);
        // No precondition on whether the result equals MIN/MAX; only that
        // it lives inside the legal range and behaves monotonically.
        if delta > 0 {
            prop_assert!(n.get() >= o.get());
        } else if delta < 0 {
            prop_assert!(n.get() <= o.get());
        }
    }

    /// Thickness saturating arithmetic stays in `[MIN, MAX]`.
    #[test]
    fn thickness_saturating_arithmetic_is_bounded(start in 1_u16..=2048, delta in -10_000_i32..10_000) {
        let t = Thickness::try_new(start).unwrap();
        let n = t.saturating_add(delta);
        prop_assert!(n.get() >= Thickness::MIN.get());
        prop_assert!(n.get() <= Thickness::MAX.get());
    }
}

/// Chord parser round-trip on a curated table. (Random ASCII fuzzing would
/// require generating valid chord shapes, which is essentially re-implementing
/// the parser — these examples cover the canonical surface.)
#[test]
fn chord_parser_round_trips_on_known_chords() {
    let cases = [
        "Ctrl+Alt+R",
        "Shift+Up",
        "Ctrl+=",
        "Meta+Q",
        "Ctrl+Alt+[",
        "Ctrl+Alt+]",
        "Shift+Down",
        "Ctrl+Shift+A",
    ];
    for input in cases {
        let parsed = chord::parse(input).expect(input);
        let printed = parsed.display();
        let reparsed = chord::parse(&printed).expect(&printed);
        assert_eq!(parsed, reparsed, "round-trip failed for {input}");
    }
}
