//! Coordinate-space-tagged geometry types.
//!
//! The phantom [`Logical`] / [`Physical`] marker stops a `Point<Logical>` being
//! passed where `Point<Physical>` is expected; conversion happens at the GPU
//! boundary in `linerule-platform-windows`.

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

/// Seals [`CoordSpace`] to the markers in this file.
mod sealed {
    /// Sealing trait; not implementable outside this module.
    pub trait Sealed {}
}

/// Sealed marker trait for coordinate spaces.
///
/// Only [`Logical`] and [`Physical`] satisfy it. The `Copy + Eq + Hash`
/// supertraits satisfy the derive-emitted bounds on `Point<S>`/`ScreenRect<S>`.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a coordinate space. Use `Logical` or `Physical`.",
    note = "`CoordSpace` is sealed; only types defined in `linerule_core::geometry` implement it."
)]
pub trait CoordSpace: sealed::Sealed + Copy + std::hash::Hash + std::cmp::Eq + 'static {
    /// Logging tag (`"logical"` / `"physical"`).
    const NAME: &'static str;
}

/// Logical (DPI-independent) pixels; the units of the config and FSM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Logical;
impl sealed::Sealed for Logical {}
impl CoordSpace for Logical {
    const NAME: &'static str = "logical";
}

/// Physical (device) pixels; logical multiplied by DPI scale at the GPU boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Physical;
impl sealed::Sealed for Physical {}
impl CoordSpace for Physical {
    const NAME: &'static str = "physical";
}

/// 2D point in a coordinate space `S`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Point<S: CoordSpace> {
    /// X coordinate in the tagged space.
    pub x: i32,
    /// Y coordinate in the tagged space.
    pub y: i32,
    _space: PhantomData<fn() -> S>,
}

impl<S: CoordSpace> Point<S> {
    /// Construct a point in the tagged coordinate space.
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            _space: PhantomData,
        }
    }
}

// Manual Serialize (derive skips PhantomData) so logs carry a `"space"` tag.

impl<S: CoordSpace> Serialize for Point<S> {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("Point", 3)?;
        s.serialize_field("space", S::NAME)?;
        s.serialize_field("x", &self.x)?;
        s.serialize_field("y", &self.y)?;
        s.end()
    }
}

/// Axis-aligned rectangle in a coordinate space `S`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScreenRect<S: CoordSpace> {
    /// Top-left corner of the rectangle.
    pub origin: Point<S>,
    /// Width in tagged-space pixels.
    pub width: u32,
    /// Height in tagged-space pixels.
    pub height: u32,
}

impl<S: CoordSpace> ScreenRect<S> {
    /// Construct a rectangle from its top-left corner and size.
    #[must_use]
    pub const fn new(origin: Point<S>, width: u32, height: u32) -> Self {
        Self {
            origin,
            width,
            height,
        }
    }

    /// Left edge (inclusive).
    #[must_use]
    pub const fn left(self) -> i32 {
        self.origin.x
    }

    /// Top edge (inclusive).
    #[must_use]
    pub const fn top(self) -> i32 {
        self.origin.y
    }

    /// Right edge (exclusive). Saturating on overflow to keep arithmetic total.
    #[must_use]
    pub const fn right(self) -> i32 {
        self.origin.x.saturating_add_unsigned(self.width)
    }

    /// Bottom edge (exclusive). Saturating on overflow.
    #[must_use]
    pub const fn bottom(self) -> i32 {
        self.origin.y.saturating_add_unsigned(self.height)
    }

    /// `true` when `point` is strictly inside `self` (half-open edges).
    #[must_use]
    pub const fn contains(self, point: Point<S>) -> bool {
        point.x >= self.left()
            && point.x < self.right()
            && point.y >= self.top()
            && point.y < self.bottom()
    }

    /// `true` when `other` is fully covered by `self` (reflexive on equality).
    #[must_use]
    pub const fn contains_rect(self, other: Self) -> bool {
        other.left() >= self.left()
            && other.top() >= self.top()
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
    }
}

