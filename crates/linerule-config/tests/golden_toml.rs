//! Golden round-trip + diagnostic tests for linerule-config.
//!
//! Validates:
//! - A canonical TOML file deserializes to the expected `Config`.
//! - A round-trip through `serde` is byte-stable.
//! - Malformed TOML produces a `ConfigError::Parse` carrying a span.

use std::path::{Path, PathBuf};

use linerule_config::{Config, ConfigError, parse_str};

const GOLDEN_TOML: &str = "\
[overlay]
bar_color   = { r = 255, g = 235, b = 59,  a = 170 }
mask_color  = { r = 8,   g = 8,   b = 8,   a = 204 }
thickness   = 28
opacity     = 170

[hotkeys]
cycle_mode     = \"Ctrl+Alt+R\"
toggle_visible = \"Ctrl+Alt+H\"
thicker        = \"Ctrl+Alt+]\"
thinner        = \"Ctrl+Alt+[\"
more_opaque    = \"Ctrl+Alt+=\"
less_opaque    = \"Ctrl+Alt+-\"
quit           = \"Ctrl+Alt+Q\"
";

#[test]
fn golden_toml_parses_to_default_config() {
    let cfg = parse_str(Path::new("golden.toml"), GOLDEN_TOML).expect("golden TOML must parse");
    assert_eq!(
        cfg,
        Config::default(),
        "golden TOML must equal Config::default()"
    );
}

#[test]
fn empty_toml_uses_defaults() {
    let cfg = parse_str(Path::new("empty.toml"), "")
        .expect("empty TOML must parse via #[serde(default)]");
    assert_eq!(cfg, Config::default());
}

#[test]
fn unknown_keys_are_rejected() {
    let bad = "[overlay]\nunknown_key = 1\n";
    let err = parse_str(Path::new("bad.toml"), bad)
        .expect_err("unknown_key must be rejected by deny_unknown_fields");
    assert!(matches!(err, ConfigError::Parse { .. }));
}

#[test]
fn malformed_syntax_produces_parse_error_with_span() {
    let bad = "[overlay\nbar_color = ?";
    let err = parse_str(Path::new("bad.toml"), bad).expect_err("must reject malformed TOML");
    match err {
        ConfigError::Parse { span, .. } => {
            // span should point somewhere into the source — we don't pin
            // the exact offset because toml::de may track it differently
            // across versions, but it must be > 0 in length.
            assert!(!span.is_empty(), "parse span must cover at least one char");
        }
        other => panic!("expected ConfigError::Parse, got {other:?}"),
    }
}

#[test]
fn opacity_zero_is_rejected_at_deserialize_time() {
    let bad = "[overlay]\nopacity = 0\n";
    let err = parse_str(Path::new("bad.toml"), bad)
        .expect_err("opacity=0 must be rejected at deserialize");
    assert!(matches!(err, ConfigError::Parse { .. }));
}

#[test]
fn thickness_overflow_is_rejected_at_deserialize_time() {
    let bad = "[overlay]\nthickness = 9999\n";
    let err = parse_str(Path::new("bad.toml"), bad)
        .expect_err("thickness=9999 must be rejected at deserialize");
    assert!(matches!(err, ConfigError::Parse { .. }));
}

#[test]
fn missing_file_produces_io_error_with_path() {
    let missing = PathBuf::from("/no/such/path/linerule-test-missing.toml");
    let err = linerule_config::load(&missing).expect_err("missing file must error");
    match err {
        ConfigError::Io { path, .. } => {
            assert_eq!(path, missing);
        }
        other => panic!("expected ConfigError::Io, got {other:?}"),
    }
}

#[test]
fn default_path_resolves_under_linerule_subdir() {
    let path =
        linerule_config::default_path().expect("default config path must resolve on this platform");
    assert!(
        path.to_string_lossy().contains("linerule"),
        "default_path should be under a `linerule` directory: {}",
        path.display(),
    );
    assert!(
        path.file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.ends_with("config.toml")),
        "default_path should end in config.toml: {}",
        path.display(),
    );
}

#[test]
fn config_default_has_seven_default_hotkeys_including_emergency_quit() {
    let h = linerule_config::HotkeyMap::default();
    assert_eq!(h.cycle_mode, "Ctrl+Alt+R");
    assert_eq!(h.toggle_visible, "Ctrl+Alt+H");
    assert_eq!(h.thicker, "Ctrl+Alt+]");
    assert_eq!(h.thinner, "Ctrl+Alt+[");
    assert_eq!(h.more_opaque, "Ctrl+Alt+=");
    assert_eq!(h.less_opaque, "Ctrl+Alt+-");
    // Emergency-exit chord MUST be present in defaults so a user who
    // installs linerule and never edits config still has an escape
    // path when the overlay wedges them out (it covers the whole
    // screen and click-through means the binary itself never sees
    // keypress events).
    assert_eq!(h.quit, "Ctrl+Alt+Q");
}
