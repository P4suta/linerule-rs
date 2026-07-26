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
fn diagnostics_data_dir_exits_zero_with_redirected_local_state() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cmd = Command::cargo_bin("linerule").expect("binary built");
    cmd.arg("diagnostics")
        .arg("--data-dir")
        .env("LOCALAPPDATA", dir.path());
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("P4suta.linerule"));
}

#[cfg(not(target_os = "windows"))]
#[test]
fn no_args_on_non_windows_fails_with_helpful_message() {
    // Default resident launch bails on non-Windows with a clear platform reason.
    Command::cargo_bin("linerule")
        .expect("binary built")
        .assert()
        .failure()
        .stderr(predicate::str::contains("require Windows 11"));
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
        .stdout(predicate::str::contains("diagnostics"))
        .stdout(predicate::str::contains("settings"));
}

#[test]
fn removed_cli_flag_is_rejected() {
    Command::cargo_bin("linerule")
        .expect("binary built")
        .arg("--cli")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument"));
}
