//! Renderer that applies an `OverlayFrame` to a WinRT composition visual tree.
//!
//! Per-layer brush:
//! - `Brush::Solid` -> `SpriteVisual` + `CompositionColorBrush`.
//! - `Brush::Blur`  -> `SpriteVisual` + backdrop blur brush (no color veil).
//!
//! Straight-alpha `Rgba` is passed as-is (WinRT premultiplies). WinRT
//! auto-commits on the DispatcherQueue tick. Visual offset is `rect.left()/top()`,
//! size is `rect.width/height`.

#![forbid(unsafe_code)]
#![cfg(windows)]

use linerule_core::{BlurAmount, Brush, Geometry, Logical, Rgba, ScreenRect};
use windows::UI::Color;
use windows::UI::Composition::{CompositionColorBrush, SpriteVisual, VisualCollection};
use windows_numerics::{Vector2, Vector3};

use crate::error::{Result, map_hr};
use crate::win32_ffi::blur_effect::{BlurConfig, create_backdrop_blur_brush};
use crate::win32_ffi::composition::{WinrtPipeline, create_winrt_pipeline};

/// Pooled sprite contents. `Blur` keeps `amount` because the sigma is baked
/// into the brush; a change forces a pool rebuild (kind-signature match).
enum SpriteKind {
    Solid(CompositionColorBrush),
    Blur { amount: BlurAmount },
}

/// One layer's `SpriteVisual` and brush state; remembers the last rect/brush to
/// skip unchanged updates.
struct PooledSprite {
    visual: SpriteVisual,
    kind: SpriteKind,
    last_rect: Option<ScreenRect<Logical>>,
    last_brush: Option<Brush>,
}

impl PooledSprite {
    /// This sprite's blur sigma, or `None` for `Solid`.
    const fn blur_amount(&self) -> Option<BlurAmount> {
        match self.kind {
            SpriteKind::Blur { amount, .. } => Some(amount),
            SpriteKind::Solid(_) => None,
        }
    }
}

/// Holds the WinRT composition tree and applies `OverlayFrame`s to it.
pub struct WinrtCompositionRenderer {
    pipeline: WinrtPipeline,
    layers: Vec<PooledSprite>,
    /// Blur post-process tuning, read from env once here (not per rebuild).
    blur: BlurConfig,
}

impl WinrtCompositionRenderer {
    /// Build a renderer with a WinRT composition tree attached to `hwnd`.
    ///
    /// # Errors
    /// When building the WinRT pipeline fails.
    pub fn new(hwnd: windows::Win32::Foundation::HWND) -> Result<Self> {
        let pipeline = create_winrt_pipeline(hwnd)?;
        Ok(Self {
            pipeline,
            layers: Vec::new(),
            blur: BlurConfig::from_env(),
        })
    }

    /// Borrow the shared pipeline (so `WinrtHudRenderer::new` can share the
    /// graphics device).
    #[must_use]
    pub fn pipeline(&self) -> &WinrtPipeline {
        &self.pipeline
    }

    /// Apply an `OverlayFrame` to the visual tree. WinRT auto-commits.
    ///
    /// # Errors
    /// When creating or updating a visual or brush fails.
    pub fn apply(&mut self, frame: &linerule_core::OverlayFrame) -> Result<()> {
        // Rebuild the pool when the brush-kind signature changes. Per-slot
        // swaps would break z-order, so a mismatch tears down and rebuilds in
        // index order. Sigma is baked into the blur brush (not settable via
        // SetColor), so it is part of the signature and triggers a rebuild too.
        let want_kinds: Vec<Option<BlurAmount>> = frame
            .layers()
            .iter()
            .map(|l| match l.brush {
                Brush::Blur { amount, .. } => Some(amount),
                Brush::Solid(_) => None,
            })
            .collect();
        let cur_kinds: Vec<Option<BlurAmount>> =
            self.layers.iter().map(PooledSprite::blur_amount).collect();
        if want_kinds != cur_kinds {
            self.rebuild_pool(&want_kinds)?;
        }

        for (i, layer) in frame.layers().iter().enumerate() {
            let Geometry::Rect(rect) = layer.geometry;
            let pooled = &mut self.layers[i];
            if pooled.last_brush != Some(layer.brush) {
                apply_brush_color(&pooled.visual, &pooled.kind, layer.brush)?;
                pooled.last_brush = Some(layer.brush);
            }
            if pooled.last_rect != Some(rect) {
                pooled
                    .visual
                    .SetSize(rect_size(rect))
                    .map_err(map_hr("SpriteVisual::SetSize"))?;
                pooled
                    .visual
                    .SetOffset(rect_offset(rect))
                    .map_err(map_hr("SpriteVisual::SetOffset"))?;
                pooled.last_rect = Some(rect);
            }
        }
        Ok(())
    }

