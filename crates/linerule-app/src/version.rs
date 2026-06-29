//! The channel-aware build version, stamped at compile time by `build.rs`.
//!
//! Resolves to `X.Y.Z` (stable, when CI sets `LINERULE_VERSION`),
//! `X.Y.Z-nightly.<date>+g<sha>` (nightly), or `X.Y.Z-dev+g<sha>[.dirty]`
//! (ordinary `cargo build`). This is the single string `linerule version`,
//! `linerule --version`, and the boot banner all report.

/// The stamped build version (see module docs for the format per channel).
pub(crate) const VERSION: &str = env!("LINERULE_VERSION");
