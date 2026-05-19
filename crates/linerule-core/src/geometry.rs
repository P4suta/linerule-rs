//! Coordinate-space-tagged geometry types.
//!
//! The phantom marker [`Logical`] / [`Physical`] guarantees that a
//! `Point<Logical>` cannot be silently passed to a function expecting a
//! `Point<Physical>` — the whole rendering pipeline operates in logical
//! pixels and the conversion to physical happens at the GPU boundary in
//! `linerule-platform-windows`.

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

/// Sealing module: only the markers in this file may implement [`CoordSpace`].
mod sealed {
    /// Sealing trait. Cannot be implemented outside `linerule_core::geometry`.
    pub trait Sealed {}
}

/// Sealed marker trait for coordinate spaces.
///
/// Only [`Logical`] and [`Physical`] satisfy it; downstream crates have no
/// reason to implement it and are prevented from doing so by the
/// `sealed::Sealed` supertrait.
///
/// The `Copy + Eq + Hash` supertraits let derives on `Point<S>` and
/// `ScreenRect<S>` propagate cleanly through generic impl blocks (otherwise
/// `S: CoordSpace` alone wouldn't satisfy the derive-emitted bounds).
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a coordinate space. Use `Logical` or `Physical`.",
    note = "`CoordSpace` is sealed; only types defined in `linerule_core::geometry` implement it."
)]
pub trait CoordSpace: sealed::Sealed + Copy + std::hash::Hash + std::cmp::Eq + 'static {
    /// Short tag emitted by structured logging (`"logical"` / `"physical"`).
    const NAME: &'static str;
}

/// Logical pixels — the units the user, the config file, and the FSM all
/// reason in. DPI-independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Logical;
impl sealed::Sealed for Logical {}
impl CoordSpace for Logical {
    const NAME: &'static str = "logical";
}

/// Physical (device) pixels — only `linerule-platform-windows` uses these,
/// at the boundary where a `Point<Logical>` is multiplied by DPI scale.
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

// `PhantomData<fn() -> S>` makes `#[derive(Serialize)]` skip the field, so we
// implement Serialize manually and emit a `"space"` tag for log readability.

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
mod tests {
    use super::*;

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
}
