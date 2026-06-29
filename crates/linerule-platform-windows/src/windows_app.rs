//! Main-thread message pump. `run_message_pump` blocks until `WM_QUIT`.

#![forbid(unsafe_code)]

use crate::error::{PlatformError, Result};
use crate::win32_ffi;

/// Run the synchronous `GetMessageW` pump until `WM_QUIT`.
///
/// # Errors
/// When `GetMessageW` returns -1.
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
