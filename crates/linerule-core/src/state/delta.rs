//! Differential update produced by [`crate::state::reduce::apply`].
//!
//! Carrying a delta (instead of just the new state) lets the platform layer
//! decide cheaply whether anything visible changed. Every field is `Option`
//! for unchanged-vs-changed, with `config_changed` as a single bit because
//! the config payload is large.

use serde::{Deserialize, Serialize};

use crate::state::Mode;

/// Per-tick diff for [`crate::state::State`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StateDelta {
    /// `Some(_)` when `Mode` changed.
    pub mode: Option<Mode>,
    /// `Some(_)` when `visible` changed.
    pub visible: Option<bool>,
    /// `true` when `OverlayConfig` changed in any field.
    pub config_changed: bool,
}

impl StateDelta {
    /// Empty delta — no field changed.
    pub const NONE: Self = Self {
        mode: None,
        visible: None,
        config_changed: false,
    };

    /// `true` if any field changed.
    #[must_use]
    pub const fn is_any(self) -> bool {
        self.mode.is_some() || self.visible.is_some() || self.config_changed
    }
}
