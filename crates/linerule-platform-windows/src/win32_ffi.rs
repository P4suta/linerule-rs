//! FFI boundary — the only `unsafe` area in the crate; submodules safe-wrap
//! Win32/COM. Every other file is `#![forbid(unsafe_code)]`.

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
pub mod input;

#[cfg(any(doc, target_os = "windows"))]
pub mod pacer;

pub use core::*;
