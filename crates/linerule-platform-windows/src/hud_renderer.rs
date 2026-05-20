//! `HudFrame` を DComp visual + D2D surface に DWrite 経由で描画する renderer。
//!
//! [`crate::composition_renderer::CompositionRenderer`] が overlay slit 用の
//! visual tree を所有するのに対し、本 renderer は HUD パネル用の 1 visual を
//! root に attach する。z-order は HUD が前面（後から AddVisual すれば top に
//! 置かれる）。
//!
//! 描画パスは `win32_ffi::dwrite::draw_hud_to_surface` に集約されており、本
//! ファイル自体は `#![forbid(unsafe_code)]` を維持する (ADR-0006)。

#![forbid(unsafe_code)]
#![cfg(windows)]

use std::collections::HashMap;

use linerule_core::{HudConfig, HudFontKey, HudFrame};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::ID2D1DeviceContext;
use windows::Win32::Graphics::DirectComposition::{
    IDCompositionDesktopDevice, IDCompositionSurface, IDCompositionVisual2,
};
use windows::Win32::Graphics::DirectWrite::{IDWriteFactory, IDWriteTextFormat};

use crate::error::Result;
use crate::win32_ffi::{dwrite, graphics};

/// HUD パネル 1 つの描画器。
pub struct HudRenderer {
    /// HUD 用ルートビジュアル（pipeline.root の子）。
    visual: IDCompositionVisual2,
    /// 現在の HUD surface。サイズ変化時に再生成。
    surface: Option<IDCompositionSurface>,
    /// 直近のサイズ（surface 再生成判定）。
    last_size: Option<(u32, u32)>,
    /// 直近の opacity（visual_set_opacity 呼び出し判定）。
    last_opacity: f32,
    /// DWrite ファクトリ。
    dwrite_factory: IDWriteFactory,
    /// D2D デバイスコンテキスト（CompositionRenderer の pipeline と共有）。
    d2d_context: ID2D1DeviceContext,
    /// DComp デスクトップデバイス（surface 再生成のために clone 保持）。
    dcomp: IDCompositionDesktopDevice,
    /// `HudFontKey::Title` の family 名（HudConfig 由来）。
    title_family: String,
    /// `HudFontKey::Mono` の family 名。
    mono_family: String,
    /// `(font_key, size_centi)` → text format の cache。size_centi は
    /// `(f32_size * 100.0).round() as u32`。
    formats: HashMap<(HudFontKey, u32), IDWriteTextFormat>,
}

impl HudRenderer {
    /// 新しい HUD renderer を構築する。`pipeline.root` に visual を attach し、
    /// 初期 surface は遅延生成。
    ///
    /// # Errors
    /// COM 呼び出し（visual 作成 / AddVisual / DWrite factory 作成）が失敗したとき。
    pub fn new(
        pipeline: &crate::win32_ffi::graphics::DcompPipeline,
        hud: &HudConfig,
    ) -> Result<Self> {
        let visual = graphics::create_visual(&pipeline.dcomp)?;
        // HUD は overlay slit より後に root に追加され、z-order 上で前面に来る。
        graphics::root_add_visual(&pipeline.root, &visual)?;
        let dwrite_factory = dwrite::create_dwrite_factory()?;
        Ok(Self {
            visual,
            surface: None,
            last_size: None,
            last_opacity: -1.0, // 必ず初回に SetOpacity を呼ぶ sentinel
            dwrite_factory,
            d2d_context: pipeline.d2d_context.clone(),
            dcomp: pipeline.dcomp.clone(),
            title_family: hud.fonts.title_family.to_string(),
            mono_family: hud.fonts.mono_family.to_string(),
            formats: HashMap::new(),
        })
    }

