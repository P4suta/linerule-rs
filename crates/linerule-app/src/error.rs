//! App-layer error aggregator `AppError`.
//!
//! Merges core and platform errors while keeping the dependency direction
//! `app → platform-windows → core`. `LineruleError → AppError` and
//! `PlatformError → AppError` come from thiserror `#[from]`; I/O and serde
//! failures land in the same enum.
//!
//! `LineruleError` has no `Platform` variant (orphan rule + keeping the
//! dependency direction clean): core stays unaware of platform-windows, and the
//! merge point lives in the app layer.
//!
//! `main()` keeps `anyhow::Result`; thiserror's `Into<anyhow::Error>` lets a
//! single `?` lift `AppError` into anyhow at the boundary.
//!
//! The `Platform` variant exists on Windows targets only (platform-windows is
//! itself cfg-gated under `[target.'cfg(windows)'.dependencies]`).

#![forbid(unsafe_code)]

use linerule_core::{ErrorClass, LineruleError};
#[cfg(target_os = "windows")]
use linerule_platform_windows::PlatformError;
use thiserror::Error;

/// Aggregate error type for linerule-app, unifying core / platform / I/O /
/// serde.
///
/// Classified via `class()` on `boot::run_overlay`'s error path. That caller is
/// Windows-only, so on Linux the `dead_code` allow is applied; on Windows the
/// type is always consumed.
#[cfg_attr(
    not(target_os = "windows"),
    allow(
        dead_code,
        reason = "caller boot::run_overlay is cfg-gated away on Linux; consumed on Windows"
    )
)]
#[derive(Debug, Error)]
pub(crate) enum AppError {
    /// From `linerule-core` (`CoreError` / `ChordError`).
    #[error(transparent)]
    Core(#[from] LineruleError),
    /// From `linerule-platform-windows`. Windows target only.
    #[cfg(target_os = "windows")]
    #[error(transparent)]
    Platform(#[from] PlatformError),
    /// I/O (e.g. `std::fs::read_dir` on the diagnostics path).
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    /// `serde_json::Error` (crash dump read/write).
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
}

impl AppError {
    /// Delegates to the inner error's `class()`. `Io` / `Serde` default to
    /// `Fatal`.
    ///
    /// On Linux the only caller (`classify_and_log`) is cfg-gated away, hence
    /// the Linux-only `dead_code` allow.
    #[cfg_attr(
        not(target_os = "windows"),
        allow(
            dead_code,
            reason = "caller classify_and_log is cfg-gated away on Linux"
        )
    )]
    pub(crate) fn class(&self) -> ErrorClass {
        match self {
            Self::Core(e) => e.class(),
            #[cfg(target_os = "windows")]
            Self::Platform(e) => e.class(),
            Self::Io(_) | Self::Serde(_) => ErrorClass::Fatal,
        }
    }
}

/// Log an `AppError` by its [`ErrorClass`]. `Recoverable` returns `Continue`,
/// `Fatal` / `ProgrammerError` return `Stop`. The caller handles any HUD push
/// (the overlay handle is context-dependent).
#[cfg(target_os = "windows")]
pub(crate) fn classify_and_log(err: &AppError) -> RunDecision {
    let class = err.class();
    match class {
        ErrorClass::Recoverable => {
            tracing::warn!(error = %err, class = "recoverable", "AppError classified recoverable; continuing");
            RunDecision::Continue
        },
        ErrorClass::Fatal => {
            tracing::error!(error = %err, class = "fatal", "AppError classified fatal");
            RunDecision::Stop
        },
        ErrorClass::ProgrammerError => {
            tracing::error!(error = %err, class = "programmer", "AppError classified as programmer error; this is a bug");
            debug_assert!(false, "ProgrammerError reached classify_and_log: {err}");
            RunDecision::Stop
        },
    }
}

/// Return value of [`classify_and_log`]. `Continue` means push to the HUD and
/// keep going; `Stop` means bubble up to main via `?`.
#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunDecision {
    /// Recoverable. Caller pushes a HUD notification.
    Continue,
    /// Fatal / `ProgrammerError`. Caller does `Err(_)?`.
    Stop,
}

#[cfg(test)]
mod tests {
    use super::*;
    use linerule_core::CoreError;

    #[test]
    fn app_error_absorbs_linerule_error() {
        let e: AppError = LineruleError::from(CoreError::Opacity { given: 0 }).into();
        assert!(matches!(e, AppError::Core(_)));
        assert_eq!(e.class(), ErrorClass::ProgrammerError);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn app_error_absorbs_platform_error() {
        let e: AppError = PlatformError::NullHandle {
            operation: "CreateWindowExW",
        }
        .into();
        assert!(matches!(e, AppError::Platform(_)));
        assert_eq!(e.class(), ErrorClass::Fatal);
    }

    #[test]
    fn app_error_absorbs_io_error() {
        let io = std::io::Error::other("test io error");
        let e: AppError = io.into();
        assert!(matches!(e, AppError::Io(_)));
        assert_eq!(e.class(), ErrorClass::Fatal);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn chord_error_via_platform_is_recoverable() {
        // ChordError stays `Recoverable` through PlatformError and AppError.
        use linerule_core::ChordError;
        let e: AppError = PlatformError::from(ChordError::Empty).into();
        assert_eq!(e.class(), ErrorClass::Recoverable);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn app_error_converts_into_anyhow_via_question_mark() {
        // Compile-time check that `?` converts into anyhow.
        fn try_chain() -> anyhow::Result<()> {
            let app: AppError = PlatformError::NullHandle { operation: "test" }.into();
            Err(app)?;
            Ok(())
        }
        let err = try_chain().unwrap_err();
        assert!(err.to_string().contains("test"));
    }
}
