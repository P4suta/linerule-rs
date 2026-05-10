//! CLI smoke tests — drive the `linerule` binary as a subprocess.
//! No GUI / no event-loop start — only the surface we can validate
//! deterministically without a display.

use assert_cmd::Command;
use predicates::str::contains;

fn cmd() -> Command {
    Command::cargo_bin("linerule").expect("linerule binary should build")
}

#[test]
fn version_flag_prints_workspace_version() {
    cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("linerule"));
}

#[test]
fn help_lists_subcommands() {
    cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("run"))
        .stdout(contains("config"));
}

#[test]
fn config_path_prints_a_path() {
    cmd()
        .arg("config")
        .arg("path")
        .assert()
        .success()
        .stdout(contains("linerule"))
        .stdout(contains("config.toml"));
}

#[test]
fn config_show_prints_defaults_when_no_file() {
    // `dirs::config_dir` returns Some on every supported target. The default
    // path almost certainly does not exist in CI; the binary should fall
    // back to printing the in-memory defaults rather than failing.
    cmd()
        .arg("config")
        .arg("show")
        .assert()
        .success()
        .stdout(contains("Config"));
}
