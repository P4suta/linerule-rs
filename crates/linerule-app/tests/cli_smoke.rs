//! CLI smoke: actually invoke the `linerule` binary and observe exit code
//! + stdout / stderr.
//!
//! Catches regressions like "the binary panics during clap parsing" or
//! "diagnostics writes outside its data dir". `cargo nextest` builds the
//! binary on demand via `CARGO_BIN_EXE_linerule`.

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn version_subcommand_exits_zero_and_prints_linerule_prefix() {
    Command::cargo_bin("linerule")
        .expect("binary built")
        .arg("version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("linerule "));
}

#[test]
fn diagnostics_dry_run_exits_zero_with_redirected_data_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cmd = Command::cargo_bin("linerule").expect("binary built");
    cmd.arg("diagnostics")
        .arg("--dry-run")
        // Redirect the platform-specific data-dir lookups so the binary
        // does not touch the real `%APPDATA%` / `~/.local/share`.
        .env("APPDATA", dir.path())
        .env("XDG_DATA_HOME", dir.path())
        .env("HOME", dir.path());
    cmd.assert().success();
}

#[cfg(not(target_os = "windows"))]
#[test]
fn no_args_on_non_windows_fails_with_helpful_message() {
    // On non-Windows the default `Run` subcommand bails out — make sure
    // it actually exits non-zero and explains why on stderr.
    Command::cargo_bin("linerule")
        .expect("binary built")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Windows-only"));
}

#[test]
fn unknown_subcommand_yields_non_zero_exit() {
    Command::cargo_bin("linerule")
        .expect("binary built")
        .arg("not-a-real-subcommand")
        .assert()
        .failure();
}

#[test]
fn help_flag_succeeds_and_lists_subcommands() {
    Command::cargo_bin("linerule")
        .expect("binary built")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("version"))
        .stdout(predicate::str::contains("diagnostics"));
}

#[test]
fn cli_flag_alone_does_not_panic() {
    // `--cli` without a subcommand still defaults to `Run`, which on
    // non-Windows bails. We just want to confirm clap accepts the flag
    // and the process exits cleanly (success on Windows, controlled
    // failure on Linux).
    if cfg!(target_os = "windows") {
        // Skip: Run on Windows would block on the message pump.
        return;
    }
    Command::cargo_bin("linerule")
        .expect("binary built")
        .arg("--cli")
        .assert()
        .failure() // bails on non-Windows
        .stderr(predicate::str::contains("Windows-only"));
}
