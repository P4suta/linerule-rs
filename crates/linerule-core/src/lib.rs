#![forbid(unsafe_code)]

//! Pure-logic core for linerule.
//!
//! This crate is IO-free and platform-free. It owns the type vocabulary the
//! rest of the workspace builds on: validating newtypes, phantom-typed
//! coordinate spaces, the [`Mode`] state machine, the [`render`] pure
//! function, and the [`reduce`] state-transition function.
//!
//! Higher layers (`linerule-config`, `linerule-platform`, the `linerule`
//! binary) consume these types but never the other way around. Tests in
//! this crate must remain platform-independent.

use core::marker::PhantomData;

use smallvec::SmallVec;
use thiserror::Error;

use serde::{Deserialize, Serialize};

// ===========================================================================
// Errors
// ===========================================================================

/// Domain validation errors raised by newtype constructors.
///
/// Each variant pinpoints the invariant that was violated and the offending
/// value, so diagnostics can render an actionable message at the IO boundary.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum CoreError {
    /// Opacity must be in `1..=255`.
    #[error("opacity must be in 1..=255 (got {0})")]
    Opacity(u16),

    /// Thickness must be in `1..=512` logical pixels.
    #[error("thickness must be in 1..=512 logical px (got {0})")]
    Thickness(u32),
}

// ===========================================================================
// Validating newtypes
// ===========================================================================

/// Alpha-channel opacity, validated to lie in `1..=255`.
///
/// `0` is excluded because a fully transparent overlay is the [`Mode::Off`]
/// case and should be expressed structurally, not via a degenerate opacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
#[non_exhaustive]
pub struct Opacity(u8);

impl Opacity {
    /// A reasonable default (`#aa`, ~67%).
    pub const DEFAULT: Self = Self(0xaa);

    /// Construct an [`Opacity`] from a raw `u8`.
    ///
    /// # Errors
    /// Returns [`CoreError::Opacity`] if `value == 0`.
    pub const fn new(value: u8) -> Result<Self, CoreError> {
        if value == 0 {
            Err(CoreError::Opacity(value as u16))
        } else {
            Ok(Self(value))
        }
    }

    /// Raw alpha channel value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Opacity {
    type Error = CoreError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Opacity> for u8 {
    fn from(value: Opacity) -> Self {
        value.0
    }
}

/// Bar / slit thickness in logical pixels, validated to lie in `1..=512`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
#[non_exhaustive]
pub struct Thickness(u16);

impl Thickness {
    /// Default bar thickness (28 logical px ≈ a typical body line height).
    pub const DEFAULT: Self = Self(28);

    /// Maximum permitted thickness in logical pixels.
    pub const MAX_PX: u16 = 512;

    /// Construct a [`Thickness`] from a raw value.
    ///
    /// # Errors
    /// Returns [`CoreError::Thickness`] if `value` is `0` or above
    /// [`Self::MAX_PX`].
    pub const fn new(value: u16) -> Result<Self, CoreError> {
        if value == 0 || value > Self::MAX_PX {
            Err(CoreError::Thickness(value as u32))
        } else {
            Ok(Self(value))
        }
    }

    /// Raw pixel count.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl TryFrom<u16> for Thickness {
    type Error = CoreError;
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Thickness> for u16 {
    fn from(value: Thickness) -> Self {
        value.0
    }
}

/// Mask dim level (0 = no dim, 255 = fully opaque dim).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[non_exhaustive]
pub struct DimLevel(pub u8);

impl DimLevel {
    /// Default mask darkness (`#cc`, ~80%).
    pub const DEFAULT: Self = Self(0xcc);
}

/// Sized 8-bit RGBA color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Rgba {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel.
    pub a: u8,
}

impl Rgba {
    /// Default bar color: warm yellow with [`Opacity::DEFAULT`] alpha.
    pub const DEFAULT_BAR: Self = Self {
        r: 0xff,
        g: 0xeb,
        b: 0x3b,
        a: 0xaa,
    };

    /// Default mask colour. Deliberately a *near*-black (`(8, 8, 8)`)
    /// rather than pure `(0, 0, 0)` because the Windows v0.1 platform
    /// uses `LWA_COLORKEY` with pure black as the transparency
    /// sentinel — see `linerule_platform::windows::COLORKEY_TRANSPARENT`
    /// and ADR-0009. A pure-black mask region would be silently
    /// colour-keyed away into a transparent slit. Visually `(8, 8, 8)`
    /// is indistinguishable from `(0, 0, 0)`.
    pub const DEFAULT_MASK: Self = Self {
        r: 8,
        g: 8,
        b: 8,
        a: 0xcc,
    };

