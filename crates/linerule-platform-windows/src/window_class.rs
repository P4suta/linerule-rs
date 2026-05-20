//! Window class registration. プロセス全体で 1 度だけ `RegisterClassExW` を
//! 走らせ、class atom を `OnceLock` にキャッシュする。
//!
//! `static mut` / `lazy_static!` / `OnceCell` は使わず [`std::sync::OnceLock`]
//! のみ ([[0002-architecture-principles]] §原則 9)。

#![forbid(unsafe_code)]

use std::sync::OnceLock;

use windows::core::{PCWSTR, w};

use crate::error::Result;
use crate::win32_ffi;

/// Overlay HWND のクラス名。プロセス内で衝突しない一意の文字列。
pub const OVERLAY_CLASS_NAME: PCWSTR = w!("linerule-rs-overlay");

static OVERLAY_CLASS_ATOM: OnceLock<u16> = OnceLock::new();

/// Overlay の window class を一度だけ登録し、class atom を返す。
///
/// # Errors
/// `RegisterClassExW` が失敗したとき。
pub fn ensure_registered() -> Result<u16> {
    if let Some(atom) = OVERLAY_CLASS_ATOM.get() {
        return Ok(*atom);
    }
    let atom = win32_ffi::register_class(OVERLAY_CLASS_NAME, Some(win32_ffi::overlay_wnd_proc))?;
    // 競合が起きても (set が Err) class atom は同じなので無視する。
    let _ = OVERLAY_CLASS_ATOM.set(atom);
    Ok(atom)
}
