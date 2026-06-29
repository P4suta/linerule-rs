//! User-issued commands (closed sum of `OverlayAction`).

use serde::{Deserialize, Serialize};

/// Commands the reducer can apply. Bump deltas are signed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayAction {
    /// Flip the slit axis. Rejected while `Mode::Off`.
    CycleMode,
    /// Advance `SurroundEffect`. Rejected while `Mode::Off` (`RejectReason::AdjustWhileOff`).
    CycleEffect,
    /// Toggle `Off ⇄ last_active`, preserving the slit axis.
    ToggleOnOff,
    /// Add signed `delta` to `OverlayConfig::thickness`.
    BumpThickness(i32),
    /// Add signed `delta` to `OverlayConfig::opacity`; under `Blur` retargets `OverlayConfig::blur`.
    BumpOpacity(i32),
    /// Toggle HUD chip/full guide. Reducer no-op; interpreted by the tick pipeline.
    ToggleHudDetail,
    /// Quit the application.
    Quit,
}
