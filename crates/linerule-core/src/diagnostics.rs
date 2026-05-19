//! Structured error and event types used by `linerule-core`.
//!
//! - [`CoreError`] is the only error returned from boundary validators inside
//!   this crate (`try_new` constructors).
//! - [`LineruleError`] is the crate's aggregate error, unifying every error
//!   shape that travels through `?` from core to the app boundary. Use
//!   [`crate::Result`] as the canonical `Result<T, LineruleError>`.
//! - [`Severity`] is the diagnostic level lattice, parallel to
//!   [`tracing::Level`].

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::Level;

use crate::input::chord::ChordError;

/// Errors produced by `linerule-core` validators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Error)]
#[serde(tag = "kind")]
pub enum CoreError {
    /// `Opacity::try_new` was called with `0`.
    #[error("opacity must be in [1, 255], got {given}")]
    Opacity {
        /// The rejected value.
        given: i32,
    },
    /// `Thickness::try_new` was called outside `[1, 2048]`.
    #[error("thickness must be in [1, 2048], got {given}")]
    Thickness {
        /// The rejected value.
        given: i32,
    },
}

/// Aggregate error for `linerule-core`.
///
/// Anything that can fail in core converts into one of these variants via
/// `#[from]`, so the app boundary can use a single `?` chain across the
/// whole stack.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
pub enum LineruleError {
    /// Boundary-validator failure (`Opacity` / `Thickness` `try_new`).
    #[error(transparent)]
    Core(#[from] CoreError),
    /// Chord-string parse failure.
    #[error(transparent)]
    Chord(#[from] ChordError),
}

/// Severity lattice for diagnostic events.
///
/// Matches the standard [`tracing::Level`] ordering (`Error < Warn < Info <
/// Debug < Trace`) so a target-level filter on tracing immediately
/// corresponds to a `Severity` cutoff. Use `Level::from(severity)` for the
/// standard conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Recoverable failures and protocol violations.
    Error,
    /// Unexpected but non-fatal conditions.
    Warn,
    /// High-level lifecycle events.
    Info,
    /// Diagnostics useful while developing.
    Debug,
    /// Fine-grained per-tick traces.
    Trace,
}

impl From<Severity> for Level {
    fn from(s: Severity) -> Self {
        match s {
            Severity::Error => Self::ERROR,
            Severity::Warn => Self::WARN,
            Severity::Info => Self::INFO,
            Severity::Debug => Self::DEBUG,
            Severity::Trace => Self::TRACE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_order_matches_tracing_intuition() {
        assert!(Severity::Error < Severity::Warn);
        assert!(Severity::Warn < Severity::Info);
        assert!(Severity::Info < Severity::Debug);
        assert!(Severity::Debug < Severity::Trace);
    }

    #[test]
    fn severity_maps_to_tracing_level() {
        assert_eq!(Level::from(Severity::Error), Level::ERROR);
        assert_eq!(Level::from(Severity::Warn), Level::WARN);
        assert_eq!(Level::from(Severity::Info), Level::INFO);
        assert_eq!(Level::from(Severity::Debug), Level::DEBUG);
        assert_eq!(Level::from(Severity::Trace), Level::TRACE);
    }

    #[test]
    fn linerule_error_absorbs_core_and_chord_errors() {
        let core: LineruleError = CoreError::Opacity { given: 0 }.into();
        assert!(matches!(
            core,
            LineruleError::Core(CoreError::Opacity { given: 0 })
        ));

        let chord: LineruleError = ChordError::Empty.into();
        assert!(matches!(chord, LineruleError::Chord(ChordError::Empty)));
    }
}