    /// Compose [`Rgba`] from individual channels.
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

// ===========================================================================
// Phantom-typed coordinate spaces
// ===========================================================================

/// Marker for logical pixel coordinates (DPI-independent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Logical;

/// Marker for physical pixel coordinates (raw, scale-aware).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Physical;

/// A 2-D point in coordinate space `S`.
///
/// `S` is a phantom marker — usually [`Logical`] or [`Physical`] — that
/// prevents accidentally mixing the two systems at the type level.
#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(bound(serialize = "", deserialize = ""))]
pub struct Point<S> {
    /// X coordinate.
    pub x: i32,
    /// Y coordinate.
    pub y: i32,
    #[serde(skip, default)]
    _space: PhantomData<S>,
}

impl<S> Clone for Point<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S> Copy for Point<S> {}

impl<S> Point<S> {
    /// Construct a [`Point`] in space `S`.
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            _space: PhantomData,
        }
    }
}

/// An axis-aligned rectangle in coordinate space `S`, expressed as origin +
/// non-negative width/height.
#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(bound(serialize = "", deserialize = ""))]
pub struct ScreenRect<S> {
    /// Top-left corner.
    pub origin: Point<S>,
    /// Width in pixels of space `S`.
    pub width: u32,
    /// Height in pixels of space `S`.
    pub height: u32,
}

impl<S> Clone for ScreenRect<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S> Copy for ScreenRect<S> {}

impl<S> ScreenRect<S> {
    /// Construct a [`ScreenRect`] in space `S`.
    #[must_use]
    pub const fn new(origin: Point<S>, width: u32, height: u32) -> Self {
        Self {
            origin,
            width,
            height,
        }
    }

    /// Returns whether `point` lies inside this rectangle (half-open).
    #[must_use]
    pub fn contains(&self, point: Point<S>) -> bool {
        let right = self.origin.x.saturating_add_unsigned(self.width);
        let bottom = self.origin.y.saturating_add_unsigned(self.height);
        point.x >= self.origin.x && point.x < right && point.y >= self.origin.y && point.y < bottom
    }

    /// Returns whether `inner` is fully contained within `self`.
    #[must_use]
    pub fn contains_rect(&self, inner: &Self) -> bool {
        let self_right = self.origin.x.saturating_add_unsigned(self.width);
        let self_bottom = self.origin.y.saturating_add_unsigned(self.height);
        let inner_right = inner.origin.x.saturating_add_unsigned(inner.width);
        let inner_bottom = inner.origin.y.saturating_add_unsigned(inner.height);
        inner.origin.x >= self.origin.x
            && inner.origin.y >= self.origin.y
            && inner_right <= self_right
            && inner_bottom <= self_bottom
    }
}

// ===========================================================================
// Mode — runtime enum (cf. ADR-0002 for the type-state vs runtime trade-off)
// ===========================================================================

/// The four user-visible overlay modes.
///
/// Cycled by [`cycle`] in the order `Off → Bar → Mask → Vertical → Off`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Mode {
    /// Overlay hidden.
    #[default]
    Off,
    /// Single horizontal bar at the cursor's Y.
    Bar,
    /// Whole screen masked except a horizontal slit at the cursor's Y.
    Mask,
    /// Single vertical bar at the cursor's X — for 縦書き / 青空文庫 reading.
    Vertical,
}

/// Advance to the next mode in the canonical cycle.
///
/// `Off → Bar → Mask → Vertical → Off`. The cycle has period 4; verified by
/// property test `cycle⁴ ≡ id` in `tests/property_cycle.rs`.
#[must_use]
pub const fn cycle(prev: Mode) -> Mode {
    match prev {
        Mode::Off => Mode::Bar,
        Mode::Bar => Mode::Mask,
        Mode::Mask => Mode::Vertical,
        Mode::Vertical => Mode::Off,
    }
}

// ===========================================================================
// Visual configuration consumed by render() (subset of linerule-config::Config)
// ===========================================================================

