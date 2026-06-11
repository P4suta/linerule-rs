//! Monitor bounds: primary monitor for startup, nearest-to-point for per-tick
//! active-monitor resolution (cursor following on multi-monitor setups).

#![forbid(unsafe_code)]

use linerule_core::{Logical, Point, ScreenRect};

use crate::error::Result;
use crate::win32_ffi;

/// Primary monitor bounds in logical pixels, via `MonitorFromPoint` +
/// `GetMonitorInfoW`.
///
/// # Errors
/// When `GetMonitorInfoW` fails.
pub fn primary_bounds() -> Result<ScreenRect<Logical>> {
    let hmonitor = win32_ffi::primary_monitor();
    let info = win32_ffi::get_monitor_info(hmonitor)?;
    let rect = rect_from_monitorinfo(&info);
    tracing::debug!(
        target: "MonitorInfo",
        width = rect.width,
        height = rect.height,
        left = rect.left(),
        top = rect.top(),
        "primary monitor bounds"
    );
    Ok(rect)
}

/// Virtual screen bounds covering all monitors. Used at startup so the overlay
/// can draw slits across monitor boundaries.
///
/// # Errors
/// Does not currently fail; `Result` is kept for signature compatibility with a
/// future `EnumDisplayMonitors`-based version.
#[allow(
    clippy::unnecessary_wraps,
    reason = "Result kept for a future EnumDisplayMonitors-based version"
)]
pub fn virtual_screen_bounds() -> Result<ScreenRect<Logical>> {
    let (left, top, width, height) = win32_ffi::virtual_screen_metrics();
    let w = u32::try_from(width.max(0)).unwrap_or(0);
    let h = u32::try_from(height.max(0)).unwrap_or(0);
    let rect = ScreenRect::new(Point::new(left, top), w, h);
    tracing::debug!(
        target: "MonitorInfo",
        left,
        top,
        width = w,
        height = h,
        "virtual screen bounds"
    );
    Ok(rect)
}

/// Bounds of the monitor containing `p`, or the nearest one if `p` is outside
/// all monitors (via `MONITOR_DEFAULTTONEAREST`).
///
/// # Errors
/// When `GetMonitorInfoW` fails.
pub fn bounds_for_point(p: Point<Logical>) -> Result<ScreenRect<Logical>> {
    let hmonitor = win32_ffi::monitor_from_point(p.x, p.y);
    let info = win32_ffi::get_monitor_info(hmonitor)?;
    Ok(rect_from_monitorinfo(&info))
}

/// Convert `MONITORINFO::rcMonitor` to `ScreenRect<Logical>`.
fn rect_from_monitorinfo(info: &windows::Win32::Graphics::Gdi::MONITORINFO) -> ScreenRect<Logical> {
    let r = info.rcMonitor;
    let width = u32::try_from((r.right - r.left).max(0)).unwrap_or(0);
    let height = u32::try_from((r.bottom - r.top).max(0)).unwrap_or(0);
    ScreenRect::new(Point::new(r.left, r.top), width, height)
}
