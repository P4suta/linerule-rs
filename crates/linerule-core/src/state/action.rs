//! User-issued commands (closed sum of `OverlayAction`).

use serde::{Deserialize, Serialize};

/// Closed sum of commands the reducer can apply. Bump variants carry a
/// signed delta; negative values shrink, positive values grow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayAction {
    /// Advance `Mode` through `Off → Horizontal → Vertical → Off`.
    CycleMode,
    /// Advance `SurroundEffect` through `DimBlack → WhiteWash → Blur →
    /// DimBlack`. Rejected while `Mode::Off` (like the bump actions) — the
    /// reducer reports `RejectReason::AdjustWhileOff`.
    CycleEffect,
    /// Toggle `Off ⇄ last_active` (quick on/off preserving the slit axis).
    ToggleOnOff,
    /// Add `delta` (signed) to `OverlayConfig::thickness`.
    BumpThickness(i32),
    /// Add `delta` (signed) to `OverlayConfig::opacity`. Under the `Blur`
    /// effect the delta retargets onto `OverlayConfig::blur` instead.
    BumpOpacity(i32),
    /// Toggle the HUD presentation between the chip and the full guide.
    /// Pure no-op at the reducer (view state lives in `TickWorld`, not
    /// `State`); the tick pipeline interprets it.
    ToggleHudDetail,
    /// Quit the application.
    Quit,
}