/// Render-time visual configuration.
///
/// `linerule-config` wraps this in a fuller user-facing TOML schema that
/// adds hotkey bindings and other IO-bound concerns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct OverlayConfig {
    /// Bar fill color (also used for the Vertical mode bar).
    #[serde(default = "OverlayConfig::default_bar_color")]
    pub bar_color: Rgba,
    /// Mask fill color (only the alpha channel is meaningfully used by render).
    #[serde(default = "OverlayConfig::default_mask_color")]
    pub mask_color: Rgba,
    /// Bar thickness (also slit thickness in Mask mode).
    #[serde(default = "OverlayConfig::default_thickness")]
    pub thickness: Thickness,
    /// Bar opacity override; the alpha in `bar_color` is the source of truth,
    /// `opacity` is exposed for hot-key adjustments.
    #[serde(default = "OverlayConfig::default_opacity")]
    pub opacity: Opacity,
}

impl OverlayConfig {
    fn default_bar_color() -> Rgba {
        Rgba::DEFAULT_BAR
    }
    fn default_mask_color() -> Rgba {
        Rgba::DEFAULT_MASK
    }
    fn default_thickness() -> Thickness {
        Thickness::DEFAULT
    }
    fn default_opacity() -> Opacity {
        Opacity::DEFAULT
    }
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            bar_color: Rgba::DEFAULT_BAR,
            mask_color: Rgba::DEFAULT_MASK,
            thickness: Thickness::DEFAULT,
            opacity: Opacity::DEFAULT,
        }
    }
}

// ===========================================================================
// Render output
// ===========================================================================

// ===========================================================================
// Render output — categorical decomposition.
//
// A frame is a sequence of `Layer`s; each layer is the product
// `Geometry × Brush`. Both `Geometry` and `Brush` are sealed-by-default
// (`#[non_exhaustive]`) sum types so v0.2 extensions (paths, gradients,
// text glyphs) land additively without changing the existing variants
// or breaking pattern matches in downstream consumers.
//
// The vocabulary intentionally mirrors the one used by the platform-side
// rendering library (`peniko::Brush`, `peniko::Color`, `vello::Scene`)
// so the translation at the FFI boundary is a structural rename, not
// a re-encoding.
// ===========================================================================

/// Geometric primitive that a [`Layer`] can fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Geometry {
    /// Axis-aligned rectangle in logical pixel coordinates.
    Rect(ScreenRect<Logical>),
    // Future variants land here without breaking existing match arms:
    //   Path(...), Glyph(...), RoundedRect(...).
}

impl Geometry {
    /// Bounding rectangle of this geometry in logical pixels.
    #[must_use]
    pub const fn bounds(&self) -> ScreenRect<Logical> {
        match *self {
            Self::Rect(r) => r,
        }
    }
}

/// Paint pattern that fills a [`Geometry`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Brush {
    /// Single solid color — alpha is the layer's effective opacity.
    Solid(Rgba),
    // Future variants land here without breaking existing match arms:
    //   LinearGradient { stops, axis },
    //   RadialGradient { center, radius, stops }.
}

impl Brush {
    /// Effective alpha channel of the brush at the given sample.
    /// For [`Brush::Solid`] the sample is irrelevant.
    #[must_use]
    pub const fn alpha(self) -> u8 {
        match self {
            Self::Solid(c) => c.a,
        }
    }
}

/// One drawable atom: geometry × brush. Layers are blitted in declaration
/// order (first → bottom of the z-stack).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Layer {
    /// What shape to fill.
    pub geometry: Geometry,
    /// How to paint it.
    pub brush: Brush,
}

impl Layer {
    /// Construct a [`Layer`].
    #[must_use]
    pub const fn new(geometry: Geometry, brush: Brush) -> Self {
        Self { geometry, brush }
    }

    /// Solid-filled rectangle convenience constructor — the only kind of
    /// layer the v0.1 render path emits.
    #[must_use]
    pub const fn solid_rect(bounds: ScreenRect<Logical>, fill: Rgba) -> Self {
        Self::new(Geometry::Rect(bounds), Brush::Solid(fill))
    }
}

/// Output of a single [`render`] call: zero to four [`Layer`]s.
///
/// `Off` produces 0 layers, `Bar` and `Vertical` produce 1, `Mask`
/// produces 2 (the regions above and below the slit).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OverlayFrame {
    /// Layers in z-order (first is drawn first → bottom of the stack).
    pub layers: SmallVec<[Layer; 4]>,
}

impl OverlayFrame {
    /// An empty frame — used by [`Mode::Off`].
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            layers: SmallVec::new_const(),
        }
    }
}

impl Default for OverlayFrame {
    fn default() -> Self {
        Self::empty()
    }
}

// ===========================================================================
// Pure render function — implementation lands in task #8
// ===========================================================================

