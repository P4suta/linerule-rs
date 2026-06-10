//! FFI boundary — the only `unsafe` area in `linerule-platform-windows`.
//!
//! Each submodule thinly safe-wraps a Win32 / COM API. Every other file in the
//! crate is `#![forbid(unsafe_code)]` and touches Win32/COM only through here.
//!
//! - [`core`] — Window / message pump / instance state
//! - [`graphics`] — D3D11 + DXGI + D2D device stack
//! - [`composition`] — WinRT `Windows.UI.Composition` host (overlay + HUD)
//! - [`blur_effect`] — backdrop Gaussian-blur effect for the `Blur` surround
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
