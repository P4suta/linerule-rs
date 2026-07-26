//! Draws a `HudFrame` to a WinRT `CompositionDrawingSurface` via DWrite.
//! Text reuses `dwrite::draw_hud_frame`; fade is the SpriteVisual's opacity.

#![forbid(unsafe_code)]
#![cfg(windows)]

use std::collections::HashMap;

use linerule_core::{HudConfig, HudFontKey, HudFrame};
use windows::UI::Composition::{CompositionDrawingSurface, CompositionSurfaceBrush, SpriteVisual};
use windows::Win32::Graphics::DirectWrite::{IDWriteFactory, IDWriteTextFormat};
use windows_numerics::{Vector2, Vector3};

use crate::error::{Result, map_hr};
use crate::win32_ffi::composition::{
    WinrtPipeline, begin_surface_draw, create_drawing_surface, end_surface_draw,
};
use crate::win32_ffi::dwrite;

/// WinRT HUD panel renderer.
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
    row_formats: Vec<IDWriteTextFormat>,
    utf16: Vec<u16>,
}

impl WinrtHudRenderer {
    /// Build a renderer with a HUD visual attached under `pipeline.hud_root`.
    ///
    /// # Errors
    /// When creating the visual, brush, or DWrite factory fails.
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
            formats: HashMap::with_capacity(4),
            row_formats: Vec::with_capacity(24),
            utf16: Vec::with_capacity(256),
        })
    }

    /// Draw one HUD frame. WinRT auto-commits.
    ///
    /// # Errors
    /// When creating the surface or text format, or D2D drawing, fails.
    pub fn apply(&mut self, frame: &HudFrame) -> Result<()> {
        let width = ceil_to_u32(frame.panel_width);
        let height = ceil_to_u32(frame.panel_height);

        // `HudTier::Hidden` intentionally produces a 0×0 frame. WinRT accepts
        // creating that drawing surface but rejects BeginDraw with
        // COR_E_INVALIDOPERATION, which would otherwise retry every telemetry
        // tick. Collapse the visual without opening a D2D draw session.
        if drawing_surface_is_empty(width, height) {
            self.visual
                .SetSize(Vector2 { X: 0.0, Y: 0.0 })
                .map_err(map_hr("SpriteVisual::SetSize (hidden HUD)"))?;
            self.visual
                .SetOpacity(0.0)
                .map_err(map_hr("SpriteVisual::SetOpacity (hidden HUD)"))?;
            self.last_size = Some((0, 0));
            return Ok(());
        }

        if self.last_size != Some((width, height)) {
            #[allow(
                clippy::cast_precision_loss,
                reason = "panel size is a few hundred px, well within f32 precision"
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

        self.row_formats.clear();
        for row in &frame.rows {
            let format = self.get_or_create_format(row.font, row.font_size)?;
            self.row_formats.push(format);
        }

        let surface = self
            .surface
            .as_ref()
            .ok_or(crate::error::PlatformError::Invariant {
                operation: "WinrtHudRenderer::apply surface initialization",
            })?;
        let (dc, offset) = begin_surface_draw(surface)?;
        let draw = dwrite::draw_hud_frame(&dc, offset, frame, &self.row_formats, &mut self.utf16);
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

    /// Set the HUD visual's opacity, clamped to `[0.0, 1.0]` (cursor-distance fade).
    ///
    /// # Errors
    /// When `Visual::SetOpacity` fails.
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
        reason = "size is a positive finite f32 from HudConfig; centi precision suffices"
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
        reason = "checked finite and non-negative; the ceil'd value is within u32 range"
    )]
    let out = v.ceil() as u32;
    out
}

const fn drawing_surface_is_empty(width: u32, height: u32) -> bool {
    width == 0 || height == 0
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::drawing_surface_is_empty;

    #[test]
    fn hidden_hud_dimensions_skip_the_drawing_surface() {
        assert!(drawing_surface_is_empty(0, 0));
        assert!(drawing_surface_is_empty(0, 100));
        assert!(drawing_surface_is_empty(100, 0));
        assert!(!drawing_surface_is_empty(100, 50));
    }
}
