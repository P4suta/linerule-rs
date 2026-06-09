//! `OverlayFrame` を WinRT composition visual tree に反映するレンダラ。
//!
//! `composition_renderer` (Win32 DComp) の WinRT 版。layer の brush に応じて:
//! - `Brush::Solid` → `SpriteVisual` + `CompositionColorBrush`
//! - `Brush::Blur`  → `SpriteVisual` + backdrop blur effect brush、その子に tint の
//!   `CompositionColorBrush` sprite を重ねる (frosted glass)
//!
//! 色は WinRT が premultiply するので straight alpha の `Rgba` をそのまま渡す。
//! WinRT は DispatcherQueue tick で自動 commit する。座標は Win32 DComp 経路と同じ
//! 規約 (visual offset = `rect.left()/top()`、size = `rect.width/height`) を使う。

#![forbid(unsafe_code)]
#![cfg(windows)]

use linerule_core::{Brush, Geometry, Logical, Rgba, ScreenRect};
use windows::UI::Color;
use windows::UI::Composition::{CompositionColorBrush, SpriteVisual, VisualCollection};
use windows_numerics::{Vector2, Vector3};

use crate::error::{PlatformError, Result};
use crate::win32_ffi::blur_effect::create_backdrop_blur_brush;
use crate::win32_ffi::composition::{WinrtPipeline, create_winrt_pipeline};

/// Gaussian blur の標準偏差 (logical px 基準)。半径 ≈ 3σ。
const BLUR_STD_DEV: f32 = 9.0;

/// pooled sprite の中身。`Solid` は単色 brush、`Blur` は backdrop blur sprite +
/// tint の子 sprite を持つ。
enum SpriteKind {
    Solid(CompositionColorBrush),
    Blur { tint_brush: CompositionColorBrush },
}

/// 1 layer ぶんの主 `SpriteVisual` と brush 状態。前回の rect / brush を覚え、
/// 変化が無ければ更新をスキップする。
struct PooledSprite {
    visual: SpriteVisual,
    kind: SpriteKind,
    last_rect: Option<ScreenRect<Logical>>,
    last_brush: Option<Brush>,
}

impl PooledSprite {
    const fn is_blur(&self) -> bool {
        matches!(self.kind, SpriteKind::Blur { .. })
    }
}

/// WinRT composition tree を保持し、`OverlayFrame` をその状態に反映する。
pub struct WinrtCompositionRenderer {
    pipeline: WinrtPipeline,
    layers: Vec<PooledSprite>,
}

impl WinrtCompositionRenderer {
    /// 指定 HWND に WinRT composition tree を attach した renderer を構築する。
    ///
    /// # Errors
    /// WinRT pipeline 構築に失敗したとき。
    pub fn new(hwnd: windows::Win32::Foundation::HWND) -> Result<Self> {
        let pipeline = create_winrt_pipeline(hwnd)?;
        Ok(Self {
            pipeline,
            layers: Vec::new(),
        })
    }

    /// 共有 pipeline を borrow する (`WinrtHudRenderer::new` から graphics device を
    /// 共有するために使う)。
    #[must_use]
    pub fn pipeline(&self) -> &WinrtPipeline {
        &self.pipeline
    }

