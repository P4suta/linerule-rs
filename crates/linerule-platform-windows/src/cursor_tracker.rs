//! `GetCursorPos` polling, yielding `Point<Logical>` for the tick pipeline.
//! Coordinates are treated as logical; no per-monitor DPI handling.

#![forbid(unsafe_code)]
#![cfg(windows)]

use linerule_core::{Logical, Point};

/// Current cursor position, or `None` on failure (e.g. locked session).
#[must_use]
pub fn poll() -> Option<Point<Logical>> {
    crate::win32_ffi::cursor_pos().ok()
}
