//! ★ FFI 境界 — `linerule-platform-windows` 内で `unsafe` を含む唯一の領域。
//!
//! 各サブモジュールは Win32 / COM API を薄く safe ラップする。クレート内の他
//! ファイルは `#![forbid(unsafe_code)]` を強制し、本モジュール経由でのみ
//! Win32/COM を触る。詳細方針は ADR-0003。
//!
//! - [`core`] — Window / message pump / instance state (Phase C)
//! - [`graphics`] — D3D11 + DXGI + D2D + DComposition pipeline (Phase D)
//! - [`hotkey`] — `RegisterHotKey` (Phase E)
//! - [`pacer`] — `DwmFlush` + `PostMessageW` (Phase F)
//! - [`dwrite`] — DirectWrite text formats + DrawText (Phase G, ADR-0006)

pub mod core;

#[cfg(any(doc, target_os = "windows"))]
pub mod dwrite;

#[cfg(any(doc, target_os = "windows"))]
pub mod graphics;

#[cfg(any(doc, target_os = "windows"))]
pub mod hotkey;

#[cfg(any(doc, target_os = "windows"))]
pub mod pacer;

pub use core::*;
