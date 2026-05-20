//! Overlay state model, reducer, and the closed sum of commands.
//!
//! `State` is the snapshot the reducer mutates; submodules carry the
//! supporting types ([`Mode`], [`OverlayAction`], [`StateDelta`]) and the
//! pure reducer in [`reduce::apply`].

pub mod action;
pub mod delta;
pub mod mode;
pub mod reduce;

pub use action::OverlayAction;
pub use delta::StateDelta;
pub use mode::Mode;

use serde::{Deserialize, Serialize};

use crate::config::OverlayConfig;

/// User-visible overlay state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct State {
    /// Display mode (`Off` / `Horizontal` / `Vertical`).
    pub mode: Mode,
    /// Whether the overlay is currently rendered.
    pub visible: bool,
    /// Mask color / thickness / opacity tunables.
    pub config: OverlayConfig,
}

impl State {
    /// Default state: mode off, visible, with [`OverlayConfig::DEFAULT`].
    pub const DEFAULT: Self = Self {
        mode: Mode::Off,
        visible: true,
        config: OverlayConfig::DEFAULT,
    };
}

impl Default for State {
    fn default() -> Self {
        Self::DEFAULT
    }
}
