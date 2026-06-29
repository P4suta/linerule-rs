//! Bounded numeric newtypes for the overlay model. All arithmetic is total
//! (saturating clamps, no panics/overflow).

use serde::{Deserialize, Serialize};

use super::perceptual;
use crate::diagnostics::CoreError;

/// Overlay mask alpha in `[1, 255]`; mapped to the on-screen byte via the
/// CIE L\* curve in [`Opacity::to_perceptual_byte`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Opacity(u8);

impl Opacity {
    /// Smallest legal opacity.
    pub const MIN: Self = Self(1);
    /// Largest legal opacity.
    pub const MAX: Self = Self(u8::MAX);

    /// Default overlay-mask opacity (`0xAA`, ~67% perceptual).
    pub const DEFAULT: Self = Self(0xAA);

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
        // Post-clamp value fits u8; `try_from` avoids a cast `#[allow]`.
        u8::try_from(next).map_or(self, Self)
    }

    /// On-screen alpha byte, mapped through the CIE L\* curve.
    #[must_use]
    pub fn to_perceptual_byte(self) -> u8 {
        let linear = f32::from(self.0) / 255.0;
        let scaled = (perceptual::l_star(linear) * 255.0)
            .clamp(0.0, 255.0)
            .round();
        // `scaled` is finite and clamped to `[0.0, 255.0]`, so `as u8` is exact.
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

/// Backdrop-blur perceptual *level* in `[1, 255]` (not σ).
///
/// Mapped to a Gaussian σ in logical px via [`BlurAmount::to_std_dev`], which
/// spaces σ geometrically so uniform level steps feel uniform (Weber–Fechner).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BlurAmount(u8);

impl BlurAmount {
    /// Smallest legal level (minimum σ, ~2 logical px).
    pub const MIN: Self = Self(1);
    /// Largest legal level (maximum σ, ~64 logical px).
    pub const MAX: Self = Self(255);

    /// Default level — `to_std_dev` ≈ 9 px.
    pub const DEFAULT: Self = Self(111);

    /// σ (logical px) at [`MIN`](Self::MIN).
    const SIGMA_MIN_PX: f32 = 2.0;
    /// σ (logical px) at [`MAX`](Self::MAX).
    const SIGMA_MAX_PX: f32 = 64.0;
    // Spell px values out in public docs: `SIGMA_*_PX` are private and rustdoc
    // `-D warnings` rejects public→private intra-doc links.

    /// Inner level byte in `[1, 255]` (perceptual index, not σ — use
    /// [`to_std_dev`](Self::to_std_dev) for pixel radius).
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Add `delta` (level units) saturating against `[MIN, MAX]`.
    #[must_use]
    pub fn saturating_add(self, delta: i32) -> Self {
        let next = i32::from(self.0)
            .saturating_add(delta)
            .clamp(i32::from(Self::MIN.0), i32::from(Self::MAX.0));
        u8::try_from(next).map_or(self, Self)
    }

    /// Gaussian σ (logical px), interpolated geometrically between ~2 px at
    /// [`MIN`](Self::MIN) and ~64 px at [`MAX`](Self::MAX).
    #[must_use]
    pub fn to_std_dev(self) -> f32 {
        let span = f32::from(Self::MAX.0 - Self::MIN.0);
        let t = f32::from(self.0 - Self::MIN.0) / span; // [0, 1]
        Self::SIGMA_MIN_PX * (Self::SIGMA_MAX_PX / Self::SIGMA_MIN_PX).powf(t)
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

    #[test]
    fn opacity_get_returns_constructor_byte() {
        assert_eq!(Opacity::DEFAULT.get(), 0xAA);
        assert_eq!(Opacity::MIN.get(), 1);
        assert_eq!(Opacity::MAX.get(), 255);
        assert_eq!(Opacity::try_new(42).unwrap().get(), 42);
    }

    #[test]
    fn dim_level_get_returns_constructor_byte() {
        assert_eq!(DimLevel::DEFAULT.get(), 0xCC);
        assert_eq!(DimLevel::new(0).get(), 0);
        assert_eq!(DimLevel::new(255).get(), 255);
        assert_eq!(DimLevel::new(42).get(), 42);
    }

    #[test]
    fn thickness_get_returns_constructor_value() {
        assert_eq!(Thickness::DEFAULT.get(), 28);
        assert_eq!(Thickness::MIN.get(), 1);
        assert_eq!(Thickness::MAX.get(), 2048);
        assert_eq!(Thickness::try_new(100).unwrap().get(), 100);
    }

    #[test]
    fn blur_amount_constants_are_pinned() {
        assert_eq!(BlurAmount::MIN.get(), 1);
        assert_eq!(BlurAmount::MAX.get(), 255);
        assert_eq!(BlurAmount::DEFAULT.get(), 111);
    }

    #[test]
    fn blur_amount_saturating_add_clamps() {
        assert_eq!(BlurAmount::DEFAULT.saturating_add(8).get(), 119);
        assert_eq!(BlurAmount::DEFAULT.saturating_add(-8).get(), 103);
        assert_eq!(BlurAmount::DEFAULT.saturating_add(99_999), BlurAmount::MAX);
        assert_eq!(BlurAmount::DEFAULT.saturating_add(-99_999), BlurAmount::MIN);
    }

    /// `DEFAULT` σ is ≈ 9 px.
    #[test]
    fn blur_amount_default_std_dev_reproduces_legacy_9px() {
        let sigma = BlurAmount::DEFAULT.to_std_dev();
        assert!(
            (sigma - 9.0).abs() < 0.5,
            "DEFAULT σ should be ≈ 9 px, got {sigma}"
        );
    }

    /// Endpoint σ: MIN → 2 px, MAX → 64 px.
    #[test]
    fn blur_amount_endpoints_map_to_sigma_bounds() {
        assert!((BlurAmount::MIN.to_std_dev() - 2.0).abs() < 1e-4);
        assert!((BlurAmount::MAX.to_std_dev() - 64.0).abs() < 1e-3);
    }

    /// σ increases monotonically with level.
    #[test]
    fn blur_amount_std_dev_is_monotonic() {
        let mut prev = BlurAmount::MIN.to_std_dev();
        for lvl in 2..=255u8 {
            let cur = BlurAmount(lvl).to_std_dev();
            assert!(cur > prev, "σ must increase with level at {lvl}");
            prev = cur;
        }
    }

    /// Equal level steps yield equal σ ratios (Weber–Fechner): a +20 step has
    /// the same σ ratio from any starting level.
    #[test]
    fn blur_amount_equal_steps_have_equal_sigma_ratio() {
        let ratio =
            |from: u8, by: u8| BlurAmount(from + by).to_std_dev() / BlurAmount(from).to_std_dev();
        let low = ratio(40, 20);
        let mid = ratio(120, 20);
        let high = ratio(200, 20);
        assert!(
            (low - mid).abs() < 1e-3 && (mid - high).abs() < 1e-3,
            "equal level steps must share a σ ratio: low={low} mid={mid} high={high}"
        );
    }
}
