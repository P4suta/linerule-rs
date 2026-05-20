//! メインスレッドのメッセージポンプ。`run_message_pump` を呼ぶと
//! `WM_QUIT` が来るまでブロックする。

#![forbid(unsafe_code)]

use crate::error::{PlatformError, Result};
use crate::win32_ffi;

/// `GetMessageW` ベースの同期メッセージポンプを実行する。
///
/// `PostQuitMessage` が呼ばれて `WM_QUIT` が流れてくるとループを抜けて `Ok(())`
/// を返す。`GetMessageW` が -1 を返した場合のみ `Err`。
///
/// # Errors
/// `GetMessageW` が -1 を返した場合（API レベルのエラー）。
pub fn run_message_pump() -> Result<()> {
    tracing::info!(target: "WindowsApp", "entering Win32 message loop");
    loop {
        match win32_ffi::pump_one() {
            Some(true) => continue,
            Some(false) => break,
            None => {
                return Err(PlatformError::LastError {
                    operation: "GetMessageW",
                    code: 0,
                    symbol: "GetMessageW returned -1",
                });
            },
        }
    }
    tracing::info!(target: "WindowsApp", "Win32 message loop exited");
    Ok(())
}