/// Compute the rectangles to draw for the given `(mode, cursor, monitor, cfg)`.
///
/// Pure: no IO, no side effects, deterministic. Each call returns a fresh
/// [`OverlayFrame`] of zero (`Off`), one (`Bar` / `Vertical`), or two
/// (`Mask`) filled rectangles, all clipped to `monitor` so the platform
/// layer can blit them without further bounds checking.
#[must_use]
pub fn render(
    mode: Mode,
    cursor: Point<Logical>,
    monitor: ScreenRect<Logical>,
    cfg: &OverlayConfig,
) -> OverlayFrame {
    match mode {
        Mode::Off => OverlayFrame::empty(),
        Mode::Bar => render_bar(cursor, monitor, cfg),
        Mode::Mask => render_mask(cursor, monitor, cfg),
        Mode::Vertical => render_vertical(cursor, monitor, cfg),
    }
}

/// Compose the bar fill colour, replacing the alpha with the user's
/// live opacity slider. The colour stored in `cfg.bar_color` is the
/// "intent"; `cfg.opacity` is the runtime override surfaced via
/// hotkey, so the runtime value wins at render time.
fn fill_with_opacity(base: Rgba, opacity: Opacity) -> Rgba {
    Rgba {
        a: opacity.get(),
        ..base
    }
}

fn render_bar(
    cursor: Point<Logical>,
    monitor: ScreenRect<Logical>,
    cfg: &OverlayConfig,
) -> OverlayFrame {
    let thick = u32::from(cfg.thickness.get());
    let half_t = i32::try_from(thick / 2).unwrap_or(0);
    let mon_top = monitor.origin.y;
    let mon_bot = monitor.origin.y.saturating_add_unsigned(monitor.height);

    // Centre the bar on the cursor Y, then clip into the monitor.
    let raw_top = cursor.y.saturating_sub(half_t);
    let top = raw_top.clamp(mon_top, mon_bot);
    let bot = raw_top
        .saturating_add_unsigned(thick)
        .clamp(mon_top, mon_bot);
    let height = u32::try_from(bot - top).unwrap_or(0);

    let bounds = ScreenRect::new(
        Point::<Logical>::new(monitor.origin.x, top),
        monitor.width,
        height,
    );

    let mut frame = OverlayFrame::empty();
    frame.layers.push(Layer::solid_rect(
        bounds,
        fill_with_opacity(cfg.bar_color, cfg.opacity),
    ));
    frame
}

fn render_vertical(
    cursor: Point<Logical>,
    monitor: ScreenRect<Logical>,
    cfg: &OverlayConfig,
) -> OverlayFrame {
    let thick = u32::from(cfg.thickness.get());
    let half_t = i32::try_from(thick / 2).unwrap_or(0);
    let mon_l = monitor.origin.x;
    let mon_r = monitor.origin.x.saturating_add_unsigned(monitor.width);

    let raw_left = cursor.x.saturating_sub(half_t);
    let left = raw_left.clamp(mon_l, mon_r);
    let right = raw_left.saturating_add_unsigned(thick).clamp(mon_l, mon_r);
    let width = u32::try_from(right - left).unwrap_or(0);

    let bounds = ScreenRect::new(
        Point::<Logical>::new(left, monitor.origin.y),
        width,
        monitor.height,
    );

    let mut frame = OverlayFrame::empty();
    frame.layers.push(Layer::solid_rect(
        bounds,
        fill_with_opacity(cfg.bar_color, cfg.opacity),
    ));
    frame
}

fn render_mask(
    cursor: Point<Logical>,
    monitor: ScreenRect<Logical>,
    cfg: &OverlayConfig,
) -> OverlayFrame {
    let thick = u32::from(cfg.thickness.get());
    let half_t = i32::try_from(thick / 2).unwrap_or(0);
    let mon_top = monitor.origin.y;
    let mon_bot = monitor.origin.y.saturating_add_unsigned(monitor.height);

    // Slit covers [slit_top, slit_bot); two rects clip to [mon_top, slit_top)
    // and [slit_bot, mon_bot).
    let raw_top = cursor.y.saturating_sub(half_t);
    let slit_top = raw_top.clamp(mon_top, mon_bot);
    let slit_bot = raw_top
        .saturating_add_unsigned(thick)
        .clamp(mon_top, mon_bot);

    let top_height = u32::try_from(slit_top - mon_top).unwrap_or(0);
    let bot_height = u32::try_from(mon_bot - slit_bot).unwrap_or(0);

    let mask_color = cfg.mask_color;
    let top_bounds = ScreenRect::new(
        Point::<Logical>::new(monitor.origin.x, mon_top),
        monitor.width,
        top_height,
    );
    let bot_bounds = ScreenRect::new(
        Point::<Logical>::new(monitor.origin.x, slit_bot),
        monitor.width,
        bot_height,
    );

    let mut frame = OverlayFrame::empty();
    frame.layers.push(Layer::solid_rect(top_bounds, mask_color));
    frame.layers.push(Layer::solid_rect(bot_bounds, mask_color));
    frame
}

