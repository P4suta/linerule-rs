//! Differential update produced by [`crate::state::reduce::apply`].
//!
//! Carrying a delta (instead of just the new state) lets the platform layer
//! decide cheaply whether anything visible changed. `mode` is `Option` for
//! unchanged-vs-changed, with `config_changed` as a single bit because the
//! config payload is large. `rejected` reports an action the reducer refused
//! — state did not change, but the user is owed feedback.

use serde::{Deserialize, Serialize};

use crate::state::Mode;

/// Why the reducer refused an action (state left untouched) and the user
/// should be told. Closed sum; new rejection causes get new variants.
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
    /// `Some(_)` when the action was rejected. Mutually exclusive with the
    /// other fields — a rejection never changes state, so it is deliberately
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

    /// `true` if any *state* field changed. Rejections are excluded: state
    /// did not change, only a notification is owed.
    #[must_use]
    pub const fn is_any(self) -> bool {
        self.mode.is_some() || self.config_changed
    }
}
