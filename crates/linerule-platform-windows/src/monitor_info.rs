//! Monitor 情報取得。プライマリ + 任意座標 → 最近接モニタの 2 系統を提供する。
//!
//! `primary_bounds()` は起動時の初期値用、`bounds_for_point()` は per-tick の
//! active monitor 解決用 (multi-monitor 環境で cursor 追随)。

#![forbid(unsafe_code)]

use linerule_core::{Logical, Point, ScreenRect};

use crate::error::Result;
use crate::win32_ffi;

/// プライマリモニタの bounds を logical pixels で返す。
///
/// `MonitorFromPoint(0, 0, MONITOR_DEFAULTTOPRIMARY)` + `GetMonitorInfoW` 経由。
/// Per-monitor DPI awareness を有効化していれば、bounds は logical (DPI scaled)
/// で返ってくる。
///
/// # Errors
/// `GetMonitorInfoW` が失敗したとき (現実には起こらない)。
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

/// すべての monitor を覆う virtual screen の bounds を返す。multi-monitor
/// 環境で overlay HWND がモニタ境界を跨いで slit を引けるようにするために、
/// `primary_bounds()` ではなく本関数を起動時に使う。
///
/// # Errors
/// 現状は失敗しない（`GetSystemMetrics` は引数チェックなし）。`Result` を返すのは
/// 将来 `EnumDisplayMonitors` ベースの厳密版に差し替える時の signature 互換のため。
#[allow(
    clippy::unnecessary_wraps,
    reason = "Result は将来 EnumDisplayMonitors 版への移行のために維持する"
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

/// 与えられた点を含む（外側ならば最も近い）monitor の bounds を返す。
///
/// `MonitorFromPoint(MONITOR_DEFAULTTONEAREST)` 経由なので、cursor が画面外
/// （remote desktop で out-of-bounds 等）でも fallback する。
///
/// # Errors
/// `GetMonitorInfoW` が失敗したとき。
pub fn bounds_for_point(p: Point<Logical>) -> Result<ScreenRect<Logical>> {
    let hmonitor = win32_ffi::monitor_from_point(p.x, p.y);
    let info = win32_ffi::get_monitor_info(hmonitor)?;
    Ok(rect_from_monitorinfo(&info))
}

/// `MONITORINFO::rcMonitor` を `ScreenRect<Logical>` に変換する。
fn rect_from_monitorinfo(info: &windows::Win32::Graphics::Gdi::MONITORINFO) -> ScreenRect<Logical> {
    let r = info.rcMonitor;
    let width = u32::try_from((r.right - r.left).max(0)).unwrap_or(0);
    let height = u32::try_from((r.bottom - r.top).max(0)).unwrap_or(0);
    ScreenRect::new(Point::new(r.left, r.top), width, height)
}
