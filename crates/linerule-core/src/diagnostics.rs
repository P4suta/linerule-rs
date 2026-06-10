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

/// Recovery class for errors. Independent from [`Severity`] which is a logging
/// level lattice; this captures *how the app should react* to a failure.
///
/// `Severity` answers "how loud should this log line be?" while `ErrorClass`
/// answers "should the app continue, exit, or treat this as a programming bug?".
/// Both axes are orthogonal — e.g. a `Recoverable` failure can be logged at
/// `Warn`, and a `Fatal` panic at `Error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClass {
    /// Recoverable via log + fallback (e.g. hotkey conflict, chord parse
    /// failure, transient network loss). Candidate for a HUD toast.
    Recoverable,
    /// Requires process exit + crash report (e.g. HWND creation or D3D11 init
    /// failure). Propagated via `?` to `main`, converted to anyhow, exit code 1.
    Fatal,
    /// An invariant violation that should be a panic, but rode `?` past a
    /// boundary (e.g. a static `Opacity::try_new(0)` bug). Not treated as
    /// recoverable; debug builds may catch it via `debug_assert!`.
    ProgrammerError,
}

impl CoreError {
    /// Recovery class. `try_new` boundary-validation failures are static
    /// programmer errors, so this returns [`ErrorClass::ProgrammerError`].
    #[must_use]
    pub const fn class(self) -> ErrorClass {
        match self {
            Self::Opacity { .. } | Self::Thickness { .. } => ErrorClass::ProgrammerError,
        }
    }
}

impl ChordError {
    /// Every `ChordError` variant comes from user config / runtime input, so
    /// all are [`ErrorClass::Recoverable`] (show in HUD, skip, continue).
    #[must_use]
    #[allow(
        clippy::unused_self,
        reason = "keep method-style API and room for per-variant branching; by-value would force a move"
    )]
    pub const fn class(&self) -> ErrorClass {
        ErrorClass::Recoverable
    }
}

impl LineruleError {
    /// Delegates to the inner error's `class()`.
    #[must_use]
    pub const fn class(&self) -> ErrorClass {
        match self {
            // `CoreError: Copy`, so the deref-copy `*e` is fine.
            Self::Core(e) => (*e).class(),
            Self::Chord(e) => e.class(),
        }
    }
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

/// `true` when `hr` is a DXGI / D2D "device-lost" HRESULT (GPU pipeline must be
/// rebuilt):
///
/// - `DXGI_ERROR_DEVICE_REMOVED` (0x887A0005): adapter removed / driver crash
/// - `DXGI_ERROR_DEVICE_HUNG` (0x887A0006): hardware fault detected
/// - `DXGI_ERROR_DEVICE_RESET` (0x887A0007): TDR (Timeout Detection & Recovery)
/// - `D2DERR_RECREATE_TARGET` (0x8899000C): D2D render target lost
#[must_use]
pub const fn is_device_lost_hresult(hr: i32) -> bool {
    // HRESULT is a signed 32-bit code; the literals exceed i32 range, so compare
    // as u32 bit patterns.
    #[allow(
        clippy::cast_sign_loss,
        reason = "u32 bit-pattern comparison, not a value-domain conversion"
    )]
    let hr_bits = hr as u32;
    matches!(
        hr_bits,
        0x887A_0005 | 0x887A_0006 | 0x887A_0007 | 0x8899_000C
    )
}

/// Outcome of `record_device_lost_failure`. `Retry` rebuilds and retries once,
/// storing the new counter; `Quit` requests app shutdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceLostOutcome {
    /// Retry, carrying the new consecutive-failure count (`prev + 1`).
    Retry {
        /// Updated counter value.
        next: u8,
    },
    /// Third consecutive failure reached; requests `OverlayAction::Quit`.
    Quit,
}