    /// `OverlayFrame` の内容を visual tree に反映する。WinRT が自動 commit する。
    ///
    /// # Errors
    /// visual / brush 生成・更新が失敗したとき。
    pub fn apply(&mut self, frame: &linerule_core::OverlayFrame) -> Result<()> {
        // brush-kind の並び (Solid/Blur) が変わったら pool を idx 順に作り直す。
        // 個別 slot の差し替え + InsertAtTop だと z-order が崩れるため、kind 列が
        // 一致しないときは全 teardown → 順次再構築する (effect 切替/端での layer
        // 数変化は稀なのでコストは無視できる)。
        let want_kinds: Vec<bool> = frame
            .layers()
            .iter()
            .map(|l| matches!(l.brush, Brush::Blur { .. }))
            .collect();
        let cur_kinds: Vec<bool> = self.layers.iter().map(PooledSprite::is_blur).collect();
        if want_kinds != cur_kinds {
            self.rebuild_pool(&want_kinds)?;
        }

        for (i, layer) in frame.layers().iter().enumerate() {
            let Geometry::Rect(rect) = layer.geometry;
            let pooled = &mut self.layers[i];
            if pooled.last_brush != Some(layer.brush) {
                apply_brush_color(&pooled.kind, layer.brush)?;
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

    /// pool を `want_kinds` (true=blur) に合わせて全 teardown → idx 順に再構築する。
    /// 各 sprite を `InsertAtTop` で順に積むので z-order は idx 順 (末尾が最前面)。
    fn rebuild_pool(&mut self, want_kinds: &[bool]) -> Result<()> {
        let children = self.overlay_children()?;
        for popped in self.layers.drain(..) {
            children
                .Remove(&popped.visual)
                .map_err(map_hr("VisualCollection::Remove"))?;
        }
        for &want_blur in want_kinds {
            let slot = self.create_sprite(want_blur)?;
            children
                .InsertAtTop(&slot.visual)
                .map_err(map_hr("VisualCollection::InsertAtTop"))?;
            self.layers.push(slot);
        }
        Ok(())
    }

    /// 単一 sprite (`want_blur` に応じて solid / blur+tint) を構築して返す。
    /// children への挿入は呼び出し側 (`rebuild_pool`) が行う。
    fn create_sprite(&self, want_blur: bool) -> Result<PooledSprite> {
        let compositor = &self.pipeline.compositor;
        let visual = compositor
            .CreateSpriteVisual()
            .map_err(map_hr("Compositor::CreateSpriteVisual"))?;

        let kind = if want_blur {
            let blur = create_backdrop_blur_brush(compositor, BLUR_STD_DEV)?;
            visual
                .SetBrush(&blur)
                .map_err(map_hr("SpriteVisual::SetBrush (blur)"))?;
            // tint child: 親を埋める色 sprite を blur の上に重ねる。
            let tint = compositor
                .CreateSpriteVisual()
                .map_err(map_hr("Compositor::CreateSpriteVisual (tint)"))?;
            let tint_brush = compositor
                .CreateColorBrush()
                .map_err(map_hr("Compositor::CreateColorBrush (tint)"))?;
            tint.SetBrush(&tint_brush)
                .map_err(map_hr("SpriteVisual::SetBrush (tint)"))?;
            tint.SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })
                .map_err(map_hr("Visual::SetRelativeSizeAdjustment (tint)"))?;
            visual
                .Children()
                .map_err(map_hr("SpriteVisual::Children"))?
                .InsertAtTop(&tint)
                .map_err(map_hr("VisualCollection::InsertAtTop (tint)"))?;
            SpriteKind::Blur { tint_brush }
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

fn apply_brush_color(kind: &SpriteKind, brush: Brush) -> Result<()> {
    match (kind, brush) {
        (SpriteKind::Solid(color_brush), Brush::Solid(c)) => color_brush
            .SetColor(rgba_to_color(c))
            .map_err(map_hr("CompositionColorBrush::SetColor")),
        (SpriteKind::Blur { tint_brush }, Brush::Blur { tint }) => tint_brush
            .SetColor(rgba_to_color(tint))
            .map_err(map_hr("CompositionColorBrush::SetColor (tint)")),
        // kind は apply_layer で brush に揃えてあるので到達しない。
        _ => Ok(()),
    }
}

/// straight-alpha `Rgba` を WinRT の `Color` (straight ARGB) にそのまま写す。
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
    reason = "screen pixel coords は f32 mantissa に収まる"
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
    reason = "screen pixel coords は f32 mantissa に収まる"
)]
fn rect_size(rect: ScreenRect<Logical>) -> Vector2 {
    Vector2 {
        X: rect.width as f32,
        Y: rect.height as f32,
    }
}

fn map_hr(operation: &'static str) -> impl Fn(windows::core::Error) -> PlatformError {
    move |e: windows::core::Error| PlatformError::BadHr {
        operation,
        hr: e.code().0,
    }
}
