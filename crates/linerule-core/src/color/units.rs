//! Bounded numeric newtypes used across the overlay model.
//!
//! - [`Opacity`] — overlay mask alpha (`1..=255`, perceptually mapped on output).
//! - [`DimLevel`] — mask darkness (`0..=255`).
//! - [`Thickness`] — slit width in logical pixels (`1..=2048`).
//!
//! Each newtype carries a `try_new` for boundary input and a `saturating_add`
//! for in-range bumping. All arithmetic on these values is total (no panics,
//! no overflow), which is why they are pure newtypes rather than aliases.

use serde::{Deserialize, Serialize};

use super::perceptual;
use crate::diagnostics::CoreError;

/// Overlay mask alpha. Stored value is in `[1, 255]`; conversion to the
/// on-screen alpha byte applies the CIE L\* curve via
/// [`Opacity::to_perceptual_byte`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Opacity(u8);

impl Opacity {
    /// Smallest legal opacity.
    pub const MIN: Self = Self(1);
    /// Largest legal opacity.
    pub const MAX: Self = Self(u8::MAX);

    /// Default overlay-mask opacity (`0xAA`, ~67% perceptual).
    pub const DEFAULT: Self = Self(0xAA);
    /// Default indicator-bar opacity (`0x80`, ~50% perceptual).
    pub const INDICATOR_DEFAULT: Self = Self(0x80);

    /// Construct from a raw byte.
    ///
    /// # Errors
    /// Returns [`CoreError::Opacity`] when `value == 0`.
    pub const fn try_new(value: u8) -> Result<Self, CoreError> {
        if value == 0 {
            return Err(CoreError::Opacity {
                given: value as i32,
            });
        }
        Ok(Self(value))
    }

    /// Inner byte value in `[1, 255]`.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Add `delta` saturating against `[MIN, MAX]`.
    ///
    /// # Examples
    ///
    /// ```
    /// use linerule_core::Opacity;
    /// let o = Opacity::try_new(0x80).unwrap();
    /// assert_eq!(o.saturating_add(16).get(), 0x90);
    /// // Overflows clamp to the legal range:
    /// assert_eq!(Opacity::try_new(1).unwrap().saturating_add(-1024).get(), 1);
    /// assert_eq!(Opacity::try_new(255).unwrap().saturating_add(1).get(), 255);
    /// ```
    #[must_use]
    pub fn saturating_add(self, delta: i32) -> Self {
        let next = i32::from(self.0).saturating_add(delta).clamp(1, 255);
        // After `clamp(1, 255)` the value fits in `u8`; `try_from` is total
        // here and short-circuits the cast lints without an `#[allow]`.
        u8::try_from(next).map_or(self, Self)
    }

    /// On-screen alpha byte, mapped through the CIE L\* curve.
    #[must_use]
    pub fn to_perceptual_byte(self) -> u8 {
        let linear = f32::from(self.0) / 255.0;
        let scaled = (perceptual::l_star(linear) * 255.0)
            .clamp(0.0, 255.0)
            .round();
        // `scaled` is finite and bounded to `[0.0, 255.0]` by the clamp above,
        // so the saturating `as u8` cast is exact (Rust 1.45+ semantics).
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "scaled is finite and clamped to [0, 255]"
        )]
        let byte = scaled as u8;
        byte
    }
}

/// Mask darkness — the multiplier applied to mask color before composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DimLevel(u8);

impl DimLevel {
    /// Default dim level (`0xCC`, ~80% darkness).
    pub const DEFAULT: Self = Self(0xCC);

    /// Construct from a raw byte. Full range `[0, 255]` is valid.
    #[must_use]
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    /// Inner byte value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Add `delta` saturating against `[0, 255]`.
    #[must_use]
    pub fn saturating_add(self, delta: i32) -> Self {
        let next = i32::from(self.0).saturating_add(delta).clamp(0, 255);
        u8::try_from(next).map_or(self, Self)
    }
}

/// Slit width in logical pixels. Range `[1, 2048]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Thickness(u16);

impl Thickness {
    /// Smallest legal thickness (1 pixel).
    pub const MIN: Self = Self(1);
    /// Largest legal thickness (2048 pixels).
    pub const MAX: Self = Self(2048);

    /// Default slit thickness (28 logical pixels).
    pub const DEFAULT: Self = Self(28);

    /// Construct from a raw value.
    ///
    /// # Errors
    /// Returns [`CoreError::Thickness`] when `value` is outside `[1, 2048]`.
    pub const fn try_new(value: u16) -> Result<Self, CoreError> {
        if value == 0 || value > 2048 {
            return Err(CoreError::Thickness {
                given: value as i32,
            });
        }
        Ok(Self(value))
    }

    /// Inner value in `[1, 2048]`.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }

    /// Add `delta` (signed) saturating against `[MIN, MAX]`.
    #[must_use]
    pub fn saturating_add(self, delta: i32) -> Self {
        let next = i32::from(self.0).saturating_add(delta).clamp(1, 2048);
        u16::try_from(next).map_or(self, Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_rejects_zero() {
        assert!(matches!(
            Opacity::try_new(0),
            Err(CoreError::Opacity { given: 0 })
        ));
    }

    #[test]
    fn opacity_saturating_add_clamps() {
        assert_eq!(Opacity::DEFAULT.saturating_add(1000), Opacity::MAX);
        assert_eq!(Opacity::DEFAULT.saturating_add(-1000), Opacity::MIN);
    }

    #[test]
    fn thickness_rejects_out_of_range() {
        assert!(Thickness::try_new(0).is_err());
        assert!(Thickness::try_new(2049).is_err());
        assert!(Thickness::try_new(1).is_ok());
        assert!(Thickness::try_new(2048).is_ok());
    }

    #[test]
    fn thickness_saturating_add_clamps() {
        assert_eq!(Thickness::DEFAULT.saturating_add(99_999), Thickness::MAX);
        assert_eq!(Thickness::DEFAULT.saturating_add(-99_999), Thickness::MIN);
    }
}