/// Records one device-lost failure and decides the next action: `Retry` while
/// `prev < 2`, `Quit` once `prev + 1 >= 3`.
#[must_use]
pub const fn record_device_lost_failure(prev: u8) -> DeviceLostOutcome {
    if prev >= 2 {
        DeviceLostOutcome::Quit
    } else {
        DeviceLostOutcome::Retry { next: prev + 1 }
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

    #[test]
    fn core_error_class_is_programmer_error() {
        assert_eq!(
            CoreError::Opacity { given: 0 }.class(),
            ErrorClass::ProgrammerError
        );
        assert_eq!(
            CoreError::Thickness { given: 9999 }.class(),
            ErrorClass::ProgrammerError
        );
    }

    #[test]
    fn chord_error_class_is_recoverable() {
        assert_eq!(ChordError::Empty.class(), ErrorClass::Recoverable);
        assert_eq!(ChordError::NoKey.class(), ErrorClass::Recoverable);
        assert_eq!(
            ChordError::EmptyToken { position: 0 }.class(),
            ErrorClass::Recoverable
        );
    }

    #[test]
    fn linerule_error_class_delegates_to_inner() {
        let core: LineruleError = CoreError::Opacity { given: 0 }.into();
        assert_eq!(core.class(), ErrorClass::ProgrammerError);

        let chord: LineruleError = ChordError::Empty.into();
        assert_eq!(chord.class(), ErrorClass::Recoverable);
    }

    #[test]
    fn error_class_variants_are_distinct() {
        // ErrorClass intentionally does NOT implement PartialOrd. Recovery class
        // is a tag, not a lattice. Use Severity for log-level ordering.
        assert_ne!(ErrorClass::Recoverable, ErrorClass::Fatal);
        assert_ne!(ErrorClass::Fatal, ErrorClass::ProgrammerError);
        assert_ne!(ErrorClass::Recoverable, ErrorClass::ProgrammerError);
    }

    #[test]
    fn is_device_lost_hresult_matches_documented_codes() {
        // table-driven: 4 hit + 1 miss
        #[allow(
            clippy::cast_possible_wrap,
            reason = "explicit cast to pass the bit pattern as i32"
        )]
        let table: [(i32, bool); 5] = [
            (0x887A_0005_u32 as i32, true),  // DXGI_ERROR_DEVICE_REMOVED
            (0x887A_0006_u32 as i32, true),  // DXGI_ERROR_DEVICE_HUNG
            (0x887A_0007_u32 as i32, true),  // DXGI_ERROR_DEVICE_RESET
            (0x8899_000C_u32 as i32, true),  // D2DERR_RECREATE_TARGET
            (0x8000_4002_u32 as i32, false), // E_NOINTERFACE (a failure, but not device-lost)
        ];
        for (hr, expected) in table {
            assert_eq!(
                is_device_lost_hresult(hr),
                expected,
                "hr={hr:#010x} expected device-lost={expected}"
            );
        }
    }

    #[test]
    fn record_device_lost_failure_retries_under_threshold() {
        assert_eq!(
            record_device_lost_failure(0),
            DeviceLostOutcome::Retry { next: 1 }
        );
        assert_eq!(
            record_device_lost_failure(1),
            DeviceLostOutcome::Retry { next: 2 }
        );
    }

    #[test]
    fn record_device_lost_failure_quits_on_third_failure() {
        assert_eq!(record_device_lost_failure(2), DeviceLostOutcome::Quit);
        // Unexpectedly high prev still quits.
        assert_eq!(record_device_lost_failure(100), DeviceLostOutcome::Quit);
    }

    #[test]
    fn error_class_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&ErrorClass::Recoverable).unwrap(),
            "\"recoverable\""
        );
        assert_eq!(
            serde_json::to_string(&ErrorClass::Fatal).unwrap(),
            "\"fatal\""
        );
        assert_eq!(
            serde_json::to_string(&ErrorClass::ProgrammerError).unwrap(),
            "\"programmer_error\""
        );
    }
}
