//! User-issued commands (closed sum of `OverlayAction`).

use serde::{Deserialize, Serialize};

/// Closed sum of commands the reducer can apply. Bump variants carry a
/// signed delta; negative values shrink, positive values grow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayAction {
    /// Advance `Mode` through `Off → Horizontal → Vertical → Off`.
    CycleMode,
    /// Flip the `visible` flag.
    ToggleVisible,
    /// Add `delta` (signed) to `OverlayConfig::thickness`.
    BumpThickness(i32),
    /// Add `delta` (signed) to `OverlayConfig::opacity`.
    BumpOpacity(i32),
    /// Quit the application.
    Quit,
}
