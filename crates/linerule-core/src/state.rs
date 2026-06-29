//! Overlay state model and reducer.

pub mod action;
pub mod delta;
pub mod mode;
pub mod reduce;
pub mod surround;

pub use action::OverlayAction;
pub use delta::{RejectReason, StateDelta};
pub use mode::{ActiveMode, Mode};
pub use surround::SurroundEffect;

use serde::{Deserialize, Serialize};

use crate::config::OverlayConfig;

/// User-visible overlay state.
///
/// No separate visibility flag: "not shown" is exactly `mode == Mode::Off`, so
/// a hidden-but-configured state can't accept hotkeys with invisible effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct State {
    /// Display mode (`Off` / `Horizontal` / `Vertical`).
    pub mode: Mode,
    /// Mode restored by `ToggleOnOff` when `mode == Off`.
    ///
    /// Invariant: when `mode` is active, `mode.active() == Some(last_active)`.
    /// Construct via [`State::with_mode`] to respect it.
    pub last_active: ActiveMode,
    /// Mask color / thickness / opacity tunables.
    pub config: OverlayConfig,
}

impl State {
    /// Default state: mode off, restore target horizontal, default config.
    pub const DEFAULT: Self = Self {
        mode: Mode::Off,
        last_active: ActiveMode::Horizontal,
        config: OverlayConfig::DEFAULT,
    };

    /// State with the given mode and an invariant-consistent `last_active`
    /// (default config). Used by `--initial-mode` and tests.
    #[must_use]
    pub const fn with_mode(mode: Mode) -> Self {
        let last_active = match mode {
            Mode::Off | Mode::Horizontal => ActiveMode::Horizontal,
            Mode::Vertical => ActiveMode::Vertical,
        };
        Self {
            mode,
            last_active,
            config: OverlayConfig::DEFAULT,
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self::DEFAULT
    }
}
