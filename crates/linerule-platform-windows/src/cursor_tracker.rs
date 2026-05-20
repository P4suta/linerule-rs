//! `GetCursorPos` ポーリング。tick pipeline へ `Point<Logical>` を渡す。
//!
//! Per-monitor DPI 対応は Phase E では行わず、論理座標として扱う。

#![forbid(unsafe_code)]
#![cfg(windows)]

use linerule_core::{Logical, Point};

/// 直近のカーソル位置を取得する。失敗時は `None`（GetCursorPos が失敗するのは
/// セッションが locked 等の特殊状況）。
#[must_use]
pub fn poll() -> Option<Point<Logical>> {
    crate::win32_ffi::cursor_pos().ok()
}
