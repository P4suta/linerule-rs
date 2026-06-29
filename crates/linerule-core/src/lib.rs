//! linerule-core: pure logic layer (ADTs, reducer, render, parser, FSM).
//!
//! Nondeterminism (time, randomness, I/O) is passed in by the caller.
//! Key types are re-exported here for short paths (`linerule_core::Rgba`);
//! internal code uses long paths. Depends on no other linerule-rs crate.

#![forbid(unsafe_code)]

pub mod anim;
pub mod color;
pub mod config;
pub mod diagnostics;
pub mod geometry;
pub mod input;
pub mod render;
pub mod state;

pub use anim::{Lerp, Transition};
pub use color::{BlurAmount, DimLevel, Opacity, Rgba, Thickness};
pub use config::{
    AnimConfig, HudColors, HudConfig, HudFonts, HudGeometry, HudPadding, InputConfig,
    OverlayConfig, RenderConfig, RepeatConfig, TapStepConfig, UserConfig,
};
pub use diagnostics::{
    CoreError, DeviceLostOutcome, ErrorClass, LineruleError, Severity, is_device_lost_hresult,
    record_device_lost_failure,
};

/// Canonical `Result` alias; defaults to [`LineruleError`].
pub type Result<T, E = LineruleError> = core::result::Result<T, E>;
pub use geometry::{CoordSpace, Logical, Physical, Point, ScreenRect};
pub use input::{ChordError, ChordSpec, Direction, HotkeyMap, KeyCode, Letter, Modifiers};
pub use render::{
    Brush, Geometry, HudFontKey, HudFrame, HudNotification, HudRow, HudRule, HudTelemetry, HudTier,
    Layer, NotificationClass, OverlayFrame, OverlaySample, frame, hud_frame,
};
pub use state::{ActiveMode, Mode, OverlayAction, RejectReason, State, StateDelta, SurroundEffect};
