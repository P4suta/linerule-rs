//! Helper to build the overlay / HUD renderers on the WinRT
//! `Windows.UI.Composition` backend.

#![forbid(unsafe_code)]
#![cfg(windows)]

use linerule_core::HudConfig;
use windows::Win32::Foundation::HWND;

use crate::error::Result;
use crate::win32_ffi::graphics::GraphicsBackend;
use crate::winrt_composition_renderer::WinrtCompositionRenderer;
use crate::winrt_hud_renderer::WinrtHudRenderer;

/// Build overlay and HUD renderers; HUD shares the overlay's pipeline.
///
/// # Errors
/// If pipeline, overlay, or HUD construction fails.
pub fn build_backends(
    hwnd: HWND,
    hud_config: &HudConfig,
    graphics_backend: GraphicsBackend,
) -> Result<(WinrtCompositionRenderer, WinrtHudRenderer)> {
    let overlay = WinrtCompositionRenderer::new(hwnd, graphics_backend)?;
    let hud = WinrtHudRenderer::new(overlay.pipeline(), hud_config)?;
    Ok((overlay, hud))
}
