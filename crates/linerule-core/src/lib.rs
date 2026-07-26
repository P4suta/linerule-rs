//! linerule-core: pure logic layer (ADTs, reducer, render, parser, FSM).
//!
//! Nondeterminism (time, randomness, I/O) is passed in by the caller.
//! Key types are re-exported here for short paths (`linerule_core::Rgba`);
//! internal code uses long paths. Depends on no other linerule-rs crate.

#![forbid(unsafe_code)]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![allow(
    clippy::redundant_pub_crate,
    reason = "crate-visible helpers live below private facade modules; pub(crate) documents their internal boundary"
)]

mod anim;
mod color;
mod config;
mod diagnostics;
mod geometry;
mod input;
mod preferences;
mod render;
mod state;

pub use anim::{Lerp, Transition};
pub use color::{BlurAmount, DimLevel, Opacity, Rgba, Thickness, smooth as perceptual_smooth};
pub use config::{
    AnimConfig, HudColors, HudConfig, HudFonts, HudGeometry, HudPadding, OverlayConfig,
    RenderConfig, TapStepConfig,
};
pub use diagnostics::{
    CoreError, DeviceLostOutcome, ErrorClass, LineruleError, Severity, is_device_lost_hresult,
    record_device_lost_failure,
};

/// Canonical `Result` alias; defaults to [`LineruleError`].
pub type Result<T, E = LineruleError> = core::result::Result<T, E>;
pub use geometry::{CoordSpace, Logical, Physical, Point, ScreenRect};
pub use input::{
    ActionBatch, ChordError, ChordSpec, Direction, KeyCode, Letter, MOD_ALT, MOD_CONTROL,
    MOD_SHIFT, MOD_WIN, Modifiers, TickEffect, TickEffects, TickInput, TickWorld,
    apply_envelope as apply_hud_envelope, chord_from_win32, chord_to_win32,
    compute_opacity as hud_distance_opacity, key_to_vk, parse as parse_chord, step as tick,
};
pub use preferences::{
    BindingError, BindingErrors, Command, Engine, HotkeyBindings, PREFERENCES_SCHEMA_VERSION,
    Preferences, PreferencesError, RulerPreferences,
};
pub use render::{
    Brush, Geometry, HudFontKey, HudFrame, HudNotification, HudRow, HudRule, HudTelemetry, HudTier,
    Layer, NotificationClass, OverlayFrame, OverlaySample, frame, hud_frame,
};
pub use state::{
    ActiveMode, Mode, OverlayAction, RejectReason, State, StateDelta, SurroundEffect,
    apply as reduce,
};
