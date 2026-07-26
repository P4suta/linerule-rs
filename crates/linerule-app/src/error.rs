//! Typed failures owned by the executable boundary.

#![forbid(unsafe_code)]

use std::path::PathBuf;

use thiserror::Error;

use crate::logging::LoggingError;
use crate::storage::StorageError;

pub(crate) type Result<T> = std::result::Result<T, AppError>;

/// Top-level application failures.
#[derive(Debug, Error)]
pub(crate) enum AppError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Logging(#[from] LoggingError),
    #[cfg(target_os = "windows")]
    #[error(transparent)]
    Platform(#[from] linerule_platform_windows::PlatformError),
    #[cfg(not(target_os = "windows"))]
    #[error("linerule's resident shell and settings window require Windows 11")]
    UnsupportedPlatform,
    #[error("cannot {operation} {path}: {source}")]
    DiagnosticIo {
        operation: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot decode diagnostic JSON {path}: {source}")]
    DiagnosticJson {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("cannot encode diagnostic JSON: {0}")]
    EncodeDiagnosticJson(serde_json::Error),
}
