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
