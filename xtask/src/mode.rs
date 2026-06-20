//! Native-host awareness for the lint / ci pipelines.
//!
//! The Justfile exports `LINERULE_MODE` (`inside` | `native` | `docker`) when
//! it invokes `cargo xtask`. Only `native` changes behavior: the Windows
//! clippy step runs without `cargo-xwin` (a native Windows host already targets
//! msvc), and the test step avoids the bare-`cargo test` parallelism that trips
//! the linerule-app `event_ring` shared-state tests. Unset / `inside` /
//! `docker` keep the original behavior, so the container and CI are unaffected.

use std::process::{Command, Stdio};

/// Whether the Justfile selected the native (Docker-less host) execution mode.
pub(crate) fn is_native() -> bool {
    std::env::var("LINERULE_MODE").as_deref() == Ok("native")
}

/// Whether `cargo nextest` is callable, so the native test step can match CI's
/// process-per-test isolation instead of a serial `cargo test` fallback.
pub(crate) fn nextest_available() -> bool {
    Command::new("cargo")
        .args(["nextest", "--version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}
