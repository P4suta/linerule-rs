//! Integration: validate the physical reasonableness of `UserConfig::DEFAULT`.
//!
//! These are not "did the constant compile" tests (the type system
//! handles that). They are "are the values sensible?" tests — DPI-scaled
//! sizes positive, repeat timings monotonically growing, fade decay > 0.

use linerule_core::{HudConfig, InputConfig, OverlayConfig, RenderConfig, UserConfig};

#[test]
fn default_overlay_config_has_legal_thickness_and_opacity() {
    let cfg = OverlayConfig::DEFAULT;
    assert!(cfg.thickness.get() >= 1, "thickness must be ≥ 1");
    assert!(cfg.thickness.get() <= 2048, "thickness must be ≤ 2048");
    assert!(cfg.opacity.get() >= 1, "opacity must be ≥ 1");
}

#[test]
fn default_hud_geometry_is_non_empty_and_positive_margin() {
    let g = HudConfig::DEFAULT.geometry;
    assert!(g.width > 0.0, "HUD width must be positive");
    assert!(g.height > 0.0, "HUD height must be positive");
    assert!(g.margin > 0.0, "HUD margin must be positive");
}

#[test]
fn default_hud_fade_decay_is_positive() {
    let fd = HudConfig::DEFAULT.fade_decay_px;
    assert!(fd > 0.0, "fade_decay_px must be > 0; got {fd}");
}

#[test]
fn default_render_warn_ratio_is_in_zero_one() {
    let r = RenderConfig::DEFAULT.warn_ratio;
    assert!(
        (0.0..=1.0).contains(&r),
        "warn_ratio must be in [0,1]; got {r}"
    );
}

#[test]
fn default_render_fallback_refresh_is_reasonable() {
    let hz = RenderConfig::DEFAULT.fallback_refresh_hz;
    assert!(
        (30..=240).contains(&hz),
        "fallback_refresh_hz outside reasonable monitor range: {hz}"
    );
}

#[test]
fn default_repeat_timings_are_non_zero() {
    let r = InputConfig::DEFAULT.repeat;
    assert!(!r.initial_delay.is_zero(), "initial_delay must be non-zero");
    assert!(
        !r.long_press_threshold.is_zero(),
        "long_press_threshold must be non-zero"
    );
    assert!(!r.release_poll.is_zero(), "release_poll must be non-zero");
    assert!(
        !r.slow_repeat_interval.is_zero(),
        "slow_repeat_interval must be non-zero"
    );
}

#[test]
fn default_long_press_threshold_exceeds_release_poll() {
    // For the AwaitRelease branch to fire long-press undo, the threshold
    // must be observably larger than the polling tick.
    let r = InputConfig::DEFAULT.repeat;
    assert!(
        r.long_press_threshold > r.release_poll,
        "long_press_threshold ({:?}) must exceed release_poll ({:?})",
        r.long_press_threshold,
        r.release_poll
    );
}

#[test]
fn default_tap_steps_are_non_zero_and_positive() {
    let t = InputConfig::DEFAULT.tap_step;
    assert!(t.thickness > 0, "tap_step.thickness must be positive");
    assert!(t.opacity > 0, "tap_step.opacity must be positive");
}

#[test]
fn user_config_default_is_internally_consistent() {
    // Smoke: the aggregate const compiles and its sub-defaults match the
    // individual sub-defaults. (Guards against accidental drift if
    // someone replaces UserConfig::DEFAULT with hand-written values.)
    let u = UserConfig::DEFAULT;
    assert_eq!(u.overlay, OverlayConfig::DEFAULT);
    assert_eq!(u.input, InputConfig::DEFAULT);
    assert_eq!(u.render, RenderConfig::DEFAULT);
    assert_eq!(u.hud, HudConfig::DEFAULT);
}
