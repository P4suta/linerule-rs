//! Applies an `OverlayFrame` to a WinRT composition visual tree.
//!
//! Per layer: `Brush::Solid` -> `CompositionColorBrush`; `Brush::Blur` ->
//! backdrop blur brush (no color veil). Straight-alpha `Rgba` passed as-is
//! (WinRT premultiplies); WinRT auto-commits on the DispatcherQueue tick.

#![forbid(unsafe_code)]
#![cfg(windows)]

use linerule_core::{
    BlurAmount, Brush, Geometry, Logical, Rgba, ScreenRect, is_device_lost_hresult,
};
use windows::UI::Color;
use windows::UI::Composition::{CompositionColorBrush, SpriteVisual, VisualCollection};
use windows_numerics::{Vector2, Vector3};

use crate::error::{PlatformError, Result, map_hr};
use crate::win32_ffi::blur_effect::create_backdrop_blur_brush;
use crate::win32_ffi::composition::{WinrtPipeline, create_winrt_pipeline};
use crate::win32_ffi::graphics::GraphicsBackend;

/// Pooled sprite contents. `Blur` keeps `amount` because the sigma is baked
/// into the brush; a change forces a pool rebuild (kind-signature match).
enum SpriteKind {
    Solid(CompositionColorBrush),
    Blur {
        amount: BlurAmount,
    },
    FallbackDim {
        brush: CompositionColorBrush,
        amount: BlurAmount,
    },
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
            SpriteKind::Blur { amount, .. } | SpriteKind::FallbackDim { amount, .. } => {
                Some(amount)
            },
            SpriteKind::Solid(_) => None,
        }
    }
}

/// Holds the WinRT composition tree and applies `OverlayFrame`s to it.
pub struct WinrtCompositionRenderer {
    pipeline: WinrtPipeline,
    layers: Vec<PooledSprite>,
}

impl WinrtCompositionRenderer {
    /// Build a renderer with a WinRT composition tree attached to `hwnd`.
    ///
    /// # Errors
    /// When building the WinRT pipeline fails.
    pub fn new(
        hwnd: windows::Win32::Foundation::HWND,
        graphics_backend: GraphicsBackend,
    ) -> Result<Self> {
        let pipeline = create_winrt_pipeline(hwnd, graphics_backend)?;
        Ok(Self {
            pipeline,
            layers: Vec::new(),
        })
    }

    /// Borrow the shared pipeline (so `WinrtHudRenderer::new` can share the
    /// graphics device).
    #[must_use]
    pub fn pipeline(&self) -> &WinrtPipeline {
        &self.pipeline
    }

    /// Whether Backdrop Blur was unavailable and the renderer substituted Dim.
    #[must_use]
    pub fn uses_blur_fallback(&self) -> bool {
        self.layers
            .iter()
            .any(|sprite| matches!(sprite.kind, SpriteKind::FallbackDim { .. }))
    }

