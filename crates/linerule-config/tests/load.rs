//! `linerule_config::load` (file → Config) integration tests.
//!
//! Cells covering the IO-boundary verbs that `parse_str` does not
//! reach: actual file reads, missing-file error wrapping, and parse
//! errors propagating with miette span information.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use linerule_config::{ConfigError, load};
use tempfile::NamedTempFile;

fn write_temp(body: &str) -> NamedTempFile {
    let mut tmp = NamedTempFile::new().expect("tempfile");
    tmp.write_all(body.as_bytes()).expect("write tempfile body");
    tmp.flush().expect("flush tempfile");
    tmp
}

const VALID_BODY: &str = r#"
[overlay]
[hotkeys]
cycle_mode  = "Ctrl+Alt+R"
pause       = "Ctrl+Alt+P"
thicker     = "Ctrl+Alt+]"
thinner     = "Ctrl+Alt+["
more_opaque = "Ctrl+Alt+="
less_opaque = "Ctrl+Alt+-"
quit        = "Ctrl+Alt+Q"
"#;

#[test]
fn load_returns_config_for_valid_file() {
    let tmp = write_temp(VALID_BODY);
    let cfg = load(tmp.path()).expect("valid config must load");
    assert_eq!(cfg.hotkeys.pause, "Ctrl+Alt+P");
}

#[test]
fn load_emits_io_error_for_missing_file() {
    let result = load(&PathBuf::from("/no/such/path/linerule_missing.toml"));
    let err = result.expect_err("missing file must surface an IO error");
    assert!(
        matches!(err, ConfigError::Io { .. }),
        "expected ConfigError::Io, got {err:?}",
    );
}

#[test]
fn load_emits_parse_error_for_garbage_file() {
    let tmp = write_temp("[overlay]\nthickness = ###\n");
    let result = load(tmp.path());
    let err = result.expect_err("garbage TOML must surface a parse error");
    assert!(
        matches!(err, ConfigError::Parse { .. }),
        "expected ConfigError::Parse, got {err:?}",
    );
}

#[test]
fn load_treats_empty_file_as_full_defaults() {
    // `Config` is `serde(default)`, so an empty file deserialises to
    // the same value as `Config::default()` rather than erroring. Pin
    // that contract here — it's the entire reason the binary's
    // first-run config-create loop can write any subset of fields.
    let tmp = write_temp("");
    let cfg = load(tmp.path()).expect("empty file is a valid all-default config");
    assert_eq!(cfg, linerule_config::Config::default());
}

#[test]
fn dropping_the_tempfile_keeps_load_working_until_then() {
    // Sanity check that the test fixture itself behaves: load must
    // succeed while the tempfile is alive, and *nothing* about the
    // returned `Config` should retain a borrow on the file body.
    let cfg = {
        let tmp = write_temp(VALID_BODY);
        load(tmp.path()).expect("load") // tmp dropped at end of block
    };
    fs::metadata("/").expect("post-drop, unrelated FS calls still work");
    assert_eq!(cfg.hotkeys.cycle_mode, "Ctrl+Alt+R");
}
