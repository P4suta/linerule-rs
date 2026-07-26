//! Aggregate error type for `linerule-platform-windows`: closed sum over
//! Win32 / COM failure shapes plus [`linerule_core::ChordError`] via `#[from]`.

#![forbid(unsafe_code)]

use linerule_core::{BindingErrors, ChordError, Command, ErrorClass, PreferencesError};
use thiserror::Error;

/// Closed sum of failures handled in `linerule-platform-windows`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
pub enum PlatformError {
    /// An HWND-returning API returned null.
    #[error("{operation}: HWND was null")]
    NullHandle {
        /// Failing API name.
        operation: &'static str,
    },
    /// A BOOL-returning API returned FALSE; code from `GetLastError`.
    #[error("{operation}: BOOL=FALSE (GetLastError = {code:#x} {symbol})")]
    BoolFalse {
        /// Failing API name.
        operation: &'static str,
        /// `GetLastError` value.
        code: u32,
        /// Known `ERROR_*` symbol, or `"WIN32_ERROR(other)"`.
        symbol: &'static str,
    },
    /// An HRESULT-returning API returned a failure code.
    #[error("{operation}: HRESULT = {hr:#x}")]
    BadHr {
        /// Failing API name.
        operation: &'static str,
        /// Returned HRESULT.
        hr: i32,
    },
    /// A standalone `GetLastError` check reported an error.
    #[error("{operation}: GetLastError = {code:#x} {symbol}")]
    LastError {
        /// Failing API name.
        operation: &'static str,
        /// `GetLastError` value.
        code: u32,
        /// Known `ERROR_*` symbol.
        symbol: &'static str,
    },
    /// Chord string parse failure, propagated from `linerule-core`.
    #[error(transparent)]
    Chord(#[from] ChordError),
    /// Complete shortcut validation failure.
    #[error(transparent)]
    Bindings(#[from] BindingErrors),
    /// Preferences schema or binding validation failure.
    #[error(transparent)]
    Preferences(#[from] PreferencesError),
    /// An internal state transition violated a checked invariant.
    #[error("internal invariant violated: {operation}")]
    Invariant {
        /// Operation whose prerequisite was unexpectedly absent.
        operation: &'static str,
    },
    /// Another controller instance already owns the per-user mutex.
    #[error("linerule is already running")]
    AlreadyRunning,
    /// One command failed during the all-at-once RegisterHotKey transaction.
    #[error("failed to register {command:?}: {source}")]
    HotkeyRegistration {
        /// Command whose chord was occupied or rejected.
        command: Command,
        /// Typed Win32 registration failure.
        source: Box<PlatformError>,
    },
    /// Restoring the previous shortcut set also failed.
    #[error("shortcut rollback failed after `{original}`: {rollback}")]
    HotkeyRollback {
        /// Failure that triggered rollback.
        original: String,
        /// Failure while restoring the old set.
        rollback: String,
    },
    /// The application-owned atomic preferences writer failed.
    #[error("persisting preferences failed: {message}")]
    Persistence {
        /// Storage-layer error text preserved across the crate boundary.
        message: String,
    },
    /// The separate Fluent settings process could not be started or did not
    /// complete its private request/response protocol.
    #[error("shortcut settings unavailable: {message}")]
    SettingsHost {
        /// Launch, wait, or protocol failure text.
        message: String,
    },
}

/// `Result` alias for `linerule-platform-windows`.
pub type Result<T, E = PlatformError> = core::result::Result<T, E>;

/// Operations whose failure the overlay continues past: `RegisterHotKey`
/// (conflicts skip with a HUD notice) and `UnregisterHotKey` (absorbed in `Drop`).
const RECOVERABLE_WIN32_OPS: &[&str] = &["RegisterHotKey", "UnregisterHotKey"];

impl PlatformError {
    /// Recovery class for this error.
    #[must_use]
    pub fn class(&self) -> ErrorClass {
        match self {
            Self::NullHandle { .. } => ErrorClass::Fatal,
            Self::BadHr { operation, .. } => {
                if RECOVERABLE_WIN32_OPS.contains(operation) {
                    ErrorClass::Recoverable
                } else {
                    ErrorClass::Fatal
                }
            },
            Self::BoolFalse { operation, .. } | Self::LastError { operation, .. } => {
                if RECOVERABLE_WIN32_OPS.contains(operation) {
                    ErrorClass::Recoverable
                } else {
                    ErrorClass::Fatal
                }
            },
            Self::Chord(e) => e.class(),
            Self::Bindings(_) | Self::Preferences(_) => ErrorClass::Recoverable,
            Self::Invariant { .. } => ErrorClass::ProgrammerError,
            Self::AlreadyRunning => ErrorClass::Recoverable,
            Self::HotkeyRegistration { .. } => ErrorClass::Recoverable,
            Self::HotkeyRollback { .. } => ErrorClass::Fatal,
            Self::Persistence { .. } | Self::SettingsHost { .. } => ErrorClass::Recoverable,
        }
    }
}

/// Symbolic name for common `ERROR_*` codes; `"WIN32_ERROR(other)"` otherwise.
#[must_use]
pub fn decode_last_error(code: u32) -> &'static str {
    match code {
        0 => "ERROR_SUCCESS",
        2 => "ERROR_FILE_NOT_FOUND",
        5 => "ERROR_ACCESS_DENIED",
        6 => "ERROR_INVALID_HANDLE",
        87 => "ERROR_INVALID_PARAMETER",
        1400 => "ERROR_INVALID_WINDOW_HANDLE",
        1407 => "ERROR_CANNOT_FIND_WND_CLASS",
        1410 => "ERROR_CLASS_ALREADY_EXISTS",
        _ => "WIN32_ERROR(other)",
    }
}

/// Build a [`PlatformError::BadHr`] tagged with `operation` from a windows-rs error.
// Fully-qualified `windows::core::Error` to avoid clashing with thiserror's `Error` derive.
pub(crate) fn map_hr(operation: &'static str) -> impl Fn(windows::core::Error) -> PlatformError {
    move |e| PlatformError::BadHr {
        operation,
        hr: e.code().0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_known_codes() {
        assert_eq!(decode_last_error(0), "ERROR_SUCCESS");
        assert_eq!(decode_last_error(2), "ERROR_FILE_NOT_FOUND");
        assert_eq!(decode_last_error(5), "ERROR_ACCESS_DENIED");
        assert_eq!(decode_last_error(6), "ERROR_INVALID_HANDLE");
        assert_eq!(decode_last_error(87), "ERROR_INVALID_PARAMETER");
        assert_eq!(decode_last_error(1400), "ERROR_INVALID_WINDOW_HANDLE");
        assert_eq!(decode_last_error(1407), "ERROR_CANNOT_FIND_WND_CLASS");
        assert_eq!(decode_last_error(1410), "ERROR_CLASS_ALREADY_EXISTS");
    }

    #[test]
    fn decode_unknown_code_falls_back_to_placeholder() {
        assert_eq!(decode_last_error(0xDEAD_BEEF), "WIN32_ERROR(other)");
        assert_eq!(decode_last_error(999_999), "WIN32_ERROR(other)");
    }

    #[test]
    fn display_null_handle_includes_operation() {
        let e = PlatformError::NullHandle {
            operation: "CreateWindowExW",
        };
        let s = e.to_string();
        assert!(s.contains("CreateWindowExW"));
        assert!(s.contains("null"));
    }

    #[test]
    fn display_bool_false_includes_code_and_symbol() {
        let e = PlatformError::BoolFalse {
            operation: "RegisterClassExW",
            code: 1410,
            symbol: "ERROR_CLASS_ALREADY_EXISTS",
        };
        let s = e.to_string();
        assert!(s.contains("RegisterClassExW"));
        assert!(s.contains("0x582"), "should include hex code: {s}");
        assert!(s.contains("ERROR_CLASS_ALREADY_EXISTS"));
    }

    #[test]
    fn display_bad_hr_uses_hex_format() {
        let e = PlatformError::BadHr {
            operation: "D3D11CreateDevice",
            hr: i32::from_be_bytes([0x80, 0x00, 0x00, 0x05_u8.wrapping_neg()]),
        };
        let s = e.to_string();
        assert!(s.contains("D3D11CreateDevice"));
        assert!(s.contains("0x"), "expected hex-formatted HRESULT: {s}");
    }

    #[test]
    fn display_last_error_includes_code_symbol_pair() {
        let e = PlatformError::LastError {
            operation: "GetMonitorInfoW",
            code: 6,
            symbol: "ERROR_INVALID_HANDLE",
        };
        let s = e.to_string();
        assert!(s.contains("GetMonitorInfoW"));
        assert!(s.contains("ERROR_INVALID_HANDLE"));
    }

    #[test]
    fn chord_variant_transparently_wraps_core_error() {
        let e: PlatformError = ChordError::Empty.into();
        assert!(matches!(e, PlatformError::Chord(ChordError::Empty)));
        // transparent display should match the inner ChordError's display.
        assert_eq!(e.to_string(), ChordError::Empty.to_string());
    }

    #[test]
    fn null_handle_is_fatal() {
        let e = PlatformError::NullHandle {
            operation: "CreateWindowExW",
        };
        assert_eq!(e.class(), ErrorClass::Fatal);
    }

    #[test]
    fn bad_hr_is_fatal() {
        let e = PlatformError::BadHr {
            operation: "D3D11CreateDevice",
            hr: -1,
        };
        assert_eq!(e.class(), ErrorClass::Fatal);
    }

    #[test]
    fn register_hotkey_failure_is_recoverable() {
        let e = PlatformError::BoolFalse {
            operation: "RegisterHotKey",
            code: 1409,
            symbol: "ERROR_HOTKEY_ALREADY_REGISTERED",
        };
        assert_eq!(e.class(), ErrorClass::Recoverable);
    }

    #[test]
    fn unregister_hotkey_failure_is_recoverable() {
        let e = PlatformError::LastError {
            operation: "UnregisterHotKey",
            code: 1419,
            symbol: "ERROR_HOTKEY_NOT_REGISTERED",
        };
        assert_eq!(e.class(), ErrorClass::Recoverable);
    }

    #[test]
    fn other_bool_false_is_fatal() {
        let e = PlatformError::BoolFalse {
            operation: "RegisterClassExW",
            code: 1410,
            symbol: "ERROR_CLASS_ALREADY_EXISTS",
        };
        assert_eq!(e.class(), ErrorClass::Fatal);
    }

    #[test]
    fn chord_class_delegates_to_inner_chord_error() {
        let e: PlatformError = ChordError::Empty.into();
        assert_eq!(e.class(), ErrorClass::Recoverable);
    }

    /// Every `RECOVERABLE_WIN32_OPS` entry is Recoverable as both `BoolFalse`
    /// and `LastError`.
    #[test]
    fn recoverable_ops_recover_in_both_bool_false_and_last_error() {
        for op in RECOVERABLE_WIN32_OPS {
            let bool_false = PlatformError::BoolFalse {
                operation: op,
                code: 1409,
                symbol: "ERROR_HOTKEY_ALREADY_REGISTERED",
            };
            assert_eq!(
                bool_false.class(),
                ErrorClass::Recoverable,
                "BoolFalse({op})"
            );

            let last_error = PlatformError::LastError {
                operation: op,
                code: 1409,
                symbol: "ERROR_HOTKEY_ALREADY_REGISTERED",
            };
            assert_eq!(
                last_error.class(),
                ErrorClass::Recoverable,
                "LastError({op})"
            );
        }
    }

    /// Matching is case-sensitive: lowercase `"registerhotkey"` is Fatal.
    #[test]
    fn lowercase_register_hotkey_is_fatal_due_to_case_sensitivity() {
        let e = PlatformError::BoolFalse {
            operation: "registerhotkey",
            code: 1409,
            symbol: "ERROR_HOTKEY_ALREADY_REGISTERED",
        };
        assert_eq!(e.class(), ErrorClass::Fatal);
    }

    /// Empty operation is not in the list, so Fatal.
    #[test]
    fn empty_operation_is_fatal() {
        let e = PlatformError::LastError {
            operation: "",
            code: 0,
            symbol: "ERROR_SUCCESS",
        };
        assert_eq!(e.class(), ErrorClass::Fatal);
    }

    /// `NullHandle` is always Fatal regardless of operation name.
    #[test]
    fn null_handle_is_fatal_regardless_of_operation_name() {
        for op in ["CreateWindowExW", "RegisterHotKey", "", "garbage"] {
            let e = PlatformError::NullHandle { operation: op };
            assert_eq!(e.class(), ErrorClass::Fatal, "NullHandle({op})");
        }
    }

    #[test]
    fn settings_host_failure_is_recoverable() {
        let error = PlatformError::SettingsHost {
            message: "sidecar was not found".to_owned(),
        };
        assert_eq!(error.class(), ErrorClass::Recoverable);
        assert!(error.to_string().contains("sidecar was not found"));
    }
}
