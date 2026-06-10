//! `HudFrame` を WinRT composition の `CompositionDrawingSurface` に DWrite 経由で
//! 描画する renderer。`hud_renderer` (Win32 DComp) の WinRT 版。
//!
//! テキスト描画本体は `dwrite::draw_hud_rows` を共有し、surface の取得だけ
//! WinRT (`CompositionDrawingSurface`) に差し替える。SpriteVisual の opacity で
//! cursor 距離 fade を multiplicative に適用する。

#![forbid(unsafe_code)]
#![cfg(windows)]

use std::collections::HashMap;

use linerule_core::{HudConfig, HudFontKey, HudFrame};
use windows::UI::Composition::{CompositionDrawingSurface, CompositionSurfaceBrush, SpriteVisual};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::DirectWrite::{IDWriteFactory, IDWriteTextFormat};
use windows_numerics::{Vector2, Vector3};

use crate::error::{Result, map_hr};
use crate::win32_ffi::composition::{
    WinrtPipeline, begin_surface_draw, create_drawing_surface, end_surface_draw,
};
use crate::win32_ffi::dwrite;

/// WinRT HUD パネル描画器。
pub struct WinrtHudRenderer {
    visual: SpriteVisual,
    surface_brush: CompositionSurfaceBrush,
    surface: Option<CompositionDrawingSurface>,
    last_size: Option<(u32, u32)>,
    dwrite_factory: IDWriteFactory,
    graphics_device: windows::UI::Composition::CompositionGraphicsDevice,
    title_family: String,
    mono_family: String,
    formats: HashMap<(HudFontKey, u32), IDWriteTextFormat>,
}

impl WinrtHudRenderer {
    /// `pipeline.hud_root` の下に HUD visual を attach した renderer を構築する。
    ///
    /// # Errors
    /// visual / brush / DWrite factory 生成が失敗したとき。
    pub fn new(pipeline: &WinrtPipeline, hud: &HudConfig) -> Result<Self> {
        let compositor = pipeline.compositor.clone();
        let visual = compositor
            .CreateSpriteVisual()
            .map_err(map_hr("Compositor::CreateSpriteVisual (HUD)"))?;
        let surface_brush = compositor
            .CreateSurfaceBrush()
            .map_err(map_hr("Compositor::CreateSurfaceBrush"))?;
        visual
            .SetBrush(&surface_brush)
            .map_err(map_hr("SpriteVisual::SetBrush (HUD)"))?;
        pipeline
            .hud_root
            .Children()
            .map_err(map_hr("ContainerVisual::Children (HUD)"))?
            .InsertAtTop(&visual)
            .map_err(map_hr("VisualCollection::InsertAtTop (HUD)"))?;
        let dwrite_factory = dwrite::create_dwrite_factory()?;
        Ok(Self {
            visual,
            surface_brush,
            surface: None,
            last_size: None,
            dwrite_factory,
            graphics_device: pipeline.graphics_device.clone(),
            title_family: hud.fonts.title_family.to_string(),
            mono_family: hud.fonts.mono_family.to_string(),
            formats: HashMap::new(),
        })
    }

    /// HUD 1 frame を描画する。WinRT が自動 commit する。
    ///
    /// # Errors
    /// surface 生成 / text format 生成 / D2D 描画が失敗したとき。
    pub fn apply(&mut self, frame: &HudFrame) -> Result<()> {
        let width = ceil_to_u32(frame.panel_width);
        let height = ceil_to_u32(frame.panel_height);

        if self.last_size != Some((width, height)) {
            #[allow(
                clippy::cast_precision_loss,
                reason = "panel サイズは数百 px、f32 精度に余裕"
            )]
            let surface =
                create_drawing_surface(&self.graphics_device, width as f32, height as f32)?;
            self.surface_brush
                .SetSurface(&surface)
                .map_err(map_hr("CompositionSurfaceBrush::SetSurface"))?;
            self.visual
                .SetSize(Vector2 {
                    X: frame.panel_width,
                    Y: frame.panel_height,
                })
                .map_err(map_hr("SpriteVisual::SetSize (HUD)"))?;
            self.surface = Some(surface);
            self.last_size = Some((width, height));
        }

        let mut row_formats: Vec<IDWriteTextFormat> = Vec::with_capacity(frame.rows.len());
        for row in &frame.rows {
            row_formats.push(self.get_or_create_format(row.font, row.font_size)?);
        }
        let drawn: Vec<dwrite::HudDrawRow<'_>> = frame
            .rows
            .iter()
            .zip(row_formats.iter())
            .map(|(row, fmt)| {
                let local_x = row.origin_x - frame.panel_left;
                let local_y = row.origin_y - frame.panel_top;
                dwrite::HudDrawRow {
                    rect: D2D_RECT_F {
                        left: local_x,
                        top: local_y,
                        right: frame.panel_width,
                        bottom: local_y + row.font_size * 1.5,
                    },
                    text: &row.text,
                    format: fmt,
                    color: row.color,
                }
            })
            .collect();

        let surface = self.surface.as_ref().expect("just created");
        let (dc, offset) = begin_surface_draw(surface)?;
        let draw = dwrite::draw_hud_rows(&dc, offset, frame.background, frame.opacity, &drawn);
        end_surface_draw(surface)?;
        draw?;

        self.visual
            .SetOffset(Vector3 {
                X: frame.panel_left,
                Y: frame.panel_top,
                Z: 0.0,
            })
            .map_err(map_hr("SpriteVisual::SetOffset (HUD)"))
    }

    /// HUD visual の opacity を `[0.0, 1.0]` に設定する (cursor 距離 fade 用)。
    ///
    /// # Errors
    /// `Visual::SetOpacity` が失敗したとき。
    pub fn set_opacity(&self, opacity: f32) -> Result<()> {
        self.visual
            .SetOpacity(opacity.clamp(0.0, 1.0))
            .map_err(map_hr("SpriteVisual::SetOpacity"))
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

fn size_to_centi(size: f32) -> u32 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "size は HudConfig 由来の正の有限 f32。centi 化で十分な精度"
    )]
    let v = (size * 100.0).round() as u32;
    v
}

fn ceil_to_u32(v: f32) -> u32 {
    if !v.is_finite() || v < 0.0 {
        return 0;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "is_finite + 非負を確認済み、ceil 後の値は u32 範囲内"
    )]
    let out = v.ceil() as u32;
    out
}