    /// HUD 1 frame を描画する。
    ///
    /// # Errors
    /// surface 生成 / DWrite text format 生成 / D2D 描画が失敗したとき。
    pub fn apply(&mut self, frame: &HudFrame) -> Result<()> {
        let width = ceil_to_u32(frame.panel_width);
        let height = ceil_to_u32(frame.panel_height);

        // Surface 再生成
        if self.last_size != Some((width, height)) {
            let surface = graphics::create_surface(&self.dcomp, width, height)?;
            graphics::visual_set_content(&self.visual, Some(&surface))?;
            self.surface = Some(surface);
            self.last_size = Some((width, height));
        }

        // Text format を事前確保
        let mut drawn: Vec<dwrite::HudDrawRow<'_>> = Vec::with_capacity(frame.rows.len());
        let mut row_formats: Vec<IDWriteTextFormat> = Vec::with_capacity(frame.rows.len());
        for row in &frame.rows {
            let fmt = self.get_or_create_format(row.font, row.font_size)?;
            row_formats.push(fmt);
        }
        for (row, fmt) in frame.rows.iter().zip(row_formats.iter()) {
            let local_x = row.origin_x - frame.panel_left;
            let local_y = row.origin_y - frame.panel_top;
            // 各行の描画矩形は「行の左端から panel 右端まで」を横幅とし、縦は
            // フォント高の 1.5 倍を確保（descender 余白）。DWrite の text format
            // にデフォルトの paragraph alignment (top) と text alignment (leading)
            // を任せているので、矩形内で baseline は自動配置される。
            let rect = D2D_RECT_F {
                left: local_x,
                top: local_y,
                right: frame.panel_width,
                bottom: local_y + row.font_size * 1.5,
            };
            drawn.push(dwrite::HudDrawRow {
                rect,
                text: &row.text,
                format: fmt,
                color: row.color,
            });
        }

        let surface = self.surface.as_ref().expect("just created");
        dwrite::draw_hud_to_surface(
            surface,
            &self.d2d_context,
            frame.background,
            frame.opacity,
            &drawn,
        )?;

        // visual の位置を反映 (opacity は色に bake 済みなので visual 単位の
        // SetOpacity は呼ばない。理由は win32_ffi/graphics.rs のコメント参照)。
        graphics::visual_set_offset(&self.visual, frame.panel_left, frame.panel_top)?;
        self.last_opacity = frame.opacity;
        Ok(())
    }

    fn get_or_create_format(&mut self, font: HudFontKey, size: f32) -> Result<IDWriteTextFormat> {
        let key = (font, size_to_centi(size));
        if let Some(fmt) = self.formats.get(&key) {
            return Ok(fmt.clone());
        }
        let (family, bold) = match font {
            HudFontKey::Title => (self.title_family.as_str(), true),
            HudFontKey::Mono => (self.mono_family.as_str(), false),
        };
        let fmt = dwrite::create_text_format(&self.dwrite_factory, family, size, bold)?;
        self.formats.insert(key, fmt.clone());
        Ok(fmt)
    }
}

/// `f32` サイズを 2 位小数までの整数キーに変換する（HashMap キー用）。
fn size_to_centi(size: f32) -> u32 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "size は HudConfig 由来の正の有限 f32 (Segoe UI 系の点数)。\
                  centi 化で十分な精度"
    )]
    let v = (size * 100.0).round() as u32;
    v
}

/// `f32` 物理サイズを `u32` に上向き丸め。`saturating` 相当（NaN 等は 0 に倒す）。
fn ceil_to_u32(v: f32) -> u32 {
    if !v.is_finite() || v < 0.0 {
        return 0;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "is_finite + 非負を確認済み、ceil 後の値は u32 範囲内"
    )]
    let v = v.ceil() as u32;
    v
}

#[cfg(test)]
mod tests {
    use super::{ceil_to_u32, size_to_centi};

    #[test]
    fn size_to_centi_rounds_to_2_decimal_places() {
        // 安全に f32 で表現できる整数 × 100 を確認する。0.005 のような半端値は
        // f32 精度で 2 進数表現が exact でない（20.005_f32 は 20.00499...）ため
        // round 後の結果が処理系依存になり、Windows native と Linux 上の rustc
        // で結果が食い違う事故があった (#42)。
        assert_eq!(size_to_centi(24.0), 2400);
        assert_eq!(size_to_centi(18.5), 1850);
        assert_eq!(size_to_centi(22.0), 2200);
        assert_eq!(size_to_centi(0.0), 0);
    }

    #[test]
    fn ceil_to_u32_rounds_up() {
        assert_eq!(ceil_to_u32(519.1), 520);
        assert_eq!(ceil_to_u32(520.0), 520);
        assert_eq!(ceil_to_u32(0.0), 0);
    }

    #[test]
    fn ceil_to_u32_clamps_invalid() {
        assert_eq!(ceil_to_u32(-1.0), 0);
        assert_eq!(ceil_to_u32(f32::NAN), 0);
        assert_eq!(ceil_to_u32(f32::INFINITY), 0);
    }
}
