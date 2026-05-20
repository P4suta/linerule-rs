//! Monitor 情報取得。Phase C ではプライマリモニタの bounds だけを返す。
//! multi-monitor 対応は Phase D 以降で必要に応じて拡張する。

#![forbid(unsafe_code)]

use linerule_core::{Logical, Point, ScreenRect};

use crate::error::Result;
use crate::win32_ffi;

/// プライマリモニタの bounds を logical pixels で返す。
///
/// `GetSystemMetrics(SM_CXSCREEN/SM_CYSCREEN)` ベース。`linerule-cs::MonitorInfo`
/// と同じシンプル実装で、複数モニタや per-monitor DPI は Phase D 以降で扱う。
///
/// # Errors
/// `GetSystemMetrics` が負値を返したとき (現実には起こらない)。
pub fn primary_bounds() -> Result<ScreenRect<Logical>> {
    let w = win32_ffi::screen_width();
    let h = win32_ffi::screen_height();
    let width = u32::try_from(w.max(0)).unwrap_or(0);
    let height = u32::try_from(h.max(0)).unwrap_or(0);

    tracing::debug!(
        target: "MonitorInfo",
        width,
        height,
        "primary monitor bounds"
    );
    Ok(ScreenRect::new(Point::new(0, 0), width, height))
}
