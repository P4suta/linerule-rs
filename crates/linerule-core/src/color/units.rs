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

/// Backdrop-blur amount, stored as a perceptual *level* in `[1, 255]` and mapped
/// to a Gaussian σ (logical px) on output via [`BlurAmount::to_std_dev`].
///
/// The stored byte is a level, not σ itself — exactly like [`Opacity`] stores a
/// linear byte and maps it through a perceptual curve. Perceived blur follows
/// Weber–Fechner (≈ `log σ`), so a *uniform* level step must map to a
/// *geometric* σ step to feel uniform; [`BlurAmount::to_std_dev`] interpolates σ
/// geometrically across the range. Storing the level (and deriving σ as a float)
/// also dodges the integer-rounding stalls a multiplicative step on a small σ
/// byte would hit at the low end. The opacity hotkeys' tap delta therefore lands
/// here unchanged (no special-casing) and still reads as a smooth knob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BlurAmount(u8);

impl BlurAmount {
    /// Smallest legal level (maps to the minimum σ, ~2 logical px — a
    /// barely-there frosting).
    pub const MIN: Self = Self(1);
    /// Largest legal level (maps to the maximum σ, ~64 logical px — heavy
    /// frosted glass).
    pub const MAX: Self = Self(255);

    /// Default level — chosen so [`to_std_dev`](Self::to_std_dev) ≈ 9 px, the
    /// historical hard-coded σ, so switching to the adjustable knob preserves the
    /// prior look.
    pub const DEFAULT: Self = Self(111);

    /// σ (logical px) at [`MIN`](Self::MIN) — a barely-there frosting.
    const SIGMA_MIN_PX: f32 = 2.0;
    /// σ (logical px) at [`MAX`](Self::MAX) — heavy frosted glass.
    const SIGMA_MAX_PX: f32 = 64.0;
    // NOTE: keep public doc comments from linking to the two `SIGMA_*_PX`
    // consts above — they are private, and `cargo doc -D warnings` (the `docs`
    // CI job) rejects public→private intra-doc links. Spell the px values out.

    /// Inner level byte in `[1, 255]` (a perceptual index, *not* σ — use
    /// [`to_std_dev`](Self::to_std_dev) for the pixel radius).
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Add `delta` (signed) saturating against `[MIN, MAX]`. The delta is in
    /// level units; geometric σ spacing makes equal deltas feel equal.
    #[must_use]
    pub fn saturating_add(self, delta: i32) -> Self {
        let next = i32::from(self.0)
            .saturating_add(delta)
            .clamp(i32::from(Self::MIN.0), i32::from(Self::MAX.0));
        u8::try_from(next).map_or(self, Self)
    }

    /// Gaussian σ in logical pixels for this level. σ is interpolated
    /// *geometrically* between the σ bounds (~2 px at [`MIN`](Self::MIN) and
    /// ~64 px at [`MAX`](Self::MAX)), so uniform level steps land on a
    /// Weber–Fechner-uniform (constant-ratio) σ progression.
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

    /// `Opacity::get` が constructor で渡した byte をそのまま返すことを pin する。
    #[test]
    fn opacity_get_returns_constructor_byte() {
        assert_eq!(Opacity::DEFAULT.get(), 0xAA);
        assert_eq!(Opacity::INDICATOR_DEFAULT.get(), 0x80);
        assert_eq!(Opacity::MIN.get(), 1);
        assert_eq!(Opacity::MAX.get(), 255);
        assert_eq!(Opacity::try_new(42).unwrap().get(), 42);
    }

    /// 同上で `DimLevel::get`。`DEFAULT = 0xCC` を pin する。
    #[test]
    fn dim_level_get_returns_constructor_byte() {
        assert_eq!(DimLevel::DEFAULT.get(), 0xCC);
        assert_eq!(DimLevel::new(0).get(), 0);
        assert_eq!(DimLevel::new(255).get(), 255);
        assert_eq!(DimLevel::new(42).get(), 42);
    }

    /// 同上で `Thickness::get`。
    #[test]
    fn thickness_get_returns_constructor_value() {
        assert_eq!(Thickness::DEFAULT.get(), 28);
        assert_eq!(Thickness::MIN.get(), 1);
        assert_eq!(Thickness::MAX.get(), 2048);
        assert_eq!(Thickness::try_new(100).unwrap().get(), 100);
    }

    /// `BlurAmount` のレベル定数を pin する。
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

    /// `DEFAULT` の σ は旧ハードコード値 9px を (丸めて) 再現する。
    #[test]
    fn blur_amount_default_std_dev_reproduces_legacy_9px() {
        let sigma = BlurAmount::DEFAULT.to_std_dev();
        assert!(
            (sigma - 9.0).abs() < 0.5,
            "DEFAULT σ should be ≈ 9 px, got {sigma}"
        );
    }

    /// 端点の σ を pin する (MIN→2px, MAX→64px)。
    #[test]
    fn blur_amount_endpoints_map_to_sigma_bounds() {
        assert!((BlurAmount::MIN.to_std_dev() - 2.0).abs() < 1e-4);
        assert!((BlurAmount::MAX.to_std_dev() - 64.0).abs() < 1e-3);
    }

    /// σ は level に対し単調増加。
    #[test]
    fn blur_amount_std_dev_is_monotonic() {
        let mut prev = BlurAmount::MIN.to_std_dev();
        for lvl in 2..=255u8 {
            let cur = BlurAmount(lvl).to_std_dev();
            assert!(cur > prev, "σ must increase with level at {lvl}");
            prev = cur;
        }
    }

    /// 知覚的になめらか = 等しい level 差は等しい σ 比を生む (Weber–Fechner)。
    /// 異なる起点で同じ +20 level の σ 比がほぼ一致することを pin する。
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
