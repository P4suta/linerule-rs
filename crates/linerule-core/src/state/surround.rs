//! Surround effect: treatment of the masked region around the (clear) slit.
//!
//! Colors are compile-time constants; only the selection is runtime-mutable.

use serde::{Deserialize, Serialize};

use crate::color::Rgba;

/// Treatment of the region surrounding the slit. Cycle: `DimBlack → WhiteWash → Blur`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurroundEffect {
    /// Darken the surround with a translucent black mask.
    #[default]
    DimBlack,
    /// Translucent white mask; suited to bright environments / white documents.
    WhiteWash,
    /// Pure backdrop blur, no color veil (rendered by the `WinRT` backend).
    Blur,
}

impl SurroundEffect {
    /// Advance to the next effect in the canonical cycle.
    #[must_use]
    pub const fn cycle(self) -> Self {
        match self {
            Self::DimBlack => Self::WhiteWash,
            Self::WhiteWash => Self::Blur,
            Self::Blur => Self::DimBlack,
        }
    }

    /// Base mask RGB for flat effects; caller overrides alpha so only RGB
    /// matters. `Blur` has no fill — its value is unused, kept to total the match.
    #[must_use]
    pub const fn mask_color(self) -> Rgba {
        match self {
            Self::DimBlack | Self::Blur => Rgba::DEFAULT_MASK,
            Self::WhiteWash => Rgba::WHITE,
        }
    }

    /// `true` when the surround blurs the backdrop instead of filling a color.
    #[must_use]
    pub const fn is_blur(self) -> bool {
        matches!(self, Self::Blur)
    }

    /// Crossfade target for the renderer's `style_mix`: `DimBlack`/`Blur` = 0,
    /// `WhiteWash` = 255. `Blur` is 0 (no veil); flat⇄blur rides the master envelope.
    #[must_use]
    pub const fn mix_target(self) -> u8 {
        match self {
            Self::DimBlack | Self::Blur => 0,
            Self::WhiteWash => u8::MAX,
        }
    }

    /// Indicator color contrasting this effect's mask, so it never vanishes.
    #[must_use]
    pub const fn indicator_color(self) -> Rgba {
        match self {
            Self::DimBlack | Self::Blur => Rgba::WHITE,
            Self::WhiteWash => Rgba::BLACK,
        }
    }

    /// Short HUD label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::DimBlack => "Dim",
            Self::WhiteWash => "White",
            Self::Blur => "Blur",
        }
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    #[test]
    fn cycle_visits_each_state_once_before_returning() {
        let e0 = SurroundEffect::DimBlack;
        let e1 = e0.cycle();
        let e2 = e1.cycle();
        let e3 = e2.cycle();
        assert_eq!(e1, SurroundEffect::WhiteWash);
        assert_eq!(e2, SurroundEffect::Blur);
        assert_eq!(e3, SurroundEffect::DimBlack);
    }

    #[test]
    fn default_is_dim_black() {
        assert_eq!(SurroundEffect::default(), SurroundEffect::DimBlack);
    }

    #[test]
    fn dim_black_masks_with_black_rgb_and_white_indicator() {
        let e = SurroundEffect::DimBlack;
        let mask = e.mask_color();
        assert_eq!((mask.r, mask.g, mask.b), (0, 0, 0));
        let ind = e.indicator_color();
        assert_eq!((ind.r, ind.g, ind.b), (0xFF, 0xFF, 0xFF));
    }

    #[test]
    fn white_wash_masks_with_white_rgb_and_dark_indicator() {
        let e = SurroundEffect::WhiteWash;
        let mask = e.mask_color();
        assert_eq!((mask.r, mask.g, mask.b), (0xFF, 0xFF, 0xFF));
        let ind = e.indicator_color();
        assert_eq!((ind.r, ind.g, ind.b), (0, 0, 0));
    }

    #[test]
    fn blur_is_blur_with_white_indicator() {
        let e = SurroundEffect::Blur;
        assert!(e.is_blur());
        assert!(!SurroundEffect::DimBlack.is_blur());
        let ind = e.indicator_color();
        assert_eq!((ind.r, ind.g, ind.b), (0xFF, 0xFF, 0xFF));
    }

    /// Pin the `mix_target` mapping: `DimBlack` = 0 / `WhiteWash` = 255 /
    /// `Blur` = 0 (no color veil under blur).
    #[test]
    fn mix_target_mapping_is_pinned() {
        assert_eq!(SurroundEffect::DimBlack.mix_target(), 0);
        assert_eq!(SurroundEffect::WhiteWash.mix_target(), 255);
        assert_eq!(SurroundEffect::Blur.mix_target(), 0);
    }

    #[test]
    fn labels_are_distinct() {
        let labels = [
            SurroundEffect::DimBlack.label(),
            SurroundEffect::WhiteWash.label(),
            SurroundEffect::Blur.label(),
        ];
        assert_eq!(labels.len(), 3);
        assert_ne!(labels[0], labels[1]);
        assert_ne!(labels[1], labels[2]);
        assert_ne!(labels[0], labels[2]);
    }
}
