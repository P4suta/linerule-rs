//! Property-based tests for `linerule-core`.
//!
//! Each test states an invariant (idempotency, commutativity, round-trip)
//! and asks `proptest` to falsify it across thousands of random inputs.
//! These complement the per-module unit tests by exercising parameter
//! spaces that example-based tests can't enumerate.

// Integration tests live outside a `#[cfg(test)]` module, so clippy's
// `allow-expect-in-tests` setting does not apply. Allow `expect` here
// explicitly: every use sits behind a generator that constrains its input,
// so the `None` case is provably unreachable.
#![allow(
    clippy::expect_used,
    reason = "integration-test file; constrained generators make None unreachable"
)]

use linerule_core::input::chord::{ChordSpec, Direction, KeyCode, Letter, Modifiers};
use linerule_core::input::win32_vk::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, chord_to_win32};
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

    /// `Letter::from_ascii` is total over `u8` and gives `Some` exactly when
    /// the byte is ASCII alphabetic.
    #[test]
    fn letter_from_ascii_is_total(b in any::<u8>()) {
        prop_assert_eq!(Letter::from_ascii(b).is_some(), b.is_ascii_alphabetic());
    }

    /// `Modifiers` bitset `contains` agrees with raw bit testing for every
    /// 4-bit value.
    #[test]
    fn modifiers_contains_agrees_with_raw_bits(bits in 0_u8..16_u8) {
        let mods = Modifiers::from_bits_truncate(bits);
        for (flag, mask) in [
            (Modifiers::CTRL,  1_u8 << 0),
            (Modifiers::ALT,   1_u8 << 1),
            (Modifiers::SHIFT, 1_u8 << 2),
            (Modifiers::META,  1_u8 << 3),
        ] {
            prop_assert_eq!(mods.contains(flag), (bits & mask) != 0);
        }
    }

    /// `chord_to_win32` is total over the full `(Modifiers, KeyCode)` product:
    /// the result `vk` is always a non-zero, valid Win32 virtual-key code.
    #[test]
    fn chord_to_win32_total_over_inputs(
        mod_bits in 0_u8..16_u8,
        key in any_key_code(),
    ) {
        let mods = Modifiers::from_bits_truncate(mod_bits);
        let (m, vk) = chord_to_win32(ChordSpec::new(mods, key));
        // Modifiers must be a subset of the four legal flags.
        prop_assert_eq!(m & !(MOD_ALT | MOD_CONTROL | MOD_SHIFT | MOD_WIN), 0);
        // VK must be a documented value: ASCII letter range or specific
        // VK_OEM_* / VK_arrow constants.
        let known = matches!(
            vk,
            0x41..=0x5A | 0xDB | 0xDD | 0xBD | 0xBB | 0x25..=0x28,
        );
        prop_assert!(known, "unexpected vk={vk:#x}");
    }

    /// `frame()` always emits one full-width band (above or below the slit)
    /// when in Horizontal mode and the cursor is inside the monitor.
    #[test]
    fn horizontal_frame_has_full_width_band(
        x in 0_i32..1920,
        y in 100_i32..980,
    ) {
        use linerule_core::{frame, render::Geometry, ScreenRect, Point};
        let s = State { mode: Mode::Horizontal, ..State::DEFAULT };
        let monitor = ScreenRect::new(Point::new(0, 0), 1920, 1080);
        let f = frame(s, Point::new(x, y), monitor);
        let any_full_width = f.layers().iter().any(|l| match l.geometry {
            Geometry::Rect(r) => r.left() == 0 && r.right() == 1920,
        });
        prop_assert!(any_full_width, "horizontal mode at ({x},{y}) lacks a full-width band");
    }

    /// `reduce::apply`'s returned delta accurately reports whether the next
    /// state differs from the previous one. If `delta.is_any()` is false, the
    /// state must be unchanged.
    #[test]
    fn reduce_delta_implies_state_change(
        state in any_state(),
        action in any_action(),
    ) {
        let (next, d) = reduce::apply(state, action);
        if !d.is_any() {
            prop_assert_eq!(next, state, "delta said nothing changed but state did");
        }
    }
}

// ---- helper strategies -------------------------------------------------

fn any_key_code() -> impl Strategy<Value = KeyCode> {
    prop_oneof![
        (b'A'..=b'Z')
            .prop_map(|b| KeyCode::Letter(Letter::from_ascii(b).expect("uppercase ASCII letter"))),
        Just(KeyCode::BracketLeft),
        Just(KeyCode::BracketRight),
        Just(KeyCode::Minus),
        Just(KeyCode::Equal),
        Just(KeyCode::Arrow(Direction::Up)),
        Just(KeyCode::Arrow(Direction::Down)),
        Just(KeyCode::Arrow(Direction::Left)),
        Just(KeyCode::Arrow(Direction::Right)),
    ]
}

fn any_action() -> impl Strategy<Value = OverlayAction> {
    prop_oneof![
        Just(OverlayAction::CycleMode),
        Just(OverlayAction::ToggleVisible),
        (-1024_i32..1024).prop_map(OverlayAction::BumpThickness),
        (-1024_i32..1024).prop_map(OverlayAction::BumpOpacity),
        Just(OverlayAction::Quit),
    ]
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
