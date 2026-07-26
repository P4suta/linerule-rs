//! Render-output data: an immutable list of geometry × brush layers.
//!
//! Pure data produced by [`crate::render::frame`]; the platform layer translates
//! it to D2D draw calls.

use crate::{
    color::{BlurAmount, Rgba},
    geometry::{Logical, Point, ScreenRect},
};

use super::{Axis, band};

const BEFORE_SIDE: u8 = 1;
const AFTER_SIDE: u8 = 2;

/// Fill style for a geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Brush {
    /// Fill the shape with a single sRGB color.
    Solid(Rgba),
    /// Pure backdrop blur behind the shape, no color veil.
    ///
    /// `amount` is an integer newtype (not `f32`) so `Brush` keeps its `Eq`/`Hash`
    /// derives; the platform layer converts it to a float σ at the brush boundary.
    Blur {
        /// Gaussian blur σ (logical px) for the backdrop.
        amount: BlurAmount,
        /// Master-envelope opacity byte (`255` = fully shown). Applied at the
        /// visual level (not baked into the effect brush) so show/hide fades
        /// never rebuild the sprite pool.
        opacity: u8,
    },
}

/// Shape of a layer.
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
    /// Construct a solid-filled axis-aligned rect layer.
    #[must_use]
    pub const fn solid_rect(bounds: ScreenRect<Logical>, fill: Rgba) -> Self {
        Self {
            geometry: Geometry::Rect(bounds),
            brush: Brush::Solid(fill),
        }
    }
}

/// Immutable composition frame.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct OverlayFrame {
    monitor: ScreenRect<Logical>,
    before_edge: i32,
    after_edge: i32,
    brush: Brush,
    axis: Axis,
    sides: u8,
}

impl OverlayFrame {
    /// Empty frame — emitted in `Mode::Off` or when the cursor is not yet known.
    pub const EMPTY: Self = Self {
        monitor: ScreenRect::new(Point::new(0, 0), 0, 0),
        before_edge: 0,
        after_edge: 0,
        brush: Brush::Solid(Rgba::new(0, 0, 0, 0)),
        axis: Axis::Horizontal,
        sides: 0,
    };

    pub(crate) fn from_slit(
        axis: Axis,
        monitor: ScreenRect<Logical>,
        before_edge: i32,
        after_edge: i32,
        brush: Brush,
    ) -> Self {
        let mut frame = Self {
            monitor,
            before_edge,
            after_edge,
            brush,
            axis,
            sides: 0,
        };
        if frame.layer_at(0).is_some() {
            frame.sides |= BEFORE_SIDE;
        }
        if frame.layer_at(1).is_some() {
            frame.sides |= AFTER_SIDE;
        }
        frame
    }

    /// `true` when this frame paints nothing.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.sides == 0
    }

    /// Number of layers in this frame.
    #[must_use]
    pub const fn layer_count(self) -> usize {
        self.sides.count_ones() as usize
    }

    /// Iterate over the at-most-two layers in paint order.
    pub fn layers(self) -> impl Iterator<Item = Layer> + Clone {
        self.layer_at(0).into_iter().chain(self.layer_at(1))
    }

    fn layer_at(self, side: u8) -> Option<Layer> {
        let rect = match (self.axis, side) {
            (Axis::Horizontal, 0) => band(
                self.monitor.left(),
                self.monitor.top(),
                self.monitor.right(),
                self.before_edge,
            ),
            (Axis::Horizontal, 1) => band(
                self.monitor.left(),
                self.after_edge,
                self.monitor.right(),
                self.monitor.bottom(),
            ),
            (Axis::Vertical, 0) => band(
                self.monitor.left(),
                self.monitor.top(),
                self.before_edge,
                self.monitor.bottom(),
            ),
            (Axis::Vertical, 1) => band(
                self.after_edge,
                self.monitor.top(),
                self.monitor.right(),
                self.monitor.bottom(),
            ),
            (Axis::Horizontal | Axis::Vertical, _) => None,
        }?;
        Some(Layer {
            geometry: Geometry::Rect(rect),
            brush: self.brush,
        })
    }
}

impl Default for OverlayFrame {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl core::fmt::Debug for OverlayFrame {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("OverlayFrame")
            .field("layers", &DebugLayers(*self))
            .finish()
    }
}

struct DebugLayers(OverlayFrame);

impl core::fmt::Debug for DebugLayers {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.debug_list().entries(self.0.layers()).finish()
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::geometry::Point;

    #[test]
    fn empty_frame_is_empty() {
        let f = OverlayFrame::EMPTY;
        assert_eq!(OverlayFrame::default(), f);
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

    #[test]
    fn slit_layers_are_generated_in_paint_order_without_allocating() {
        let monitor = ScreenRect::new(Point::<Logical>::new(0, 0), 100, 50);
        let frame = OverlayFrame::from_slit(
            Axis::Horizontal,
            monitor,
            20,
            30,
            Brush::Solid(Rgba::DEFAULT_MASK),
        );
        let mut iterator = frame.layers();
        assert_eq!(iterator.size_hint(), (2, Some(2)));
        let first = iterator.next().expect("before layer");
        assert_eq!(iterator.size_hint(), (1, Some(1)));
        let second = iterator.next().expect("after layer");
        assert_eq!(iterator.size_hint(), (0, Some(0)));
        assert_eq!(iterator.next(), None);
        assert_eq!(iterator.next(), None);

        let layers = [first, second];
        assert_eq!(layers.len(), 2);
        assert_eq!(
            layers[0].geometry,
            Geometry::Rect(ScreenRect::new(Point::new(0, 0), 100, 20))
        );
        assert_eq!(
            layers[1].geometry,
            Geometry::Rect(ScreenRect::new(Point::new(0, 30), 100, 20))
        );
        assert_eq!(frame.layers().count(), frame.layer_count());
        assert_eq!(core::mem::size_of::<OverlayFrame>(), 32);
    }
}
