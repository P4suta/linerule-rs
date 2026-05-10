//! State-machine cells for `reduce`. Each test exercises one
//! `Action` arm against a meaningful starting `State` and pins the
//! resulting `(State, StateDelta)` shape.

use linerule_core::{Action, Lifecycle, Mode, OverlayConfig, State, StateDelta, Thickness, reduce};

fn fresh() -> State {
    State::default()
}

// ---- CycleMode ------------------------------------------------------------

#[test]
fn cycle_mode_advances_one_step_within_active_lifecycle() {
    let mut s = fresh();
    let prev = s.lifecycle.mode();
    let delta = reduce(&mut s, Action::CycleMode);
    assert_eq!(
        s.lifecycle.mode(),
        linerule_core::cycle(prev),
        "cycle_mode must advance one step",
    );
    assert!(s.lifecycle.is_active(), "starting from Active stays Active");
    assert_eq!(
        delta.lifecycle,
        Some(s.lifecycle),
        "delta must report the new lifecycle",
    );
}

#[test]
fn cycle_mode_period_5() {
    let mut s = fresh();
    let initial = s.lifecycle;
    for _ in 0..5 {
        reduce(&mut s, Action::CycleMode);
    }
    assert_eq!(
        s.lifecycle, initial,
        "applying CycleMode five times must return to initial lifecycle",
    );
}

#[test]
fn cycle_mode_preserves_paused_state() {
    let mut s = fresh();
    s.lifecycle = Lifecycle::Paused(Mode::BAR);
    reduce(&mut s, Action::CycleMode);
    assert!(
        matches!(s.lifecycle, Lifecycle::Paused(_)),
        "cycle while paused stays paused (the next mode is queued for resume)",
    );
    assert_eq!(
        s.lifecycle.mode(),
        Mode::MASK,
        "cycle while paused still advances the mode",
    );
}

// ---- TogglePause ----------------------------------------------------------

#[test]
fn toggle_pause_is_self_inverse() {
    let mut s = fresh();
    let initial = s.lifecycle;
    reduce(&mut s, Action::TogglePause);
    let after_one = s.lifecycle;
    reduce(&mut s, Action::TogglePause);
    let after_two = s.lifecycle;
    assert_ne!(
        after_one, initial,
        "first TogglePause must transition the lifecycle",
    );
    assert_eq!(after_two, initial, "second TogglePause must restore it");
}

#[test]
fn toggle_pause_preserves_inner_mode() {
    let mut s = fresh();
    s.lifecycle = Lifecycle::Active(Mode::VERTICAL_MASK);
    reduce(&mut s, Action::TogglePause);
    assert_eq!(s.lifecycle, Lifecycle::Paused(Mode::VERTICAL_MASK));
    reduce(&mut s, Action::TogglePause);
    assert_eq!(s.lifecycle, Lifecycle::Active(Mode::VERTICAL_MASK));
}

#[test]
fn toggle_pause_delta_reports_new_lifecycle() {
    let mut s = fresh();
    let delta = reduce(&mut s, Action::TogglePause);
    assert_eq!(delta.lifecycle, Some(s.lifecycle));
    assert!(!delta.config, "pause must not touch the visual config");
}

// ---- BumpThickness --------------------------------------------------------

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

// ---- BumpOpacity ----------------------------------------------------------

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
    assert_eq!(
        s.config.opacity.get(),
        255,
        "opacity must saturate at 255 instead of wrapping",
    );
}

// ---- Cross-action invariants ----------------------------------------------

#[test]
fn bump_actions_set_config_changed_flag() {
    let mut s = fresh();
    let delta_t = reduce(&mut s, Action::BumpThickness(1));
    assert!(delta_t.config, "BumpThickness must set the config flag");
    assert_eq!(
        delta_t.lifecycle, None,
        "BumpThickness must not touch lifecycle"
    );
    let delta_o = reduce(&mut s, Action::BumpOpacity(1));
    assert!(delta_o.config, "BumpOpacity must set the config flag");
    assert_eq!(
        delta_o.lifecycle, None,
        "BumpOpacity must not touch lifecycle"
    );
}

#[test]
fn cycle_mode_does_not_change_config() {
    let mut s = fresh();
    let cfg_before = s.config;
    let delta = reduce(&mut s, Action::CycleMode);
    assert_eq!(s.config, cfg_before, "cycle_mode must not change config");
    assert!(!delta.config);
}

// ---- StateDelta defaults --------------------------------------------------

#[test]
fn delta_default_is_no_change() {
    let d = StateDelta::default();
    assert_eq!(d.lifecycle, None);
    assert!(!d.config);
}

// ---- State / Lifecycle defaults -------------------------------------------

#[test]
fn state_default_is_active_off_with_default_config() {
    let s = State::default();
    assert_eq!(s.config, OverlayConfig::default());
    assert_eq!(s.lifecycle, Lifecycle::Active(Mode::Off));
    assert!(s.lifecycle.is_active(), "default lifecycle is Active");
    assert_eq!(s.lifecycle.mode(), Mode::Off, "default inner mode is Off");
}

// ---- Lifecycle helpers ----------------------------------------------------

#[test]
fn lifecycle_with_mode_preserves_active_paused() {
    let active = Lifecycle::Active(Mode::Off);
    let paused = Lifecycle::Paused(Mode::Off);
    assert_eq!(active.with_mode(Mode::BAR), Lifecycle::Active(Mode::BAR));
    assert_eq!(paused.with_mode(Mode::BAR), Lifecycle::Paused(Mode::BAR));
}

#[test]
fn lifecycle_toggled_pause_is_self_inverse() {
    let lc = Lifecycle::Active(Mode::MASK);
    assert_eq!(lc.toggled_pause(), Lifecycle::Paused(Mode::MASK));
    assert_eq!(lc.toggled_pause().toggled_pause(), lc);
}

#[test]
fn lifecycle_mode_strips_pause_layer() {
    assert_eq!(Lifecycle::Active(Mode::BAR).mode(), Mode::BAR);
    assert_eq!(Lifecycle::Paused(Mode::BAR).mode(), Mode::BAR);
}

#[test]
fn lifecycle_is_active_distinguishes_active_from_paused() {
    assert!(Lifecycle::Active(Mode::BAR).is_active());
    assert!(!Lifecycle::Paused(Mode::BAR).is_active());
}
