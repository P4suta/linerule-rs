//! Render-output data: an immutable list of geometry × brush layers.
//!
//! `OverlayFrame` is produced by [`crate::render::frame`] and consumed by the
//! platform layer (`linerule-platform-windows::composition_renderer`). It is
//! pure data; the platform layer translates it to D2D draw calls.

use crate::{
    color::Rgba,
    geometry::{Logical, ScreenRect},
};

/// Fill style for a geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Brush {
    /// Fill the shape with a single sRGB color.
    Solid(Rgba),
}

/// Shape of a layer. Currently axis-aligned rectangles in logical space; new
/// variants would be added here (e.g. rounded rect for indicator pills).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Geometry {
    /// Axis-aligned rectangle in logical pixels.
    Rect(ScreenRect<Logical>),
}

/// One layer = one geometry filled with one brush. Layers paint back-to-front
/// in the order they appear inside an [`OverlayFrame`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Layer {
    /// Shape outline.
    pub geometry: Geometry,
    /// Fill style applied to the geometry.
    pub brush: Brush,
}

impl Layer {
    /// Convenience constructor for the most common case (axis-aligned
    /// filled rect).
    #[must_use]
    pub const fn solid_rect(bounds: ScreenRect<Logical>, fill: Rgba) -> Self {
        Self {
            geometry: Geometry::Rect(bounds),
            brush: Brush::Solid(fill),
        }
    }
}

/// Immutable composition frame.
///
/// Stored as a `Vec<Layer>` — the per-frame allocation cost (3 layers × 60 Hz
/// ≈ 9 KiB/s) is negligible and lets the crate stay strictly
/// `#![forbid(unsafe_code)]` without pulling in `smallvec`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct OverlayFrame {
    layers: Vec<Layer>,
}

impl OverlayFrame {
    /// Empty frame — emitted in `Mode::Off` or when the cursor is not yet known.
    pub const EMPTY: Self = Self { layers: Vec::new() };

    /// Construct a frame from a layer iterator.
    #[must_use]
    pub fn from_layers<I: IntoIterator<Item = Layer>>(layers: I) -> Self {
        Self {
            layers: layers.into_iter().collect(),
        }
    }

    /// `true` when this frame paints nothing.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// Number of layers in this frame.
    #[must_use]
    pub const fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Borrow the layer slice for composition.
    #[must_use]
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    #[test]
    fn empty_frame_is_empty() {
        let f = OverlayFrame::EMPTY;
        assert!(f.is_empty());
        assert_eq!(f.layer_count(), 0);
    }

    #[test]
    fn solid_rect_constructs_a_filled_layer() {
        let rect = ScreenRect::new(Point::<Logical>::new(0, 0), 100, 50);
        let layer = Layer::solid_rect(rect, Rgba::DEFAULT_MASK);
        assert_eq!(layer.geometry, Geometry::Rect(rect));
        assert_eq!(layer.brush, Brush::Solid(Rgba::DEFAULT_MASK));
    }
}
