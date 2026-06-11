//! Surround effect: how the area *around* the slit is treated.
//!
//! The slit itself stays clear; the "surround" is the masked region above and
//! below (or beside) it. Variants are a flat color (`DimBlack`, `WhiteWash`) or
//! a backdrop blur (`Blur`). They form a runtime cycle that mirrors
//! [`crate::state::Mode`]; each variant supplies its own mask/tint color and a
//! contrasting indicator color so the corner indicator stays visible on any
//! background.
//!
//! Effect *parameters* (the exact colors) are compile-time constants here;
//! only the *selection* is runtime-mutable, exactly like `Mode`.

use serde::{Deserialize, Serialize};

use crate::color::Rgba;

/// Treatment applied to the region surrounding the slit. The cycle is
/// `DimBlack → WhiteWash → Blur → DimBlack`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurroundEffect {
    /// Darken the surround with a translucent black mask.
    #[default]
    DimBlack,
    /// Wash the surround with a translucent white mask — suited to bright
    /// environments / white-background documents.
    WhiteWash,
    /// Blur the screen content behind the surround — a pure backdrop blur with
    /// no color veil over it. Rendered as a true backdrop blur by the `WinRT`
    /// composition backend (the sole composition backend).
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

    /// Base mask color (RGB) for the flat effects. The caller overrides alpha
    /// with the current [`crate::color::Opacity`], so only the RGB channels carry
    /// meaning here. `Blur` has no fill color (it is a pure backdrop blur); its
    /// value here is unused and kept only to make the match total.
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

    /// Style-crossfade target for the renderer's `style_mix` channel:
    /// `DimBlack` = `0`, `WhiteWash` = `255`. `Blur` maps to `0`: it carries no
    /// color veil, so the surround does not consume `style_mix` while blurred
    /// (flat ⇄ blur switches ride the master envelope instead).
    #[must_use]
    pub const fn mix_target(self) -> u8 {
        match self {
            Self::DimBlack | Self::Blur => 0,
            Self::WhiteWash => u8::MAX,
        }
    }

    /// Indicator color that contrasts against this effect's mask. White on a
    /// dim (black) mask or blur; near-black on a white wash so the corner
    /// indicator never vanishes into the surround.
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

    /// Pin the `mix_target` mapping: DimBlack = 0 / WhiteWash = 255 / Blur = 0
    /// (no color veil under blur).
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
