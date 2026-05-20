//! Win32 / COM 呼び出し失敗を表すエラー型。
//!
//! 本ファイルは `#![forbid(unsafe_code)]`。実 FFI 呼び出しは `win32_ffi.rs`
//! 側で行い、ここではエラー値の構築・整形だけを担当する。

#![forbid(unsafe_code)]

use thiserror::Error;

/// `linerule-platform-windows` 内で扱う Win32 / COM 失敗の closed sum。
///
/// `windows::core::Error` は HRESULT / Last-Error / GetLastError をまとめて
/// 表す型だが、表示・分類のためにここでは別 enum に薄くラップする。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
pub enum Win32Error {
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
}

/// `linerule-platform-windows` の Result alias。
pub type Result<T, E = Win32Error> = core::result::Result<T, E>;

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
        let e = Win32Error::NullHandle {
            operation: "CreateWindowExW",
        };
        let s = e.to_string();
        assert!(s.contains("CreateWindowExW"));
        assert!(s.contains("null"));
    }

    #[test]
    fn display_bool_false_includes_code_and_symbol() {
        let e = Win32Error::BoolFalse {
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
        let e = Win32Error::BadHr {
            operation: "D3D11CreateDevice",
            hr: i32::from_be_bytes([0x80, 0x00, 0x00, 0x05_u8.wrapping_neg()]),
        };
        let s = e.to_string();
        assert!(s.contains("D3D11CreateDevice"));
        assert!(s.contains("0x"), "expected hex-formatted HRESULT: {s}");
    }

    #[test]
    fn display_last_error_includes_code_symbol_pair() {
        let e = Win32Error::LastError {
            operation: "GetMonitorInfoW",
            code: 6,
            symbol: "ERROR_INVALID_HANDLE",
        };
        let s = e.to_string();
        assert!(s.contains("GetMonitorInfoW"));
        assert!(s.contains("ERROR_INVALID_HANDLE"));
    }
}
