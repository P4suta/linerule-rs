//! `OverlayFrame` を WinRT composition visual tree に反映するレンダラ。
//!
//! `composition_renderer` (Win32 DComp) の WinRT 版。layer の brush に応じて:
//! - `Brush::Solid` → `SpriteVisual` + `CompositionColorBrush`
//! - `Brush::Blur`  → `SpriteVisual` + backdrop blur effect brush (純粋なぼかし、
//!   色ベール無し)
//!
//! 色は WinRT が premultiply するので straight alpha の `Rgba` をそのまま渡す。
//! WinRT は DispatcherQueue tick で自動 commit する。座標は Win32 DComp 経路と同じ
//! 規約 (visual offset = `rect.left()/top()`、size = `rect.width/height`) を使う。

#![forbid(unsafe_code)]
#![cfg(windows)]

use linerule_core::{BlurAmount, Brush, Geometry, Logical, Rgba, ScreenRect};
use windows::UI::Color;
use windows::UI::Composition::{CompositionColorBrush, SpriteVisual, VisualCollection};
use windows_numerics::{Vector2, Vector3};

use crate::error::{Result, map_hr};
use crate::win32_ffi::blur_effect::{BlurConfig, create_backdrop_blur_brush};
use crate::win32_ffi::composition::{WinrtPipeline, create_winrt_pipeline};

/// pooled sprite の中身。`Solid` は単色 brush、`Blur` は backdrop blur brush のみ
/// (色ベール無し)。`Blur` の `amount` は brush に焼き込んだ σ を覚えておき、値が
/// 変わったら pool を作り直す (`apply` の kind 署名照合) ために保持する。
enum SpriteKind {
    Solid(CompositionColorBrush),
    Blur { amount: BlurAmount },
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
    /// この sprite の blur σ (`Solid` なら `None`)。pool 再構築要否の判定に使う。
    const fn blur_amount(&self) -> Option<BlurAmount> {
        match self.kind {
            SpriteKind::Blur { amount, .. } => Some(amount),
            SpriteKind::Solid(_) => None,
        }
    }
}

/// WinRT composition tree を保持し、`OverlayFrame` をその状態に反映する。
pub struct WinrtCompositionRenderer {
    pipeline: WinrtPipeline,
    layers: Vec<PooledSprite>,
    /// Blur post-process tuning, read from env once here (not per rebuild).
    blur: BlurConfig,
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
            blur: BlurConfig::from_env(),
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
        // brush-kind の並び (Solid / Blur(σ)) が変わったら pool を idx 順に作り直す。
        // 個別 slot の差し替え + InsertAtTop だと z-order が崩れるため、kind 列が
        // 一致しないときは全 teardown → 順次再構築する (effect 切替/端での layer
        // 数変化は稀なのでコストは無視できる)。σ は brush に焼き込まれており
        // `SetColor` では変えられないので、σ 変化も再構築トリガに含める
        // (kind 署名を `Option<BlurAmount>` にして比較する)。
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

    /// pool を `want_kinds` (`Some(σ)`=blur / `None`=solid) に合わせて全 teardown →
    /// idx 順に再構築する。各 sprite を `InsertAtTop` で順に積むので z-order は idx
    /// 順 (末尾が最前面)。
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

    /// 単一 sprite (`want` が `Some(σ)` なら blur+tint、`None` なら solid) を構築して
    /// 返す。children への挿入は呼び出し側 (`rebuild_pool`) が行う。
    fn create_sprite(&self, want: Option<BlurAmount>) -> Result<PooledSprite> {
        let compositor = &self.pipeline.compositor;
        let visual = compositor
            .CreateSpriteVisual()
            .map_err(map_hr("Compositor::CreateSpriteVisual"))?;

        let kind = if let Some(amount) = want {
            // σ は logical px。perceptual level → σ への変換は float 境界 (ここ) でのみ行う。
            // backdrop blur brush をそのまま visual に載せる (色ベール無しの純粋なぼかし)。
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

fn apply_brush_color(kind: &SpriteKind, brush: Brush) -> Result<()> {
    match (kind, brush) {
        (SpriteKind::Solid(color_brush), Brush::Solid(c)) => color_brush
            .SetColor(rgba_to_color(c))
            .map_err(map_hr("CompositionColorBrush::SetColor")),
        // Blur sprite は色を持たない (純粋なぼかし)。σ 変化は rebuild_pool で反映する
        // ので、ここでは何もしない。kind は apply_layer で brush に揃えてあるので
        // 到達しない組み合わせも含め no-op で良い。
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
