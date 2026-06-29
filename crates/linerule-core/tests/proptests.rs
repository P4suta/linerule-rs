//! Property-based invariant tests for `linerule-core`.

// Integration tests sit outside `#[cfg(test)]`, so clippy's
// `allow-expect-in-tests` does not apply; every `expect` here is behind a
// constraining generator, so `None` is unreachable.
#![allow(
    clippy::expect_used,
    reason = "integration-test file; constrained generators make None unreachable"
)]

use linerule_core::input::chord::{ChordSpec, Direction, KeyCode, Letter, Modifiers};
use linerule_core::input::win32_vk::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, chord_to_win32};
use linerule_core::{
    ActiveMode, Mode, Opacity, OverlayAction, RejectReason, State, Thickness, input::chord,
    state::reduce,
};
use proptest::prelude::*;

fn any_mode() -> impl Strategy<Value = Mode> {
    prop_oneof![
        Just(Mode::Off),
        Just(Mode::Horizontal),
        Just(Mode::Vertical),
    ]
}

fn any_active_mode() -> impl Strategy<Value = ActiveMode> {
    prop_oneof![Just(ActiveMode::Horizontal), Just(ActiveMode::Vertical)]
}

/// State generator: when `mode` is active, `last_active` must agree with it;
/// free only while `Off`.
fn any_state() -> impl Strategy<Value = State> {
    (any_mode(), any_active_mode()).prop_map(|(mode, generated_last)| State {
        mode,
        last_active: mode.active().unwrap_or(generated_last),
        ..State::DEFAULT
    })
}

