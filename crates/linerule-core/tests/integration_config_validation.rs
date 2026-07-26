//! Integration: assert internal policy defaults are sensible.

use linerule_core::{
    BlurAmount, CoreError, HudConfig, Opacity, OverlayConfig, RenderConfig, SurroundEffect,
    TapStepConfig, Thickness,
};

#[test]
fn default_overlay_config_has_legal_thickness_and_opacity() {
    let cfg = OverlayConfig::DEFAULT;
    assert!(cfg.thickness.get() >= 1, "thickness must be ≥ 1");
    assert!(cfg.thickness.get() <= 2048, "thickness must be ≤ 2048");
    assert!(cfg.opacity.get() >= 1, "opacity must be ≥ 1");
}

#[test]
fn default_surround_effect_preserves_historical_dim_black() {
    // Default stays dim-black; white-wash is opt-in via the effect cycle.
    assert_eq!(OverlayConfig::DEFAULT.effect, SurroundEffect::DimBlack);
    let mask = SurroundEffect::DimBlack.mask_color();
    assert_eq!(
        (mask.r, mask.g, mask.b),
        (0, 0, 0),
        "dim mask must be black"
    );
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
fn default_tap_steps_are_non_zero_and_positive() {
    let t = TapStepConfig::DEFAULT;
    assert!(t.thickness > 0, "tap_step.thickness must be positive");
    assert!(t.opacity > 0, "tap_step.opacity must be positive");
}

#[test]
fn invalid_scalar_boundaries_return_typed_errors() {
    assert!(matches!(
        Opacity::try_new(0),
        Err(CoreError::Opacity { given: 0 })
    ));
    assert!(matches!(
        Thickness::try_new(0),
        Err(CoreError::Thickness { given: 0 })
    ));
    assert!(matches!(
        Thickness::try_new(2_049),
        Err(CoreError::Thickness { given: 2_049 })
    ));
    assert!(matches!(
        BlurAmount::try_new(0),
        Err(CoreError::Blur { given: 0 })
    ));
    assert_eq!(BlurAmount::try_new(1).map(BlurAmount::get), Ok(1));
}
