//! Overlay / HUD レンダラの backend (Win32 DComp / WinRT Composition) を束ねる enum。
//!
//! WinRT へ移行する間、両 backend を `LINERULE_COMPOSITOR` 環境変数で切り替えられる
//! ようにし、WinRT 経路が実機で安定するまで DComp を fallback として残す。

#![forbid(unsafe_code)]
#![cfg(windows)]

use linerule_core::{HudConfig, HudFrame, OverlayFrame};
use windows::Win32::Foundation::HWND;

use crate::composition_renderer::CompositionRenderer;
use crate::error::Result;
use crate::hud_renderer::HudRenderer;
use crate::winrt_composition_renderer::WinrtCompositionRenderer;
use crate::winrt_hud_renderer::WinrtHudRenderer;

/// どの composition backend を使うか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositorKind {
    /// Win32 DirectComposition (従来経路、backdrop blur 不可)。
    Dcomp,
    /// WinRT Windows.UI.Composition (backdrop blur 対応)。
    Winrt,
}

impl CompositorKind {
    /// `LINERULE_COMPOSITOR` 環境変数から解決する (`winrt` のみ WinRT、既定 `Dcomp`)。
    #[must_use]
    pub fn from_env() -> Self {
        match std::env::var("LINERULE_COMPOSITOR").ok().as_deref() {
            Some("winrt") => Self::Winrt,
            _ => Self::Dcomp,
        }
    }

    /// tracing 用の静的ラベル。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Dcomp => "dcomp",
            Self::Winrt => "winrt",
        }
    }
}

/// overlay slit レンダラの backend。
pub enum OverlayBackend {
    /// Win32 DirectComposition。
    Dcomp(CompositionRenderer),
    /// WinRT Composition。
    Winrt(WinrtCompositionRenderer),
}

impl OverlayBackend {
    /// `OverlayFrame` を visual tree に反映する。
    ///
    /// # Errors
    /// backend の `apply` が失敗したとき。
    pub fn apply(&mut self, frame: &OverlayFrame) -> Result<()> {
        match self {
            Self::Dcomp(r) => r.apply(frame),
            Self::Winrt(r) => r.apply(frame),
        }
    }
}

/// HUD レンダラの backend。
pub enum HudBackend {
    /// Win32 DirectComposition。
    Dcomp(HudRenderer),
    /// WinRT Composition。
    Winrt(WinrtHudRenderer),
}

impl HudBackend {
    /// `HudFrame` を描画する。
    ///
    /// # Errors
    /// backend の `apply` が失敗したとき。
    pub fn apply(&mut self, frame: &HudFrame) -> Result<()> {
        match self {
            Self::Dcomp(r) => r.apply(frame),
            Self::Winrt(r) => r.apply(frame),
        }
    }

    /// HUD visual の opacity を設定する (cursor 距離 fade 用)。
    ///
    /// # Errors
    /// backend の `set_opacity` が失敗したとき。
    pub fn set_opacity(&mut self, opacity: f32) -> Result<()> {
        match self {
            Self::Dcomp(r) => r.set_opacity(opacity),
            Self::Winrt(r) => r.set_opacity(opacity),
        }
    }
}

/// 指定 backend の overlay + HUD レンダラ一式を構築する。`attach_compositor` と
/// device-lost rebuild が共有する。
///
/// # Errors
/// いずれかの pipeline / renderer 構築に失敗したとき。
pub fn build_backends(
    hwnd: HWND,
    kind: CompositorKind,
    hud_config: &HudConfig,
) -> Result<(OverlayBackend, HudBackend)> {
    match kind {
        CompositorKind::Dcomp => {
            let overlay = CompositionRenderer::new(hwnd)?;
            let hud = HudRenderer::new(overlay.pipeline(), hud_config)?;
            Ok((OverlayBackend::Dcomp(overlay), HudBackend::Dcomp(hud)))
        },
        CompositorKind::Winrt => {
            let overlay = WinrtCompositionRenderer::new(hwnd)?;
            let hud = WinrtHudRenderer::new(overlay.pipeline(), hud_config)?;
            Ok((OverlayBackend::Winrt(overlay), HudBackend::Winrt(hud)))
        },
    }
}
