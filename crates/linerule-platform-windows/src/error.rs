//! `linerule-platform-windows` の集約エラー型。
//!
//! Win32 / COM 呼び出しの失敗形に加え、[`linerule_core::ChordError`] を
//! `#[from]` で取り込み、`?` 1 つでアプリ境界まで上げられる closed sum を作る。
//! 実 FFI 呼び出しは `win32_ffi.rs` 側で行い、ここではエラー値の構築・整形だけ
//! を担当する (本ファイルは `#![forbid(unsafe_code)]`)。

#![forbid(unsafe_code)]

use linerule_core::{ChordError, ErrorClass};
use thiserror::Error;

/// `linerule-platform-windows` で扱う失敗の closed sum。
///
/// Win32 / COM の失敗形 4 種に加え、`linerule-core` から伝搬する
/// [`ChordError`] を [`PlatformError::Chord`] として受け取る。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
pub enum PlatformError {
    /// HWND を返す API が null を返した（CreateWindowExW など）。
    #[error("{operation}: HWND was null")]
    NullHandle {
        /// 失敗した API 名（`&'static str`、heap 非依存）。
        operation: &'static str,
    },
    /// BOOL を返す API が FALSE を返し、`GetLastError` でコードを取得した。
    #[error("{operation}: BOOL=FALSE (GetLastError = {code:#x} {symbol})")]
    BoolFalse {
        /// 失敗した API 名。
        operation: &'static str,
        /// `GetLastError` の値。
        code: u32,
        /// 既知の `ERROR_*` symbol（不明時は `"WIN32_ERROR(other)"`）。
        symbol: &'static str,
    },
    /// HRESULT を返す API が負値を返した。
    #[error("{operation}: HRESULT = {hr:#x}")]
    BadHr {
        /// 失敗した API 名。
        operation: &'static str,
        /// 返ってきた HRESULT。
        hr: i32,
    },
    /// 単体の `GetLastError` チェックでエラーが報告された。
    #[error("{operation}: GetLastError = {code:#x} {symbol}")]
    LastError {
        /// 失敗した API 名。
        operation: &'static str,
        /// `GetLastError` の値。
        code: u32,
        /// 既知の `ERROR_*` symbol。
        symbol: &'static str,
    },
    /// chord 文字列の解析に失敗した（`linerule-core::input::chord::parse`）。
    #[error(transparent)]
    Chord(#[from] ChordError),
}

/// `linerule-platform-windows` の Result alias。
pub type Result<T, E = PlatformError> = core::result::Result<T, E>;

/// `Win32 / COM` 系 API の中で「失敗しても overlay 全体は継続できる」既知の
/// recoverable operation 名のリスト。`PlatformError::class()` がここに当たれば
/// [`ErrorClass::Recoverable`] を返す。
///
/// 現在は `RegisterHotKey` (既に他プロセスが押さえていた等の競合は HUD に
/// notification を出してスキップ可能) と `UnregisterHotKey` (Drop で吸収) のみ。
const RECOVERABLE_WIN32_OPS: &[&str] = &["RegisterHotKey", "UnregisterHotKey"];

impl PlatformError {
    /// `PlatformError` の recovery class を返す。
    ///
    /// - `NullHandle` / `BadHr` は HWND・COM 初期化失敗の傾向が強く [`ErrorClass::Fatal`]
    /// - `BoolFalse` / `LastError` は `operation` 名で分岐し、`RegisterHotKey` 等
    ///   既知の Recoverable な API なら `Recoverable`、それ以外は `Fatal`
    /// - `Chord` は内部 [`ChordError::class()`] に委譲（実質常に `Recoverable`）
    #[must_use]
    pub fn class(&self) -> ErrorClass {
        match self {
            Self::NullHandle { .. } | Self::BadHr { .. } => ErrorClass::Fatal,
            Self::BoolFalse { operation, .. } | Self::LastError { operation, .. } => {
                if RECOVERABLE_WIN32_OPS.contains(operation) {
                    ErrorClass::Recoverable
                } else {
                    ErrorClass::Fatal
                }
            },
            Self::Chord(e) => e.class(),
        }
    }
}

/// よく出る `ERROR_*` だけ static 文字列で symbolic name を返す。
/// 不明な値は `"WIN32_ERROR(other)"` を返してログに残せるようにする。
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
}
