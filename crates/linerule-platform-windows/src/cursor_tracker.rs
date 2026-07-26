//! `GetCursorPos` polling, yielding `Point<Logical>` for the tick pipeline.
//! Coordinates are treated as logical; no per-monitor DPI handling.

#![forbid(unsafe_code)]
#![cfg(windows)]

use linerule_core::{Logical, Point};

use crate::error::Result;

/// Current cursor position.
///
/// # Errors
/// Returns the typed `GetCursorPos` failure. A locked or disconnected session
/// is handled as a recoverable missing sample by the caller.
pub fn poll() -> Result<Point<Logical>> {
    crate::win32_ffi::cursor_pos()
}
