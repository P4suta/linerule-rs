//! Integration: reducer + frame builder composed across multi-step action sequences.

use linerule_core::{
    Mode, OverlayAction, OverlaySample, Point, ScreenRect, State, SurroundEffect, frame,
    state::reduce,
};

fn run(actions: &[OverlayAction]) -> State {
    let mut s = State::DEFAULT;
    for &a in actions {
        let (next, _) = reduce::apply(s, a);
        s = next;
    }
    s
}

const fn monitor() -> ScreenRect<linerule_core::Logical> {
    ScreenRect::new(Point::new(0, 0), 1920, 1080)
}

#[test]
fn cycle_mode_while_off_is_rejected_and_stays_off() {
    let s = run(&[
        OverlayAction::CycleMode,
        OverlayAction::CycleMode,
        OverlayAction::CycleMode,
    ]);
    assert_eq!(
        s.mode,
        Mode::Off,
        "CycleMode must never turn the overlay on"
    );
}

#[test]
fn cycle_mode_twice_while_on_is_the_identity() {
    let s = run(&[
        OverlayAction::ToggleOnOff, // Off → Horizontal
        OverlayAction::CycleMode,   // Horizontal → Vertical
        OverlayAction::CycleMode,   // Vertical → Horizontal
    ]);
    assert_eq!(s.mode, Mode::Horizontal);
}

#[test]
fn toggle_on_then_frame_has_layers() {
    let s = run(&[OverlayAction::ToggleOnOff]);
    assert_eq!(s.mode, Mode::Horizontal);
    let f = frame(
        s.mode,
        s.config,
        Point::new(960, 540),
        monitor(),
        OverlaySample::settled(s.config),
    );
    assert!(!f.is_empty(), "Horizontal mode should produce layers");
}

#[test]
fn toggle_on_off_after_cycle_turns_off_and_frame_is_empty() {
    let s = run(&[
        OverlayAction::ToggleOnOff, // Off → Horizontal
        OverlayAction::CycleMode,   // Horizontal → Vertical
        OverlayAction::ToggleOnOff, // Vertical → Off
    ]);
    assert_eq!(s.mode, Mode::Off);
    let f = frame(
        s.mode,
        s.config,
        Point::new(960, 540),
        monitor(),
        OverlaySample::settled(s.config),
    );
    assert!(f.is_empty(), "Off must produce an empty frame");
}

#[test]
fn toggle_on_off_twice_restores_the_active_mode() {
    let s = run(&[
        OverlayAction::ToggleOnOff, // Off → Horizontal
        OverlayAction::CycleMode,   // Horizontal → Vertical
        OverlayAction::ToggleOnOff, // Vertical → Off (remembers Vertical)
        OverlayAction::ToggleOnOff, // Off → Vertical
    ]);
    assert_eq!(
        s.mode,
        Mode::Vertical,
        "restore must target the last active mode"
    );
}

#[test]
fn bump_thickness_accumulates_with_repeated_application() {
    let start = State {
        mode: Mode::Horizontal,
        ..State::DEFAULT
    };
    let (after_one, _) = reduce::apply(start, OverlayAction::BumpThickness(8));
    let (after_two, _) = reduce::apply(after_one, OverlayAction::BumpThickness(8));
    assert!(
        after_two.config.thickness.get() > after_one.config.thickness.get(),
        "second bump should keep growing (or saturate); got {} ≤ {}",
        after_two.config.thickness.get(),
        after_one.config.thickness.get(),
    );
}

#[test]
fn bump_then_undo_returns_to_starting_thickness() {
    let start = State::with_mode(Mode::Horizontal);
    // ToggleOnOff first: bumps are rejected while Off, so enter an active mode.
    let s = run(&[
        OverlayAction::ToggleOnOff,
        OverlayAction::BumpThickness(8),
        OverlayAction::BumpThickness(-8),
    ]);
    assert_eq!(
        s.config.thickness.get(),
        start.config.thickness.get(),
        "bump + reverse bump should be an identity on thickness"
    );
}

#[test]
fn cycle_effect_walks_surround_then_returns_when_mode_is_active() {
    // CycleEffect only acts while a mode is on (mirrors the bump actions).
    let start = State {
        mode: Mode::Horizontal,
        ..State::DEFAULT
    };
    assert_eq!(start.config.effect, SurroundEffect::DimBlack);
    let (after_one, _) = reduce::apply(start, OverlayAction::CycleEffect);
    assert_eq!(after_one.config.effect, SurroundEffect::WhiteWash);
    let (after_two, _) = reduce::apply(after_one, OverlayAction::CycleEffect);
    assert_eq!(after_two.config.effect, SurroundEffect::Blur);
    let (after_three, _) = reduce::apply(after_two, OverlayAction::CycleEffect);
    assert_eq!(after_three.config.effect, SurroundEffect::DimBlack);
}

#[test]
fn cycle_effect_is_inert_in_off_mode() {
    let before = State::DEFAULT; // mode Off
    let (after, delta) = reduce::apply(before, OverlayAction::CycleEffect);
    assert_eq!(before, after);
    assert!(!delta.is_any());
}

#[test]
fn quit_action_is_observable_via_state_unchanged() {
    // Quit is a one-shot signal emitted by the tick pipeline; reducer stays pure.
    let before = State::DEFAULT;
    let (after, delta) = reduce::apply(before, OverlayAction::Quit);
    assert_eq!(before, after);
    assert!(!delta.is_any());
}
