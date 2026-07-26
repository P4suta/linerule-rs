//! Color types and perceptual brightness curves.
//!
//! [`perceptual`] maps a stored linear opacity to its on-screen alpha byte
//! (gamma-2.2, CIE L\*); [`rgba`] is 8-bit sRGB with straight alpha; [`units`]
//! holds bounded numeric newtypes.

pub(crate) mod perceptual;
mod rgba;
mod units;

pub use perceptual::smooth;
pub use rgba::Rgba;
pub use units::{BlurAmount, DimLevel, Opacity, Thickness};
