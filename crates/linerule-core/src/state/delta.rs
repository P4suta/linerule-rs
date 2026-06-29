//! Differential update produced by [`crate::state::reduce::apply`].
//!
//! Lets the platform layer cheaply tell whether anything visible changed.
//! `config_changed` is one bit because the config payload is large.

use serde::{Deserialize, Serialize};

use crate::state::Mode;

/// Why the reducer refused an action (state untouched) but the user is owed feedback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectReason {
    /// Thickness / opacity / style adjustments require an active mode.
    AdjustWhileOff,
}

/// Per-tick diff for [`crate::state::State`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StateDelta {
    /// `Some(_)` when `Mode` changed.
    pub mode: Option<Mode>,
    /// `true` when `OverlayConfig` changed in any field.
    pub config_changed: bool,
    /// `Some(_)` when rejected. Never co-occurs with state changes, so it is
    /// excluded from [`StateDelta::is_any`].
    pub rejected: Option<RejectReason>,
}

impl StateDelta {
    /// Empty delta — no field changed.
    pub const NONE: Self = Self {
        mode: None,
        config_changed: false,
        rejected: None,
    };

    /// `true` if any state field changed. Rejections excluded (no state change).
    #[must_use]
    pub const fn is_any(self) -> bool {
        self.mode.is_some() || self.config_changed
    }
}
