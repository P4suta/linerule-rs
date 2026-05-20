//! `OverlayFrame` を DirectComposition visual tree に反映するレンダラ。
//!
//! データ構造: `Vec<PooledLayer>` で visual + surface のプールを保持し、frame の
//! layer 数が変わったら resize するだけ。各 layer は last_size / last_color を
//! 持って前回値と一致する場合は surface 再生成を省略する。
//!
//! Phase D 完了条件: `apply(&overlay_frame)` で transparent click-through
//! overlay に dim layer + indicator が描けること。HUD は Phase F。

#![forbid(unsafe_code)]
#![cfg(windows)]

use linerule_core::{Brush, Geometry, Layer, Logical, OverlayFrame, Rgba, ScreenRect};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::DirectComposition::{IDCompositionSurface, IDCompositionVisual2};

use crate::error::Result;
use crate::win32_ffi::graphics::{self, DcompPipeline};

/// 透明 click-through overlay 上の visual tree を保持し、`OverlayFrame` を
/// その状態に反映する責務を持つ。
pub struct CompositionRenderer {
    pipeline: DcompPipeline,
    layers: Vec<PooledLayer>,
}

/// 1 つの dim layer / indicator 等に対応する `IDCompositionVisual2` と
/// その `IDCompositionSurface` の組。前回適用したサイズ・色を覚えており、
/// 変化がなければ surface 再生成や fill をスキップする。
struct PooledLayer {
    visual: IDCompositionVisual2,
    surface: Option<IDCompositionSurface>,
    last_rect: Option<ScreenRect<Logical>>,
    last_color: Option<Rgba>,
}

impl CompositionRenderer {
    /// 指定 HWND に dcomp visual tree を attach した renderer を新規構築する。
    ///
    /// # Errors
    /// D3D11 / DXGI / D2D / DComp のいずれかの初期化に失敗したとき。
    pub fn new(hwnd: HWND) -> Result<Self> {
        let pipeline = graphics::create_dcomp_pipeline(hwnd)?;
        graphics::commit(&pipeline.dcomp)?;
        Ok(Self {
            pipeline,
            layers: Vec::new(),
        })
    }

    /// DComp + D2D パイプラインを参照で借りる。`HudRenderer::new` 等、同じ
    /// pipeline を共有して visual を attach する別 renderer を構築するために
    /// `pub(crate)` で公開する。
    pub(crate) fn pipeline(&self) -> &crate::win32_ffi::graphics::DcompPipeline {
        &self.pipeline
    }

    /// `OverlayFrame` の内容を visual tree に反映し、`Commit` で表示する。
    ///
    /// # Errors
    /// 各 COM 呼び出し（surface 作成・visual 操作・commit）が失敗したとき。
    pub fn apply(&mut self, frame: &OverlayFrame) -> Result<()> {
        self.grow_pool_to(frame.layer_count())?;
        self.shrink_pool_to(frame.layer_count())?;

        for (i, layer) in frame.layers().iter().enumerate() {
            self.apply_layer(i, *layer)?;
        }

        graphics::commit(&self.pipeline.dcomp)
    }

    fn grow_pool_to(&mut self, target: usize) -> Result<()> {
        while self.layers.len() < target {
            let visual = graphics::create_visual(&self.pipeline.dcomp)?;
            graphics::root_add_visual(&self.pipeline.root, &visual)?;
            self.layers.push(PooledLayer {
                visual,
                surface: None,
                last_rect: None,
                last_color: None,
            });
        }
        Ok(())
    }

    fn shrink_pool_to(&mut self, target: usize) -> Result<()> {
        while self.layers.len() > target {
            let popped = self.layers.pop().expect("len > target");
            graphics::root_remove_visual(&self.pipeline.root, &popped.visual)?;
            // popped が drop されると visual / surface も Drop で Release
        }
        Ok(())
    }

    fn apply_layer(&mut self, idx: usize, layer: Layer) -> Result<()> {
        let (rect, color) = decompose(layer);
        let pooled = &mut self.layers[idx];

        // サイズ変化があれば surface を再生成
        let size_changed =
            pooled.last_rect.map(|r| (r.width, r.height)) != Some((rect.width, rect.height));
        if size_changed {
            pooled.surface = Some(graphics::create_surface(
                &self.pipeline.dcomp,
                rect.width,
                rect.height,
            )?);
            graphics::visual_set_content(&pooled.visual, pooled.surface.as_ref())?;
            pooled.last_color = None; // 色は再塗装必要
        }

        // 色変化があれば fill_surface
        if pooled.last_color != Some(color) {
            if let Some(surface) = pooled.surface.as_ref() {
                graphics::fill_surface(surface, color)?;
            }
            pooled.last_color = Some(color);
        }

        // 位置はあらゆる frame で更新する（cursor 追随）
        graphics::visual_set_offset(
            &pooled.visual,
            #[allow(
                clippy::cast_precision_loss,
                reason = "screen pixel coords fit f32 mantissa"
            )]
            {
                rect.left() as f32
            },
            #[allow(
                clippy::cast_precision_loss,
                reason = "screen pixel coords fit f32 mantissa"
            )]
            {
                rect.top() as f32
            },
        )?;

        pooled.last_rect = Some(rect);
        Ok(())
    }
}

/// `Layer` を (rect, color) に分解する純粋関数。`Brush::Solid` / `Geometry::Rect`
/// 以外は将来拡張点。
pub(crate) fn decompose(layer: Layer) -> (ScreenRect<Logical>, Rgba) {
    let Geometry::Rect(rect) = layer.geometry;
    let Brush::Solid(color) = layer.brush;
    (rect, color)
}

#[cfg(test)]
mod tests {
    use super::*;
    use linerule_core::Point;

    #[test]
    fn decompose_round_trips_layer_into_constructor_args() {
        let rect = ScreenRect::new(Point::new(100, 50), 800, 400);
        let color = Rgba::new(0x12, 0x34, 0x56, 0x78);
        let layer = Layer::solid_rect(rect, color);
        let (back_rect, back_color) = decompose(layer);
        assert_eq!(back_rect, rect);
        assert_eq!(back_color, color);
    }

    #[test]
    fn decompose_preserves_zero_alpha_color() {
        let rect = ScreenRect::new(Point::new(0, 0), 1, 1);
        let color = Rgba::new(0xFF, 0xFF, 0xFF, 0x00);
        let (_, c) = decompose(Layer::solid_rect(rect, color));
        assert_eq!(c.a, 0);
    }

    #[test]
    fn decompose_preserves_origin_point() {
        let rect = ScreenRect::new(Point::new(-500, -200), 100, 100);
        let color = Rgba::DEFAULT_MASK;
        let (r, _) = decompose(Layer::solid_rect(rect, color));
        assert_eq!(r.left(), -500);
        assert_eq!(r.top(), -200);
    }
}
