//! Window class registration. Runs `RegisterClassExW` once per process and
//! caches the class atom in [`std::sync::OnceLock`].

#![forbid(unsafe_code)]

use std::sync::OnceLock;

use windows::core::{PCWSTR, w};

use crate::error::Result;
use crate::win32_ffi;

/// Overlay HWND class name; unique within the process.
pub const OVERLAY_CLASS_NAME: PCWSTR = w!("linerule-rs-overlay");

static OVERLAY_CLASS_ATOM: OnceLock<u16> = OnceLock::new();

/// Register the overlay window class once and return its class atom.
///
/// # Errors
/// When `RegisterClassExW` fails.
pub fn ensure_registered() -> Result<u16> {
    if let Some(atom) = OVERLAY_CLASS_ATOM.get() {
        return Ok(*atom);
    }
    let atom = win32_ffi::register_class(OVERLAY_CLASS_NAME, Some(win32_ffi::overlay_wnd_proc))?;
    match OVERLAY_CLASS_ATOM.set(atom) {
        Ok(()) => Ok(atom),
        Err(_) => OVERLAY_CLASS_ATOM
            .get()
            .copied()
            .ok_or(crate::error::PlatformError::Invariant {
                operation: "overlay class atom initialization",
            }),
    }
}
