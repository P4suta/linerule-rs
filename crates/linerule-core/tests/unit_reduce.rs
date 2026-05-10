//! State-machine cells for `reduce`. These test the contract — apply each
//! `Action` to each meaningful starting `State` and check the resulting
//! `(state, delta)` shape. Most assertions fail until task #9 lands.

use linerule_core::{Action, Mode, OverlayConfig, State, StateDelta, Thickness, cycle, reduce};

fn fresh() -> State {
    State::default()
}

#[test]
fn cycle_mode_advances_one_step() {
    let mut s = fresh();
    let prev_mode = s.mode;
    let delta = reduce(&mut s, Action::CycleMode);
    assert_eq!(s.mode, cycle(prev_mode), "cycle_mode must advance one step");
    assert_eq!(delta.mode, Some(s.mode), "delta must report new mode");
}

#[test]
fn cycle_mode_period_4() {
    let mut s = fresh();
    let initial = s.mode;
    for _ in 0..4 {
        reduce(&mut s, Action::CycleMode);
    }
    assert_eq!(
        s.mode, initial,
        "applying CycleMode four times must return to initial mode"
    );
}

#[test]
fn toggle_visible_is_self_inverse() {
    let mut s = fresh();
    let initial_visible = s.visible;
    reduce(&mut s, Action::ToggleVisible);
    let after_one = s.visible;
    reduce(&mut s, Action::ToggleVisible);
    let after_two = s.visible;
    assert_ne!(after_one, initial_visible);
    assert_eq!(after_two, initial_visible);
}

#[test]
fn toggle_visible_delta_reports_new_visibility() {
    let mut s = fresh();
    let delta = reduce(&mut s, Action::ToggleVisible);
    assert_eq!(delta.visible, Some(s.visible));
}

#[test]
fn bump_thickness_increases_thickness() {
    let mut s = fresh();
    let before = s.config.thickness.get();
    reduce(&mut s, Action::BumpThickness(2));
    assert_eq!(
        s.config.thickness.get(),
        before.saturating_add(2),
        "BumpThickness(+2) must add 2 logical px",
    );
}

#[test]
fn bump_thickness_decreases_thickness() {
    let mut s = fresh();
    let before = s.config.thickness.get();
    reduce(&mut s, Action::BumpThickness(-2));
    assert_eq!(
        s.config.thickness.get(),
        before.saturating_sub(2),
        "BumpThickness(-2) must subtract 2 logical px",
    );
}

#[test]
fn bump_thickness_saturates_at_minimum() {
    let mut s = fresh();
    s.config.thickness = Thickness::new(1).expect("1 is in range");
    reduce(&mut s, Action::BumpThickness(-100));
    assert!(
        s.config.thickness.get() >= 1,
        "thickness must never drop below 1 (was {})",
        s.config.thickness.get(),
    );
}

#[test]
fn bump_thickness_saturates_at_maximum() {
    let mut s = fresh();
    s.config.thickness = Thickness::new(Thickness::MAX_PX).expect("MAX is in range");
    reduce(&mut s, Action::BumpThickness(100));
    assert!(
        s.config.thickness.get() <= Thickness::MAX_PX,
        "thickness must never exceed MAX_PX (was {})",
        s.config.thickness.get(),
    );
}

#[test]
fn bump_opacity_increases_opacity() {
    let mut s = fresh();
    let before = s.config.opacity.get();
    reduce(&mut s, Action::BumpOpacity(5));
    assert_eq!(
        s.config.opacity.get(),
        before.saturating_add(5),
        "BumpOpacity(+5) must add 5",
    );
}

#[test]
fn bump_opacity_saturates_at_one() {
    let mut s = fresh();
    s.config.opacity = linerule_core::Opacity::new(1).expect("1 is in range");
    reduce(&mut s, Action::BumpOpacity(-100));
    assert!(
        s.config.opacity.get() >= 1,
        "opacity must never drop below 1 (was {})",
        s.config.opacity.get(),
    );
}

#[test]
fn bump_opacity_saturates_at_max() {
    let mut s = fresh();
    s.config.opacity = linerule_core::Opacity::new(255).expect("255 is in range");
    reduce(&mut s, Action::BumpOpacity(100));
    // opacity is `u8` so the type bound already implies `<= 255`; what we
    // really want to confirm is that the saturating add did not wrap to a
    // smaller value.
    assert_eq!(
        s.config.opacity.get(),
        255,
        "opacity must saturate at 255 instead of wrapping to a smaller value",
    );
}

#[test]
fn bump_actions_set_config_changed_flag() {
    let mut s = fresh();
    let delta_t = reduce(&mut s, Action::BumpThickness(1));
    assert!(delta_t.config, "BumpThickness must set config_changed");
    let delta_o = reduce(&mut s, Action::BumpOpacity(1));
    assert!(delta_o.config, "BumpOpacity must set config_changed");
}

#[test]
fn cycle_mode_does_not_change_config_or_visible() {
    let mut s = fresh();
    let cfg_before = s.config;
    let vis_before = s.visible;
    let delta = reduce(&mut s, Action::CycleMode);
    assert_eq!(s.config, cfg_before, "cycle_mode must not change config");
    assert_eq!(
        s.visible, vis_before,
        "cycle_mode must not change visibility"
    );
    assert_eq!(delta.visible, None);
    assert!(!delta.config);
}

#[test]
fn delta_default_is_no_change() {
    let d = StateDelta::default();
    assert_eq!(d.mode, None);
    assert_eq!(d.visible, None);
    assert_eq!(d.paused, None);
    assert!(!d.config);
}

#[test]
fn toggle_pause_is_self_inverse() {
    let mut s = fresh();
    let initial = s.paused;
    reduce(&mut s, Action::TogglePause);
    assert_ne!(s.paused, initial, "first TogglePause flips the flag");
    reduce(&mut s, Action::TogglePause);
    assert_eq!(s.paused, initial, "second TogglePause restores the flag");
}

#[test]
fn toggle_pause_delta_reports_new_paused() {
    let mut s = fresh();
    let delta = reduce(&mut s, Action::TogglePause);
    assert_eq!(delta.paused, Some(s.paused));
    assert_eq!(delta.mode, None, "paused must not touch mode");
    assert_eq!(delta.visible, None, "paused must not touch visibility");
    assert!(!delta.config, "paused must not touch the visual config");
}

#[test]
fn fresh_state_is_not_paused() {
    let s = State::default();
    assert!(!s.paused, "default State must not start paused");
}

#[test]
fn state_default_uses_overlay_config_default() {
    let s = State::default();
    assert_eq!(s.config, OverlayConfig::default());
    assert_eq!(s.mode, Mode::Off);
    assert!(!s.visible);
}

#[test]
fn quit_is_a_state_machine_no_op() {
    // `Action::Quit` is a side-effect-only action handled by the
    // platform layer (event-loop tear-down). The state machine MUST
    // treat it as a no-op so the OverlayApp can pattern-match on
    // Action exhaustively without special-casing Quit.
    let mut s = fresh();
    let snapshot = s;
    let delta = reduce(&mut s, Action::Quit);
    assert_eq!(s, snapshot, "Quit must not mutate State");
    assert_eq!(
        delta,
        StateDelta::default(),
        "Quit must produce a default StateDelta"
    );
}
