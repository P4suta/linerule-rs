//! 8-bit per channel sRGB color with straight alpha.

use serde::{Deserialize, Serialize};

/// sRGB color with straight (non-premultiplied) alpha. All channels are 8-bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgba {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel (straight, not premultiplied).
    pub a: u8,
}

impl Rgba {
    /// Default overlay mask: opaque-black at ~80% alpha. Straight alpha; the
    /// platform layer converts to premultiplied at the GPU boundary.
    pub const DEFAULT_MASK: Self = Self::new(0x00, 0x00, 0x00, 0xCC);

    /// Fully transparent (all channels zero).
    pub const TRANSPARENT: Self = Self::new(0, 0, 0, 0);
    /// Opaque black.
    pub const BLACK: Self = Self::new(0, 0, 0, 0xFF);
    /// Opaque white.
    pub const WHITE: Self = Self::new(0xFF, 0xFF, 0xFF, 0xFF);

    /// Construct an `Rgba` from raw channel bytes.
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Returns a copy with the alpha channel replaced.
    #[must_use]
    pub const fn with_alpha(self, alpha: u8) -> Self {
        Self { a: alpha, ..self }
    }
}

impl Default for Rgba {
    fn default() -> Self {
        Self::TRANSPARENT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_mask_is_translucent_black() {
        let mask = Rgba::DEFAULT_MASK;
        assert_eq!(mask.r, 0);
        assert_eq!(mask.g, 0);
        assert_eq!(mask.b, 0);
        assert_eq!(mask.a, 0xCC);
    }

    #[test]
    fn with_alpha_replaces_only_alpha() {
        let c = Rgba::new(0x12, 0x34, 0x56, 0x78);
        let d = c.with_alpha(0xFF);
        assert_eq!(d, Rgba::new(0x12, 0x34, 0x56, 0xFF));
        // Original is unchanged (Copy).
        assert_eq!(c.a, 0x78);
    }
}
