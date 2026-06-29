//! CLI smoke: invoke the `linerule` binary, check exit code + stdout/stderr.

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn version_subcommand_exits_zero_and_prints_linerule_prefix() {
    Command::cargo_bin("linerule")
        .expect("binary built")
        .arg("version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("linerule "))
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn version_flag_exits_zero_and_prints_linerule_prefix() {
    // clap's native `--version`, same stamped string as the `version` subcommand.
    Command::cargo_bin("linerule")
        .expect("binary built")
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("linerule "))
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn diagnostics_dry_run_exits_zero_with_redirected_data_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cmd = Command::cargo_bin("linerule").expect("binary built");
    cmd.arg("diagnostics")
        .arg("--dry-run")
        // Redirect data-dir lookups off the real `%APPDATA%` / `~/.local/share`.
        .env("APPDATA", dir.path())
        .env("XDG_DATA_HOME", dir.path())
        .env("HOME", dir.path());
    cmd.assert().success();
}

#[cfg(not(target_os = "windows"))]
#[test]
fn no_args_on_non_windows_fails_with_helpful_message() {
    // Default `Run` subcommand bails on non-Windows: non-zero exit + stderr reason.
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
    // `--cli` without a subcommand defaults to `Run`; just confirm clap accepts it.
    if cfg!(target_os = "windows") {
        // Skip: Run on Windows blocks on the message pump.
        return;
    }
    Command::cargo_bin("linerule")
        .expect("binary built")
        .arg("--cli")
        .assert()
        .failure() // bails on non-Windows
        .stderr(predicate::str::contains("Windows-only"));
}
