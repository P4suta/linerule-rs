//! Overlay / HUD レンダラの構築ヘルパ。
//!
//! composition backend は WinRT `Windows.UI.Composition` 単一。`attach_compositor`
//! と device-lost rebuild がこの 1 箇所でレンダラ一式を組み立てる。

#![forbid(unsafe_code)]
#![cfg(windows)]

use linerule_core::HudConfig;
use windows::Win32::Foundation::HWND;

use crate::error::Result;
use crate::winrt_composition_renderer::WinrtCompositionRenderer;
use crate::winrt_hud_renderer::WinrtHudRenderer;

/// overlay slit + HUD のレンダラ一式を WinRT composition で構築する。HUD は
/// overlay の共有 pipeline (graphics device) を借りて同じ device 上に描く。
///
/// # Errors
/// WinRT pipeline / overlay / HUD レンダラのいずれかの構築に失敗したとき。
pub fn build_backends(
    hwnd: HWND,
    hud_config: &HudConfig,
) -> Result<(WinrtCompositionRenderer, WinrtHudRenderer)> {
    let overlay = WinrtCompositionRenderer::new(hwnd)?;
    let hud = WinrtHudRenderer::new(overlay.pipeline(), hud_config)?;
    Ok((overlay, hud))
}