impl<S: CoordSpace> Serialize for ScreenRect<S> {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("ScreenRect", 4)?;
        s.serialize_field("space", S::NAME)?;
        s.serialize_field("origin", &self.origin)?;
        s.serialize_field("width", &self.width)?;
        s.serialize_field("height", &self.height)?;
        s.end()
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    #[test]
    fn tagged_geometry_serializes_with_its_coordinate_space() {
        let point: Point<Logical> = Point::new(-3, 7);
        assert_eq!(
            serde_json::to_value(point).expect("serialize point"),
            serde_json::json!({"space": "logical", "x": -3, "y": 7})
        );

        let rect: ScreenRect<Physical> = ScreenRect::new(Point::new(11, 13), 17, 19);
        assert_eq!(
            serde_json::to_value(rect).expect("serialize rectangle"),
            serde_json::json!({
                "space": "physical",
                "origin": {"space": "physical", "x": 11, "y": 13},
                "width": 17,
                "height": 19
            })
        );
    }

    #[test]
    fn contains_excludes_right_and_bottom_edges() {
        let r: ScreenRect<Logical> = ScreenRect::new(Point::new(0, 0), 10, 10);
        assert!(r.contains(Point::new(0, 0)));
        assert!(r.contains(Point::new(9, 9)));
        assert!(!r.contains(Point::new(10, 5)));
        assert!(!r.contains(Point::new(5, 10)));
    }

    #[test]
    fn contains_rect_is_reflexive_on_self() {
        let r: ScreenRect<Logical> = ScreenRect::new(Point::new(0, 0), 100, 50);
        assert!(r.contains_rect(r));
    }

    /// `top()` / `left()` reflect `origin` for a non-zero origin.
    #[test]
    fn top_and_left_reflect_origin_for_nonzero_points() {
        let r: ScreenRect<Logical> = ScreenRect::new(Point::new(7, 11), 100, 50);
        assert_eq!(r.left(), 7);
        assert_eq!(r.top(), 11);
        assert_eq!(r.right(), 107);
        assert_eq!(r.bottom(), 61);
    }

    /// Breaks each of `contains_rect`'s four boundary conditions independently
    /// (a rect that satisfies three but violates one must return false).
    #[test]
    fn contains_rect_rejects_each_boundary_independently() {
        let outer: ScreenRect<Logical> = ScreenRect::new(Point::new(0, 0), 100, 100);
        let inner: ScreenRect<Logical> = ScreenRect::new(Point::new(10, 10), 50, 50);
        assert!(outer.contains_rect(inner));

        // off left: other.left = -1 (outer.left = 0)
        let off_left: ScreenRect<Logical> = ScreenRect::new(Point::new(-1, 10), 50, 50);
        assert!(!outer.contains_rect(off_left), "left edge must be rejected");

        // off top: other.top = -1 (outer.top = 0)
        let off_top: ScreenRect<Logical> = ScreenRect::new(Point::new(10, -1), 50, 50);
        assert!(!outer.contains_rect(off_top), "top edge must be rejected");

        // off right: other.right = 101 (outer.right = 100)
        let off_right: ScreenRect<Logical> = ScreenRect::new(Point::new(60, 10), 41, 50);
        assert!(
            !outer.contains_rect(off_right),
            "right edge must be rejected"
        );

        // off bottom: other.bottom = 101 (outer.bottom = 100)
        let off_bottom: ScreenRect<Logical> = ScreenRect::new(Point::new(10, 60), 50, 41);
        assert!(
            !outer.contains_rect(off_bottom),
            "bottom edge must be rejected"
        );

        // Fully outside: catches a mutant that always returns true.
        let totally_outside: ScreenRect<Logical> = ScreenRect::new(Point::new(200, 200), 10, 10);
        assert!(
            !outer.contains_rect(totally_outside),
            "fully-outside rect must be rejected"
        );
    }
}