// ===========================================================================
// State machine — landed as type stubs; reduce() lands in task #9
// ===========================================================================

/// Action vocabulary that the state machine accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Action {
    /// Advance to the next [`Mode`] in the cycle.
    CycleMode,
    /// Toggle visibility on / off (orthogonal to cycle).
    ToggleVisible,
    /// Increase bar thickness by `step` logical px (saturating at the bound).
    BumpThickness(i16),
    /// Increase opacity by `step` (saturating at the bound).
    BumpOpacity(i16),
    /// Emergency exit. The state machine treats this as a no-op
    /// ([`reduce`] returns a default [`StateDelta`]); the platform
    /// layer recognises it specially and tears the event loop down so
    /// the user can always recover from a stuck overlay even when
    /// every other hotkey is masked by another app.
    Quit,
}

/// Persistent runtime state of the overlay.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct State {
    /// Current overlay mode.
    pub mode: Mode,
    /// Whether the overlay window is currently visible.
    pub visible: bool,
    /// Live overlay configuration (mutated by [`Action::BumpThickness`] etc.).
    pub config: OverlayConfig,
}

/// Description of what changed in [`reduce`].
///
/// The platform layer reads this to reapply only the diff — don't
/// re-create the window for an opacity bump, etc. All `Option` fields
/// are `Some(new_value)` when the corresponding piece of state changed
/// and `None` when it did not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct StateDelta {
    /// New mode, if the action changed the mode.
    pub mode: Option<Mode>,
    /// New visibility, if the action toggled visibility.
    pub visible: Option<bool>,
    /// `true` if visual config (color/thickness/opacity) changed.
    pub config: bool,
}

/// Apply an [`Action`] to a [`State`], returning a [`StateDelta`]
/// describing what changed.
///
/// Pure deterministic transition. Saturating arithmetic on the
/// `Bump*` variants keeps `State.config` inside the validated range
/// of every newtype it contains — `Thickness` stays in `1..=512`,
/// `Opacity` stays in `1..=255`.
///
/// # Panics
/// This function panics on a logic bug only: the `Bump*` arms call into
/// [`Thickness::new`] / [`Opacity::new`] *after* clamping the step into
/// the validated range, so the inner `expect` should be unreachable.
/// A panic here indicates that the clamp invariant has drifted and is a
/// bug to be fixed in this crate, not a runtime concern for callers.
pub fn reduce(state: &mut State, action: Action) -> StateDelta {
    match action {
        Action::CycleMode => {
            let next = cycle(state.mode);
            state.mode = next;
            StateDelta {
                mode: Some(next),
                ..Default::default()
            }
        }
        Action::ToggleVisible => {
            state.visible = !state.visible;
            StateDelta {
                visible: Some(state.visible),
                ..Default::default()
            }
        }
        Action::BumpThickness(step) => {
            let current = i32::from(state.config.thickness.get());
            let next = (current + i32::from(step)).clamp(1, i32::from(Thickness::MAX_PX));
            let bumped = Thickness::new(
                u16::try_from(next).expect("clamp invariant guarantees 1..=MAX_PX (u16)"),
            )
            .expect("clamp invariant guarantees 1..=MAX_PX");
            state.config.thickness = bumped;
            StateDelta {
                config: true,
                ..Default::default()
            }
        }
        Action::BumpOpacity(step) => {
            let current = i32::from(state.config.opacity.get());
            let next = (current + i32::from(step)).clamp(1, 255);
            let bumped =
                Opacity::new(u8::try_from(next).expect("clamp invariant guarantees 1..=255 (u8)"))
                    .expect("clamp invariant guarantees 1..=255");
            state.config.opacity = bumped;
            StateDelta {
                config: true,
                ..Default::default()
            }
        }
        // Quit is a side-effect-only action handled at the platform
        // layer (event loop exit). The state machine sees a no-op so
        // downstream consumers can pattern-match on Action exhaustively
        // without special-casing Quit.
        Action::Quit => StateDelta::default(),
    }
}
