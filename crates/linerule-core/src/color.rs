//! Color types and perceptual brightness curves.
//!
//! Submodules:
//! - [`rgba`]: 8-bit sRGB color with straight alpha.
//! - [`perceptual`]: gamma-2.2 and CIE L\* curves used when mapping a stored
//!   linear opacity to its on-screen alpha byte.
//! - [`units`]: bounded numeric newtypes ([`Opacity`], [`DimLevel`],
//!   [`Thickness`], [`BlurAmount`]).

pub mod perceptual;
pub mod rgba;
pub mod units;

pub use rgba::Rgba;
pub use units::{BlurAmount, DimLevel, Opacity, Thickness};
