//! Attach a console in CLI mode despite `windows_subsystem = "windows"`.
//! Only place in `linerule-app` needing `unsafe`; non-Windows is a no-op.

#![cfg_attr(not(target_os = "windows"), forbid(unsafe_code))]
#![cfg_attr(
    target_os = "windows",
    allow(unsafe_code, reason = "console attach calls Win32 directly")
)]

/// Attach the parent console, allocating one if none, so `println!` is visible.
pub(crate) fn ensure_console_attached() {
    #[cfg(target_os = "windows")]
    win::ensure_console_attached();
    #[cfg(not(target_os = "windows"))]
    {
        // Non-Windows targets already have a console.
    }
}

#[cfg(target_os = "windows")]
mod win {
    use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AllocConsole, AttachConsole};

    pub(crate) fn ensure_console_attached() {
        // SAFETY: AttachConsole is harmless on failure; fall back to AllocConsole.
        let attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }.is_ok();
        if !attached {
            // SAFETY: AllocConsole allocates a new console.
            let _ = unsafe { AllocConsole() };
        }
        // stdout/stderr rebind automatically once a console exists.
    }
}
