//! Native-host awareness for lint/ci pipelines.
//!
//! `LINERULE_MODE` (set by the Justfile): only `native` changes behavior; unset/`inside`/`docker` keep container/CI defaults.

use std::process::{Command, Stdio};

/// Whether the Justfile selected the native (Docker-less host) execution mode.
pub(crate) fn is_native() -> bool {
    std::env::var("LINERULE_MODE").as_deref() == Ok("native")
}

/// Whether `cargo nextest` is callable (native step needs process-per-test isolation; else serial `cargo test`).
pub(crate) fn nextest_available() -> bool {
    Command::new("cargo")
        .args(["nextest", "--version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}
