//! Channel-aware build version, stamped at compile time by `build.rs`.
//! Format per channel: `X.Y.Z` (stable), `X.Y.Z-nightly.<date>+g<sha>`, or `X.Y.Z-dev+g<sha>[.dirty]`.

/// The stamped build version (see module docs for the format per channel).
pub(crate) const VERSION: &str = env!("LINERULE_VERSION");
