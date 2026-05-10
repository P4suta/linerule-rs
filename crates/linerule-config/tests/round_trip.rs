//! Property-style round-trip tests for the TOML schema.
//!
//! Generates a representative grid of valid `Config` values, serializes
//! to TOML, parses back, asserts equality. Catches serde drift,
//! field-rename forgetfulness, and round-trip-breaking newtypes.
//!
//! Not bolero / proptest based — the input space is small enough that
//! a deliberate cartesian sweep beats a randomised search for
//! comprehensiveness AND keeps the test crate dep-light.

use std::path::Path;

use linerule_config::{Config, HotkeyMap, parse_str};
use linerule_core::{OverlayConfig, Rgba, Thickness};

struct ConfigSeed {
    mask_color: Rgba,
    thickness: u16,
    hotkeys: HotkeyMap,
}

fn make_config(seed: ConfigSeed) -> Config {
    let mut overlay = OverlayConfig::default();
    overlay.mask_color = seed.mask_color;
    overlay.thickness = Thickness::new(seed.thickness).expect("thickness in 1..=512");
    let mut cfg = Config::default();
    cfg.overlay = overlay;
    cfg.hotkeys = seed.hotkeys;
    cfg
}

fn roundtrip(cfg: &Config) {
    let body = toml::to_string_pretty(cfg).expect("serialize Config");
    let back = parse_str(Path::new("test.toml"), &body).expect("parse round-trip");
    assert_eq!(*cfg, back, "round-trip mismatch:\n{body}");
}

#[test]
fn default_config_round_trips() {
    roundtrip(&Config::default());
}

#[test]
fn round_trip_grid_thickness() {
    // Sweep the boundaries of the validating Thickness newtype.
    let thicknesses = [1, 2, 27, 28, 29, Thickness::MAX_PX - 1, Thickness::MAX_PX];
    for &t in &thicknesses {
        let cfg = make_config(ConfigSeed {
            mask_color: Rgba::DEFAULT_MASK,
            thickness: t,
            hotkeys: HotkeyMap::default(),
        });
        roundtrip(&cfg);
    }
}

#[test]
fn round_trip_grid_mask_colors() {
    // A grid of corner / mid-range mask colour combinations.
    let colors = [
        Rgba::new(0, 0, 0, 1),
        Rgba::new(255, 255, 255, 255),
        Rgba::new(255, 0, 0, 128),
        Rgba::new(0, 255, 0, 128),
        Rgba::new(0, 0, 255, 128),
        Rgba::new(128, 64, 32, 200),
    ];
    for &mask in &colors {
        let cfg = make_config(ConfigSeed {
            mask_color: mask,
            thickness: 28,
            hotkeys: HotkeyMap::default(),
        });
        roundtrip(&cfg);
    }
}

#[test]
fn round_trip_alternative_hotkey_chords() {
    let mut hk = HotkeyMap::default();
    hk.cycle_mode = "Shift+F1".into();
    hk.pause = "ctrl+alt+shift+meta+H".into();
    hk.thicker = "Win+Up".into();
    hk.thinner = "Cmd+Down".into();
    hk.more_opaque = "Ctrl+Shift+=".into();
    hk.less_opaque = "Ctrl+Shift+-".into();
    hk.quit = "Ctrl+Alt+Shift+Q".into();
    let cfg = make_config(ConfigSeed {
        mask_color: Rgba::DEFAULT_MASK,
        thickness: 28,
        hotkeys: hk,
    });
    roundtrip(&cfg);
}

#[test]
fn deserialize_rejects_thickness_zero() {
    let bad = "
[overlay]
thickness = 0
[hotkeys]
cycle_mode  = \"Ctrl+Alt+R\"
thicker     = \"Ctrl+Alt+]\"
thinner     = \"Ctrl+Alt+[\"
more_opaque = \"Ctrl+Alt+=\"
less_opaque = \"Ctrl+Alt+-\"
pause       = \"Ctrl+Alt+P\"
quit        = \"Ctrl+Alt+Q\"
";
    parse_str(Path::new("bad.toml"), bad)
        .expect_err("invalid config must be rejected at deserialize time");
}

#[test]
fn deserialize_rejects_thickness_above_max() {
    let bad = format!(
        "
[overlay]
thickness = {}
[hotkeys]
cycle_mode  = \"Ctrl+Alt+R\"
thicker     = \"Ctrl+Alt+]\"
thinner     = \"Ctrl+Alt+[\"
more_opaque = \"Ctrl+Alt+=\"
less_opaque = \"Ctrl+Alt+-\"
pause       = \"Ctrl+Alt+P\"
quit        = \"Ctrl+Alt+Q\"
",
        Thickness::MAX_PX + 1,
    );
    parse_str(Path::new("bad.toml"), &bad).expect_err("thickness above MAX_PX must be rejected");
}

#[test]
fn partial_overlay_section_uses_per_field_defaults() {
    // serde(default = "...") on each field means individual entries
    // can be omitted and the corresponding field defaults.
    let partial = "
[overlay]
thickness = 40
[hotkeys]
cycle_mode  = \"Ctrl+Alt+R\"
thicker     = \"Ctrl+Alt+]\"
thinner     = \"Ctrl+Alt+[\"
more_opaque = \"Ctrl+Alt+=\"
less_opaque = \"Ctrl+Alt+-\"
pause       = \"Ctrl+Alt+P\"
quit        = \"Ctrl+Alt+Q\"
";
    let cfg = parse_str(Path::new("partial.toml"), partial).expect("partial overlay must parse");
    assert_eq!(cfg.overlay.thickness.get(), 40);
    assert_eq!(cfg.overlay.mask_color, Rgba::DEFAULT_MASK);
}

#[test]
fn omitted_fields_each_invoke_their_serde_default_callback() {
    // Drives every `serde(default = "OverlayConfig::default_<field>")`
    // arm so the per-field default callbacks are exercised. With every
    // [overlay] field omitted, all callbacks must fire and the
    // result must equal `OverlayConfig::default()`.
    let only_hotkeys = "
[overlay]
[hotkeys]
cycle_mode  = \"Ctrl+Alt+R\"
thicker     = \"Ctrl+Alt+]\"
thinner     = \"Ctrl+Alt+[\"
more_opaque = \"Ctrl+Alt+=\"
less_opaque = \"Ctrl+Alt+-\"
pause       = \"Ctrl+Alt+P\"
quit        = \"Ctrl+Alt+Q\"
";
    let cfg = parse_str(Path::new("only_hotkeys.toml"), only_hotkeys)
        .expect("empty [overlay] must parse via per-field defaults");
    assert_eq!(
        cfg.overlay,
        OverlayConfig::default(),
        "all-default [overlay] must equal OverlayConfig::default()",
    );
}