proptest! {
    /// `CycleMode` twice is identity on the mode field.
    #[test]
    fn cycle_mode_has_period_two(mode in any_mode()) {
        let s = State { mode, ..State::DEFAULT };
        let (a, _) = reduce::apply(s, OverlayAction::CycleMode);
        let (b, _) = reduce::apply(a, OverlayAction::CycleMode);
        prop_assert_eq!(b.mode, mode);
    }

    /// `ToggleOnOff` twice is identity on the full state.
    #[test]
    fn toggle_on_off_twice_is_identity(state in any_state()) {
        let (a, _) = reduce::apply(state, OverlayAction::ToggleOnOff);
        let (b, _) = reduce::apply(a, OverlayAction::ToggleOnOff);
        prop_assert_eq!(b, state);
    }

    /// Every action preserves the active-mode/`last_active` invariant.
    #[test]
    fn invariant_preserved_by_every_action(state in any_state(), action in any_action()) {
        let (next, _) = reduce::apply(state, action);
        if let Some(active) = next.mode.active() {
            prop_assert_eq!(next.last_active, active);
        }
    }

    /// `BumpThickness` while `Off` leaves state untouched and reports a rejection.
    #[test]
    fn bump_thickness_is_rejected_in_off_mode(delta in -1024_i32..1024) {
        let s = State { mode: Mode::Off, ..State::DEFAULT };
        let (next, d) = reduce::apply(s, OverlayAction::BumpThickness(delta));
        prop_assert_eq!(next, s);
        prop_assert!(!d.is_any());
        prop_assert_eq!(d.rejected, Some(RejectReason::AdjustWhileOff));
    }

    /// Every adjustment action is rejected while `Off`.
    #[test]
    fn off_adjustments_are_rejected(
        last in any_active_mode(),
        delta in -1024_i32..1024,
        which in 0_u8..4,
    ) {
        let s = State { mode: Mode::Off, last_active: last, ..State::DEFAULT };
        let action = match which {
            0 => OverlayAction::BumpThickness(delta),
            1 => OverlayAction::BumpOpacity(delta),
            2 => OverlayAction::CycleEffect,
            _ => OverlayAction::CycleMode,
        };
        let (next, d) = reduce::apply(s, action);
        prop_assert_eq!(next, s);
        prop_assert_eq!(d.rejected, Some(RejectReason::AdjustWhileOff));
    }

    /// In an active mode no action is ever rejected.
    #[test]
    fn active_adjustments_never_rejected(state in any_state(), action in any_action()) {
        if matches!(state.mode, Mode::Off) {
            return Ok(());
        }
        let (_, d) = reduce::apply(state, action);
        prop_assert_eq!(d.rejected, None);
    }

    /// A rejection never changes state.
    #[test]
    fn rejected_implies_state_unchanged(state in any_state(), action in any_action()) {
        let (next, d) = reduce::apply(state, action);
        if d.rejected.is_some() {
            prop_assert_eq!(next, state, "rejected action must leave state untouched");
            prop_assert!(!d.is_any(), "rejection and state delta are mutually exclusive");
        }
    }

    /// `BumpThickness` in an active mode touches only thickness, never mode or restore target.
    #[test]
    fn bump_thickness_only_touches_config(state in any_state(), delta in -1024_i32..1024) {
        if matches!(state.mode, Mode::Off) {
            return Ok(());
        }
        let (next, _) = reduce::apply(state, OverlayAction::BumpThickness(delta));
        prop_assert_eq!(next.mode, state.mode);
        prop_assert_eq!(next.last_active, state.last_active);
        prop_assert_eq!(next.config.opacity, state.config.opacity);
        prop_assert_eq!(next.config.effect, state.config.effect);
    }

    /// Toggle off then on restores the axis active just before `Off`.
    #[test]
    fn flip_then_toggle_restores_the_flipped_axis(start in any_active_mode()) {
        let s = State::with_mode(Mode::from(start));
        // Flip, toggle off, toggle on: restored mode is the flipped axis.
        let (flipped, _) = reduce::apply(s, OverlayAction::CycleMode);
        let (off, _) = reduce::apply(flipped, OverlayAction::ToggleOnOff);
        prop_assert_eq!(off.mode, Mode::Off);
        let (restored, _) = reduce::apply(off, OverlayAction::ToggleOnOff);
        prop_assert_eq!(restored.mode, flipped.mode);
    }

    /// Opacity saturating arithmetic is monotonic and stays in range.
    #[test]
    fn opacity_saturating_arithmetic_is_bounded(start in 1_u8..=255, delta in -1024_i32..1024) {
        let o = Opacity::try_new(start).unwrap();
        let n = o.saturating_add(delta);
        prop_assert!(n.get() >= 1);
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

    /// `Letter::from_ascii` gives `Some` exactly when the byte is ASCII alphabetic.
    #[test]
    fn letter_from_ascii_is_total(b in any::<u8>()) {
        prop_assert_eq!(Letter::from_ascii(b).is_some(), b.is_ascii_alphabetic());
    }

    /// `Modifiers::contains` agrees with raw bit testing for every 4-bit value.
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

    /// `chord_to_win32` yields a valid non-zero Win32 vk for every `(Modifiers, KeyCode)`.
    #[test]
    fn chord_to_win32_total_over_inputs(
        mod_bits in 0_u8..16_u8,
        key in any_key_code(),
    ) {
        let mods = Modifiers::from_bits_truncate(mod_bits);
        let (m, vk) = chord_to_win32(ChordSpec::new(mods, key));
        // Modifiers are a subset of the four legal flags.
        prop_assert_eq!(m & !(MOD_ALT | MOD_CONTROL | MOD_SHIFT | MOD_WIN), 0);
        // vk: ASCII letter range or specific VK_OEM_* / VK_arrow constants.
        let known = matches!(
            vk,
            0x41..=0x5A | 0xDB | 0xDD | 0xBD | 0xBB | 0x25..=0x28,
        );
        prop_assert!(known, "unexpected vk={vk:#x}");
    }

    /// In Horizontal mode with the cursor inside the monitor, `frame()` emits a full-width band.
    #[test]
    fn horizontal_frame_has_full_width_band(
        x in 0_i32..1920,
        y in 100_i32..980,
    ) {
        use linerule_core::{frame, render::Geometry, OverlayConfig, OverlaySample, ScreenRect, Point};
        let monitor = ScreenRect::new(Point::new(0, 0), 1920, 1080);
        let config = OverlayConfig::DEFAULT;
        let f = frame(
            Mode::Horizontal,
            config,
            Point::new(x, y),
            monitor,
            OverlaySample::settled(config),
        );
        let any_full_width = f.layers().iter().any(|l| match l.geometry {
            Geometry::Rect(r) => r.left() == 0 && r.right() == 1920,
        });
        prop_assert!(any_full_width, "horizontal mode at ({x},{y}) lacks a full-width band");
    }

    /// When `delta.is_any()` is false, the state is unchanged.
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

// helper strategies

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
        Just(OverlayAction::CycleEffect),
        Just(OverlayAction::ToggleOnOff),
        (-1024_i32..1024).prop_map(OverlayAction::BumpThickness),
        (-1024_i32..1024).prop_map(OverlayAction::BumpOpacity),
        Just(OverlayAction::ToggleHudDetail),
        Just(OverlayAction::Quit),
    ]
}

/// Chord parser round-trip on a curated table (random fuzzing would mean
/// re-implementing the parser to generate valid shapes).
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
