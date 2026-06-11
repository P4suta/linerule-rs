//! Attach a console only in CLI mode, even under `windows_subsystem =
//! "windows"`.
//!
//! Tries `AttachConsole(ATTACH_PARENT_PROCESS)`, falling back to
//! `AllocConsole`.
//!
//! This is the only place in `linerule-app` that needs `unsafe`. The Win32
//! calls are cfg-gated to Windows; a `cfg(not(windows))` no-op keeps the Linux
//! build compiling.

#![cfg_attr(not(target_os = "windows"), forbid(unsafe_code))]
#![cfg_attr(
    target_os = "windows",
    allow(unsafe_code, reason = "console attach calls Win32 directly")
)]

/// Attach the parent process console, allocating a new one if there is none, so
/// `println!` etc. become visible.
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
        // stdout/stderr rebinding happens automatically once a console exists.
    }
}
