//! Helper to build the overlay / HUD renderers on the WinRT
//! `Windows.UI.Composition` backend.

#![forbid(unsafe_code)]
#![cfg(windows)]

use linerule_core::HudConfig;
use windows::Win32::Foundation::HWND;

use crate::error::Result;
use crate::winrt_composition_renderer::WinrtCompositionRenderer;
use crate::winrt_hud_renderer::WinrtHudRenderer;

/// Build the overlay and HUD renderers on WinRT composition. The HUD shares the
/// overlay's pipeline (graphics device).
///
/// # Errors
/// When building the pipeline, overlay, or HUD renderer fails.
pub fn build_backends(
    hwnd: HWND,
    hud_config: &HudConfig,
) -> Result<(WinrtCompositionRenderer, WinrtHudRenderer)> {
    let overlay = WinrtCompositionRenderer::new(hwnd)?;
    let hud = WinrtHudRenderer::new(overlay.pipeline(), hud_config)?;
    Ok((overlay, hud))
}
