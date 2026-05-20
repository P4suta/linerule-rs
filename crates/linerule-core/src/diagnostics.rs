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
    /// Log + fallback と継続可能 (例: hotkey 競合 / chord parse 失敗 / network 一時断)。
    /// HUD toast に表示する候補。
    Recoverable,
    /// プロセス終了 + crash report 残しを要求 (例: HWND 作成失敗 / D3D11 初期化失敗)。
    /// 上位で `?` で `main` に上げて anyhow に変換され、exit code 1 で終了する。
    Fatal,
    /// 本来 panic で表現すべき不変条件違反だが、boundary でふいに `?` に乗ったときの
    /// tag (例: `Opacity::try_new(0)` の静的バグ)。Recoverable とは扱わず、debug build
    /// では `debug_assert!` で即捕捉する余地を残す。
    ProgrammerError,
}

impl CoreError {
    /// `CoreError` の recovery class。`Opacity::try_new` / `Thickness::try_new`
    /// の静的入力エラーは boundary validation のプログラマ誤りとして
    /// [`ErrorClass::ProgrammerError`] を返す。
    ///
    /// `CoreError` は `Copy` (8 byte) なので by-value で受ける。
    #[must_use]
    pub const fn class(self) -> ErrorClass {
        match self {
            Self::Opacity { .. } | Self::Thickness { .. } => ErrorClass::ProgrammerError,
        }
    }
}

impl ChordError {
    /// `ChordError` は全 variant が user config / runtime input 由来なので、
    /// HUD に表示してスキップ継続できる [`ErrorClass::Recoverable`]。
    #[must_use]
    #[allow(
        clippy::unused_self,
        reason = "method-style API を維持し将来 per-variant 分岐の余地を残す。\
                  ChordError は `String` field を持つため by-value 化は move 化に\
                  なり caller 影響大"
    )]
    pub const fn class(&self) -> ErrorClass {
        ErrorClass::Recoverable
    }
}

impl LineruleError {
    /// 内部 error の `class()` に委譲。
    #[must_use]
    pub const fn class(&self) -> ErrorClass {
        match self {
            // `CoreError: Copy` なので `*e` で deref-copy しても安全。
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
