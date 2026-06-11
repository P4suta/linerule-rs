//! linerule-core
//!
//! Pure logic layer: ADTs, reducer, render, parser, FSM. `#![forbid(unsafe_code)]`
//! bans `unsafe` outright; nondeterminism (time, randomness, I/O) is passed in
//! by the caller as arguments.
//!
//! ## Modules
//!
//! - [`anim`] — integer-endpoint timed transitions (`Transition<T>`) and easing
//! - [`color`] — `Rgba` / `Opacity` / `DimLevel` / `Thickness` / `BlurAmount` and perceptual curves
//! - [`config`] — `UserConfig` tree (`OverlayConfig` / `HudConfig` / ...)
//! - [`diagnostics`] — `LineruleError` / `Severity`
//! - [`geometry`] — coordinate-space-tagged `Point<S>` / `ScreenRect<S>`
//! - [`input`] — chord parser / hold FSM / tick pipeline / HUD fade / hotkey map
//! - [`render`] — `OverlayFrame` ADT and the pure `render::frame`
//! - [`state`] — `State` / `OverlayAction` / `StateDelta` and `state::reduce::apply`
//!
//! ## Short public paths
//!
//! Key types are re-exported here, so consumers write short paths like
//! `linerule_core::Rgba` / `linerule_core::frame(...)`. Internal code uses the
//! long paths (`linerule_core::color::rgba::Rgba`), leaving room to refactor.
//!
//! ## Dependency direction
//!
//! `linerule-app` → `linerule-platform-windows` → `linerule-core`. This crate
//! depends on no other linerule-rs crate.

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

/// Canonical `Result` alias for `linerule-core`.
///
/// Defaults to [`LineruleError`] so the whole crate's failure surface flows
/// through a single error type; override for narrow validators that return
/// [`CoreError`] etc.
pub type Result<T, E = LineruleError> = core::result::Result<T, E>;
pub use geometry::{CoordSpace, Logical, Physical, Point, ScreenRect};
pub use input::{ChordError, ChordSpec, Direction, HotkeyMap, KeyCode, Letter, Modifiers};
pub use render::{
    Brush, Geometry, HudFontKey, HudFrame, HudNotification, HudRow, HudRule, HudTelemetry, HudTier,
    Layer, NotificationClass, OverlayFrame, OverlaySample, frame, hud_frame,
};
pub use state::{ActiveMode, Mode, OverlayAction, RejectReason, State, StateDelta, SurroundEffect};