    /// Apply an `OverlayFrame` to the visual tree. WinRT auto-commits.
    ///
    /// # Errors
    /// When creating or updating a visual or brush fails.
    pub fn apply(&mut self, frame: &linerule_core::OverlayFrame) -> Result<()> {
        // Rebuild on brush-kind signature change; per-slot swaps would break
        // z-order. Blur sigma is baked into the brush, so it is part of the
        // signature and forces a rebuild too.
        let signature_changed = frame.layer_count() != self.layers.len()
            || frame
                .layers()
                .zip(&self.layers)
                .any(|(layer, sprite)| desired_blur(layer.brush) != sprite.blur_amount());
        if signature_changed {
            self.rebuild_pool(frame)?;
        }

        for (i, layer) in frame.layers().enumerate() {
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

    /// Tear down the pool and rebuild it to match `frame`. Sprites are
    /// `InsertAtTop`ped in index order, so z-order follows index order (last is
    /// frontmost).
    fn rebuild_pool(&mut self, frame: &linerule_core::OverlayFrame) -> Result<()> {
        let children = self.overlay_children()?;
        for popped in self.layers.drain(..) {
            children
                .Remove(&popped.visual)
                .map_err(map_hr("VisualCollection::Remove"))?;
        }
        for layer in frame.layers() {
            let slot = self.create_sprite(desired_blur(layer.brush))?;
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
            // Sigma in logical px; blur brush goes directly on the visual (no veil).
            match create_backdrop_blur_brush(compositor, amount.to_std_dev()) {
                Ok(blur_brush) => {
                    visual
                        .SetBrush(&blur_brush)
                        .map_err(map_hr("SpriteVisual::SetBrush (blur)"))?;
                    SpriteKind::Blur { amount }
                },
                Err(error) if !is_device_lost_error(&error) => {
                    tracing::warn!(
                        %error,
                        "Backdrop Blur unavailable; substituting Dim for this session"
                    );
                    let brush = compositor
                        .CreateColorBrush()
                        .map_err(map_hr("Compositor::CreateColorBrush (blur fallback)"))?;
                    visual
                        .SetBrush(&brush)
                        .map_err(map_hr("SpriteVisual::SetBrush (blur fallback)"))?;
                    SpriteKind::FallbackDim { brush, amount }
                },
                Err(error) => return Err(error),
            }
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

const fn desired_blur(brush: Brush) -> Option<BlurAmount> {
    match brush {
        Brush::Blur { amount, .. } => Some(amount),
        Brush::Solid(_) => None,
    }
}

fn apply_brush_color(visual: &SpriteVisual, kind: &SpriteKind, brush: Brush) -> Result<()> {
    match (kind, brush) {
        // Solid sprites bake all alpha into the brush color; visual opacity untouched.
        (SpriteKind::Solid(color_brush), Brush::Solid(c)) => color_brush
            .SetColor(rgba_to_color(c))
            .map_err(map_hr("CompositionColorBrush::SetColor")),
        // Blur sprites have no color; apply only master-envelope opacity at the
        // visual level so show/hide fades never rebuild the pool. Perceptual
        // curve matches the Solid envelope (`composite_alpha` in linerule-core).
        (SpriteKind::Blur { .. }, Brush::Blur { opacity, .. }) => visual
            .SetOpacity(linerule_core::perceptual_smooth(f32::from(opacity) / 255.0))
            .map_err(map_hr("SpriteVisual::SetOpacity (blur)")),
        (SpriteKind::FallbackDim { brush, .. }, Brush::Blur { opacity, .. }) => brush
            .SetColor(rgba_to_color(Rgba::new(
                0,
                0,
                0,
                fallback_dim_alpha(opacity),
            )))
            .map_err(map_hr("CompositionColorBrush::SetColor (blur fallback)")),
        // Other combinations unreachable (pool signature matches frame).
        _ => Ok(()),
    }
}

fn fallback_dim_alpha(opacity: u8) -> u8 {
    u8::try_from(u16::from(opacity) * 160 / 255).unwrap_or(160)
}

fn is_device_lost_error(error: &PlatformError) -> bool {
    matches!(
        error,
        PlatformError::BadHr { hr, .. } if is_device_lost_hresult(*hr)
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use linerule_core::Point;

    #[test]
    fn desired_blur_distinguishes_solid_and_blur_signatures() {
        assert_eq!(desired_blur(Brush::Solid(Rgba::DEFAULT_MASK)), None);
        assert_eq!(
            desired_blur(Brush::Blur {
                amount: BlurAmount::DEFAULT,
                opacity: 77,
            }),
            Some(BlurAmount::DEFAULT)
        );
    }

    #[test]
    fn blur_fallback_alpha_preserves_endpoints_and_scales_linearly() {
        assert_eq!(fallback_dim_alpha(0), 0);
        assert_eq!(fallback_dim_alpha(128), 80);
        assert_eq!(fallback_dim_alpha(u8::MAX), 160);
    }

    #[test]
    fn device_lost_filter_accepts_only_documented_hresult_values() {
        for hr in [0x887A_0005_u32, 0x887A_0006, 0x887A_0007, 0x8899_000C] {
            assert!(is_device_lost_error(&PlatformError::BadHr {
                operation: "fixture",
                hr: i32::from_ne_bytes(hr.to_ne_bytes()),
            }));
        }
        assert!(!is_device_lost_error(&PlatformError::BadHr {
            operation: "fixture",
            hr: -1,
        }));
        assert!(!is_device_lost_error(&PlatformError::AlreadyRunning));
    }

    #[test]
    fn rgba_and_rectangle_helpers_preserve_channels_and_geometry() {
        let color = rgba_to_color(Rgba::new(1, 2, 3, 4));
        assert_eq!((color.R, color.G, color.B, color.A), (1, 2, 3, 4));

        let rectangle = ScreenRect::new(Point::new(-20, 30), 640, 480);
        let offset = rect_offset(rectangle);
        let size = rect_size(rectangle);
        assert_eq!((offset.X, offset.Y, offset.Z), (-20.0, 30.0, 0.0));
        assert_eq!((size.X, size.Y), (640.0, 480.0));
    }
}
