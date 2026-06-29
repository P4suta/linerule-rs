//! Color types and perceptual brightness curves.
//!
//! [`perceptual`] maps a stored linear opacity to its on-screen alpha byte
//! (gamma-2.2, CIE L\*); [`rgba`] is 8-bit sRGB with straight alpha; [`units`]
//! holds bounded numeric newtypes.

pub mod perceptual;
pub mod rgba;
pub mod units;

pub use rgba::Rgba;
pub use units::{BlurAmount, DimLevel, Opacity, Thickness};
