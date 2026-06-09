//! ★ FFI 境界 — `linerule-platform-windows` 内で `unsafe` を含む唯一の領域。
//!
//! 各サブモジュールは Win32 / COM API を薄く safe ラップする。クレート内の他
//! ファイルは `#![forbid(unsafe_code)]` を強制し、本モジュール経由でのみ
//! Win32/COM を触る。
//!
//! - [`core`] — Window / message pump / instance state
//! - [`graphics`] — D3D11 + DXGI + D2D + DComposition pipeline
//! - [`hotkey`] — `RegisterHotKey`
//! - [`pacer`] — `DwmFlush` + `PostMessageW`
//! - [`dwrite`] — DirectWrite text formats + DrawText

pub mod core;

#[cfg(any(doc, target_os = "windows"))]
pub mod accessibility;

#[cfg(any(doc, target_os = "windows"))]
pub mod blur_effect;

#[cfg(any(doc, target_os = "windows"))]
pub mod composition;

#[cfg(any(doc, target_os = "windows"))]
pub mod dwrite;

#[cfg(any(doc, target_os = "windows"))]
pub mod graphics;

#[cfg(any(doc, target_os = "windows"))]
pub mod hotkey;

#[cfg(any(doc, target_os = "windows"))]
pub mod pacer;

pub use core::*;
