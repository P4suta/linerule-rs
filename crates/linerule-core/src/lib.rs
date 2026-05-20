//! linerule-core
//!
//! 純粋ロジック層: ADT、reducer、render、parser、FSM。`#![forbid(unsafe_code)]`
//! で `unsafe` を完全に排除し、非決定性 (時刻・乱数・I/O) は呼び出し側から引数で
//! 受け取る。
//!
//! ## 構成
//!
//! - [`color`] — `Rgba` / `Opacity` / `DimLevel` / `Thickness` と perceptual カーブ
//! - [`config`] — `UserConfig` ツリー (`OverlayConfig` / `HudConfig` / ...)
//! - [`diagnostics`] — `LineruleError` / `Severity`
//! - [`geometry`] — 座標空間タグ付き `Point<S>` / `ScreenRect<S>`
//! - [`input`] — chord parser / hold FSM / tick pipeline / HUD fade / hotkey map
//! - [`render`] — `OverlayFrame` ADT と純粋関数 `render::frame`
//! - [`state`] — `State` / `OverlayAction` / `StateDelta` と `state::reduce::apply`
//!
//! ## 短い public path
//!
//! 主要型は `lib.rs` で再エクスポートしているので、consumer は
//! `linerule_core::Rgba` / `linerule_core::frame(...)` のような短い path で
//! 書ける。internal 実装は `linerule_core::color::rgba::Rgba` などの長い
//! path で書き、リファクタの自由度を残す。
//!
//! ## 依存方向
//!
//! `linerule-app` → `linerule-platform-windows` → `linerule-core`。本クレートは
//! 他の linerule-rs クレートに依存しない。

#![forbid(unsafe_code)]

pub mod color;
pub mod config;
pub mod diagnostics;
pub mod geometry;
pub mod input;
pub mod render;
pub mod state;

pub use color::{DimLevel, Opacity, Rgba, Thickness};
pub use config::{
    HudColors, HudConfig, HudFonts, HudGeometry, HudPadding, InputConfig, OverlayConfig,
    RenderConfig, RepeatConfig, TapStepConfig, UserConfig,
};
pub use diagnostics::{CoreError, ErrorClass, LineruleError, Severity};

/// Canonical `Result` alias for `linerule-core`.
///
/// Defaults to [`LineruleError`] so the whole crate's failure surface flows
/// through a single error type; override for narrow validators that return
/// [`CoreError`] etc.
pub type Result<T, E = LineruleError> = core::result::Result<T, E>;
pub use geometry::{CoordSpace, Logical, Physical, Point, ScreenRect};
pub use input::{ChordError, ChordSpec, Direction, HotkeyMap, KeyCode, Letter, Modifiers};
pub use render::{
    Brush, Geometry, HudFontKey, HudFrame, HudNotification, HudRow, Layer, NotificationClass,
    OverlayFrame, frame, hud_frame,
};
pub use state::{Mode, OverlayAction, State, StateDelta};
