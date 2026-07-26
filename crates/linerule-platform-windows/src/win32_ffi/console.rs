//! Console attachment FFI.

#![allow(unsafe_code, reason = "Win32 FFI boundary")]

use windows::Win32::Foundation::GetLastError;
use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AllocConsole, AttachConsole};

use crate::error::{PlatformError, Result, decode_last_error};

pub fn attach() -> Result<()> {
    // SAFETY: process-wide console attachment has no pointer arguments.
    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }.is_ok() {
        return Ok(());
    }
    // ERROR_ACCESS_DENIED means the process already has a console.
    // SAFETY: retrieves the calling thread's last-error value.
    let attach_code = unsafe { GetLastError() }.0;
    if attach_code == 5 {
        return Ok(());
    }
    // SAFETY: AllocConsole has no pointer arguments.
    unsafe { AllocConsole() }.map_err(|_| {
        // SAFETY: retrieves the calling thread's last-error value.
        let code = unsafe { GetLastError() }.0;
        PlatformError::LastError {
            operation: "AllocConsole",
            code,
            symbol: decode_last_error(code),
        }
    })
}
