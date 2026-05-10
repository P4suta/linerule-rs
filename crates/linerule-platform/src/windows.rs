//! Windows implementation of the platform traits.
//!
//! Wires winit (window) + vello/wgpu/peniko (GPU 2D) + the Win32
//! `windows` crate (click-through extended window styles) +
//! `global-hotkey` (system-wide chords).
//!
//! All real OS calls land in task #11; this file currently exposes
//! only the public constructors required by `lib.rs`'s re-exports.

use crate::{HotkeyError, HotkeyHost, MouseError, MouseTracker, OverlaySurface, SurfaceError};

/// Open the overlay surface bound to the monitor containing the cursor.
///
/// # Errors
/// Returns [`SurfaceError::Create`] if window creation, vello/wgpu init,
/// or click-through application fails. The current scaffold always
/// returns this error (real impl lands in task #11).
pub fn open() -> Result<Box<dyn OverlaySurface>, SurfaceError> {
    Err(SurfaceError::Create(
        "Windows overlay impl pending (task #11)".into(),
    ))
}

/// Open a system-wide hotkey host.
///
/// # Errors
/// Returns [`HotkeyError::OsReject`] if the host cannot be registered.
pub fn hotkeys() -> Result<Box<dyn HotkeyHost>, HotkeyError> {
    Err(HotkeyError::OsReject(
        "Windows hotkey host impl pending (task #11)".into(),
    ))
}

/// Open the cursor-position tracker.
///
/// # Errors
/// Returns [`MouseError::Query`] if the tracker cannot be initialised.
pub fn mouse() -> Result<Box<dyn MouseTracker>, MouseError> {
    Err(MouseError::Query(
        "Windows mouse tracker impl pending (task #11)".into(),
    ))
}