    fn overlay_children(&self) -> Result<VisualCollection> {
        self.pipeline
            .overlay_root
            .Children()
            .map_err(map_hr("ContainerVisual::Children"))
    }

    /// Tear down the pool and rebuild it to match `want_kinds` (`Some` = blur,
    /// `None` = solid). Sprites are `InsertAtTop`ped in index order, so z-order
    /// follows index order (last is frontmost).
    fn rebuild_pool(&mut self, want_kinds: &[Option<BlurAmount>]) -> Result<()> {
        let children = self.overlay_children()?;
        for popped in self.layers.drain(..) {
            children
                .Remove(&popped.visual)
                .map_err(map_hr("VisualCollection::Remove"))?;
        }
        for &want in want_kinds {
            let slot = self.create_sprite(want)?;
            children
                .InsertAtTop(&slot.visual)
                .map_err(map_hr("VisualCollection::InsertAtTop"))?;
            self.layers.push(slot);
        }
        Ok(())
    }

    /// Build a single sprite (`Some` = blur, `None` = solid). The caller inserts
    /// it into the children collection.
    fn create_sprite(&self, want: Option<BlurAmount>) -> Result<PooledSprite> {
        let compositor = &self.pipeline.compositor;
        let visual = compositor
            .CreateSpriteVisual()
            .map_err(map_hr("Compositor::CreateSpriteVisual"))?;

        let kind = if let Some(amount) = want {
            // Sigma is in logical px. Put the backdrop blur brush directly on
            // the visual (no color veil).
            let blur_brush =
                create_backdrop_blur_brush(compositor, amount.to_std_dev(), &self.blur)?;
            visual
                .SetBrush(&blur_brush)
                .map_err(map_hr("SpriteVisual::SetBrush (blur)"))?;
            SpriteKind::Blur { amount }
        } else {
            let brush = compositor
                .CreateColorBrush()
                .map_err(map_hr("Compositor::CreateColorBrush"))?;
            visual
                .SetBrush(&brush)
                .map_err(map_hr("SpriteVisual::SetBrush"))?;
            SpriteKind::Solid(brush)
        };

        Ok(PooledSprite {
            visual,
            kind,
            last_rect: None,
            last_brush: None,
        })
    }
}

fn apply_brush_color(visual: &SpriteVisual, kind: &SpriteKind, brush: Brush) -> Result<()> {
    match (kind, brush) {
        // Solid sprites bake all alpha (opacity × master envelope) into the
        // brush color, so the visual's own opacity is never touched here.
        (SpriteKind::Solid(color_brush), Brush::Solid(c)) => color_brush
            .SetColor(rgba_to_color(c))
            .map_err(map_hr("CompositionColorBrush::SetColor")),
        // Blur sprites have no color and sigma changes are handled in
        // rebuild_pool; only the master-envelope opacity is applied here, at
        // the visual level, so show/hide fades never rebuild the pool. The
        // perceptual curve matches the Solid path's envelope handling
        // (`composite_alpha` in linerule-core).
        (SpriteKind::Blur { .. }, Brush::Blur { opacity, .. }) => visual
            .SetOpacity(linerule_core::color::perceptual::smooth(
                f32::from(opacity) / 255.0,
            ))
            .map_err(map_hr("SpriteVisual::SetOpacity (blur)")),
        // Other combinations are unreachable (the pool signature matches the
        // frame), so no-op.
        _ => Ok(()),
    }
}

/// Map straight-alpha `Rgba` to WinRT `Color` (straight ARGB).
fn rgba_to_color(c: Rgba) -> Color {
    Color {
        A: c.a,
        R: c.r,
        G: c.g,
        B: c.b,
    }
}

#[allow(
    clippy::cast_precision_loss,
    reason = "screen pixel coords fit in the f32 mantissa"
)]
fn rect_offset(rect: ScreenRect<Logical>) -> Vector3 {
    Vector3 {
        X: rect.left() as f32,
        Y: rect.top() as f32,
        Z: 0.0,
    }
}

#[allow(
    clippy::cast_precision_loss,
    reason = "screen pixel coords fit in the f32 mantissa"
)]
fn rect_size(rect: ScreenRect<Logical>) -> Vector2 {
    Vector2 {
        X: rect.width as f32,
        Y: rect.height as f32,
    }
}
