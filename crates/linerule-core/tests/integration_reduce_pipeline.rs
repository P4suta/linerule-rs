//! Integration: state machine pipeline end-to-end.
//!
//! Tests that the reducer + frame builder compose correctly across
//! multi-step action sequences. This is the pure-stack equivalent of
//! "press Ctrl+Alt+R three times in a row" without the OS layer.

use linerule_core::{Mode, OverlayAction, Point, ScreenRect, State, frame, state::reduce};

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
fn cycle_mode_three_times_returns_to_off() {
    let s = run(&[
        OverlayAction::CycleMode,
        OverlayAction::CycleMode,
        OverlayAction::CycleMode,
    ]);
    assert_eq!(s.mode, Mode::Off);
}

#[test]
fn cycle_mode_then_frame_has_layers() {
    let s = run(&[OverlayAction::CycleMode]);
    assert_eq!(s.mode, Mode::Horizontal);
    let f = frame(s, Point::new(960, 540), monitor());
    assert!(!f.is_empty(), "Horizontal mode should produce layers");
}

#[test]
fn toggle_visible_then_frame_is_empty_even_in_active_mode() {
    let s = run(&[OverlayAction::CycleMode, OverlayAction::ToggleVisible]);
    assert_eq!(s.mode, Mode::Horizontal);
    assert!(!s.visible);
    let f = frame(s, Point::new(960, 540), monitor());
    assert!(f.is_empty(), "invisible state must produce an empty frame");
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
    let start = State {
        mode: Mode::Horizontal,
        ..State::DEFAULT
    };
    let s = run(&[
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
fn quit_action_is_observable_via_state_unchanged() {
    // Quit is a one-shot signal: the reducer does not mutate state, the
    // tick pipeline emits the TickEffect::Quit. We verify reducer purity here.
    let before = State::DEFAULT;
    let (after, delta) = reduce::apply(before, OverlayAction::Quit);
    assert_eq!(before, after);
    assert!(!delta.is_any());
}
