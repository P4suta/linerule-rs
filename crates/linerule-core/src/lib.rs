#![forbid(unsafe_code)]

//! Pure-logic core for linerule.
//!
//! This crate is IO-free and platform-free. It owns the type vocabulary
//! the rest of the workspace builds on: validating newtypes,
//! phantom-typed coordinate spaces, the [`Mode`] direct-product ADT
//! (`Off | Active(Shape, Orientation)`), the [`Lifecycle`] sum type
//! (`Active(Mode) | Paused(Mode)`), the [`render`] pure function, and
//! the [`reduce`] state-transition function.
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

    /// Default mask colour: dark near-black at ~85% opacity
    /// (`alpha = 0xd9 = 217`). 85% is the sweet spot for the
    /// typoscope effect — the surrounding text is clearly suppressed
    /// without being so opaque that you lose the layout cues. The
    /// near-black `(8, 8, 8)` (NOT pure `(0, 0, 0)`) is a hold-over
    /// from the earlier `LWA_COLORKEY` approach (see ADR-0009 for the
    /// history); under the current `UpdateLayeredWindow` path pure
    /// black would also work, but the test suite pins the near-black
    /// invariant and there is no visual cost to keeping it.
    pub const DEFAULT_MASK: Self = Self {
        r: 8,
        g: 8,
        b: 8,
        a: 0xd9,
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
// Mode — direct-product decomposition (cf. ADR-0012)
//
// Mode = Off | Active(Shape, Orientation)
//
// `Shape` and `Orientation` are independent axes:
//   - `Shape       ∈ {Bar, Mask}`           — what to draw
//   - `Orientation ∈ {Horizontal, Vertical}` — along which axis
//
// The four `Active(_, _)` combinations cover the full reading-aid
// grid; `cycle` traverses the lattice in a fixed canonical order;
// `render` dispatches on the (shape, orientation) pair through a single
// axis-symmetric pipeline (project → slit → lift → paint) so adding a
// new orientation or shape variant is local.
// ===========================================================================

/// What to draw on the primary axis: a single solid line, or the
/// complement (everything *except* a slit) painted with a dim mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Shape {
    /// Single bar at the cursor's primary-axis projection.
    Bar,
    /// Typoscope: dim everything except a slit at the cursor's projection.
    Mask,
}

/// Reading orientation. Selects which screen axis is the *primary*
/// axis (the one that gets the slit / bar) and which is the secondary
/// (the one that gets stretched to monitor extent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Orientation {
    /// Left-to-right text. Primary axis = Y (vertical screen axis);
    /// the bar / slit spans the full monitor width.
    Horizontal,
    /// 縦書き / vertical Japanese. Primary axis = X; the bar / slit
    /// spans the full monitor height.
    Vertical,
}

/// The user-visible overlay mode.
///
/// Decomposed as a direct product on the `Active` arm so the four
/// drawn modes share a single render pipeline (cf. ADR-0012). `Off`
/// is structurally separate from `Lifecycle::Paused(_)`: `Off` is a
/// deliberate cycle position, while `Paused` is the orthogonal
/// "temporarily silenced" lifecycle state.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Mode {
    /// Overlay hidden — explicit cycle position meaning "show nothing".
    #[default]
    Off,
    /// Overlay drawing the (shape, orientation) pair.
    Active(Shape, Orientation),
}

impl Mode {
    /// Horizontal bar — `Active(Bar, Horizontal)`.
    pub const BAR: Self = Self::Active(Shape::Bar, Orientation::Horizontal);
    /// Horizontal mask (typoscope) — `Active(Mask, Horizontal)`.
    pub const MASK: Self = Self::Active(Shape::Mask, Orientation::Horizontal);
    /// Vertical bar — `Active(Bar, Vertical)`.
    pub const VERTICAL_BAR: Self = Self::Active(Shape::Bar, Orientation::Vertical);
    /// Vertical mask (縦書き typoscope) — `Active(Mask, Vertical)`.
    pub const VERTICAL_MASK: Self = Self::Active(Shape::Mask, Orientation::Vertical);
}

/// Advance to the next mode in the canonical cycle.
///
/// `Off → Bar → Mask → VerticalBar → VerticalMask → Off`. The cycle
/// has period 5; verified by property test `cycle⁵ ≡ id` in
/// `tests/property_cycle.rs`.
#[must_use]
pub const fn cycle(prev: Mode) -> Mode {
    match prev {
        Mode::Off => Mode::BAR,
        Mode::Active(Shape::Bar, Orientation::Horizontal) => Mode::MASK,
        Mode::Active(Shape::Mask, Orientation::Horizontal) => Mode::VERTICAL_BAR,
        Mode::Active(Shape::Bar, Orientation::Vertical) => Mode::VERTICAL_MASK,
        Mode::Active(Shape::Mask, Orientation::Vertical) => Mode::Off,
    }
}

// ===========================================================================
// Lifecycle — orthogonal Active vs Paused, both carrying a Mode
//
// Pause / resume preserves the inner mode for free; the platform layer
// short-circuits to `OverlayFrame::empty()` whenever the lifecycle is
// `Paused(_)`. The two-flag (`enabled: bool` + `mode: Mode`)
// representation that this replaces produced unreachable states; the
// sum type makes resume-with-prior-mode the only representable shape.
// ===========================================================================

/// Lifecycle of the overlay — actively rendering, or paused but
/// remembering the mode it was in for instant resume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Lifecycle {
    /// Overlay is rendering the inner [`Mode`].
    Active(Mode),
    /// Overlay is paused; the inner [`Mode`] is preserved so resume
    /// snaps back without losing the user's mode selection.
    Paused(Mode),
}

impl Default for Lifecycle {
    /// Default lifecycle is `Active(Mode::default())` = `Active(Off)`.
    /// The binary entry point promotes `Off` to a sensible reading-aid
    /// mode at first run; pure-core consumers see the structural
    /// "nothing decided yet" state.
    fn default() -> Self {
        Self::Active(Mode::default())
    }
}

impl Lifecycle {
    /// The inner mode, regardless of pause state.
    #[must_use]
    pub const fn mode(self) -> Mode {
        match self {
            Self::Active(m) | Self::Paused(m) => m,
        }
    }

    /// `true` iff the lifecycle is `Active(_)`.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active(_))
    }

    /// Flip pause / resume, preserving the inner mode.
    #[must_use]
    pub const fn toggled_pause(self) -> Self {
        match self {
            Self::Active(m) => Self::Paused(m),
            Self::Paused(m) => Self::Active(m),
        }
    }

    /// Replace the inner mode, preserving the active / paused state.
    #[must_use]
    pub const fn with_mode(self, m: Mode) -> Self {
        match self {
            Self::Active(_) => Self::Active(m),
            Self::Paused(_) => Self::Paused(m),
        }
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

/// Output of a single [`render`] call: zero, one, or two [`Layer`]s.
///
/// `Off` and `Lifecycle::Paused(_)` produce 0 layers, `Active(Bar, _)`
/// produces 1, `Active(Mask, _)` produces 2 (the regions on either
/// side of the slit). Inline capacity 2 covers every present render
/// output without heap allocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OverlayFrame {
    /// Layers in z-order (first is drawn first → bottom of the stack).
    pub layers: SmallVec<[Layer; 2]>,
}

impl OverlayFrame {
    /// An empty frame — used by [`Mode::Off`] and `Lifecycle::Paused(_)`.
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
// Pure render — axis-symmetric pipeline (project → slit → lift → paint)
// ===========================================================================

/// Compute the rectangles to draw for the given `(mode, cursor, monitor, cfg)`.
///
/// Pure: no IO, no side effects, deterministic. Returns a fresh
/// [`OverlayFrame`] of zero (`Off`), one (`Active(Bar, _)`), or two
/// (`Active(Mask, _)`) filled rectangles, all clipped to `monitor` so
/// the platform layer can blit them without further bounds checking.
#[must_use]
pub fn render(
    mode: Mode,
    cursor: Point<Logical>,
    monitor: ScreenRect<Logical>,
    cfg: &OverlayConfig,
) -> OverlayFrame {
    match mode {
        Mode::Off => OverlayFrame::empty(),
        Mode::Active(shape, orientation) => {
            render_active((shape, orientation), cursor, monitor, cfg)
        }
    }
}

/// 1-D half-open interval `[lo, hi)` on the primary axis selected by an
/// [`Orientation`]. Used internally to factor the four `Active(_, _)`
/// render arms through a single pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span1D {
    lo: i32,
    hi: i32,
}

impl Span1D {
    /// Length of the interval, saturating to 0 if `hi <= lo`.
    /// `abs_diff` is exact whenever `hi >= lo` (the post-clamp
    /// invariant) and gives a sign-loss-free conversion to `u32`.
    const fn length(self) -> u32 {
        if self.hi <= self.lo {
            0
        } else {
            self.hi.abs_diff(self.lo)
        }
    }
}

/// Project the cursor + monitor onto the primary axis selected by
/// `orientation`. Returns `(cursor projection, monitor span on the
/// primary axis)`.
const fn project(
    orientation: Orientation,
    cursor: Point<Logical>,
    monitor: ScreenRect<Logical>,
) -> (i32, Span1D) {
    match orientation {
        Orientation::Horizontal => {
            let span = Span1D {
                lo: monitor.origin.y,
                hi: monitor.origin.y.saturating_add_unsigned(monitor.height),
            };
            (cursor.y, span)
        }
        Orientation::Vertical => {
            let span = Span1D {
                lo: monitor.origin.x,
                hi: monitor.origin.x.saturating_add_unsigned(monitor.width),
            };
            (cursor.x, span)
        }
    }
}

/// Slit interval on the primary axis: cursor centred ± thickness/2,
/// clamped to `monitor_span`.
fn slit_span(cursor_proj: i32, monitor_span: Span1D, thickness: Thickness) -> Span1D {
    let thick = u32::from(thickness.get());
    let half_t = i32::try_from(thick / 2).unwrap_or(0);
    let raw_lo = cursor_proj.saturating_sub(half_t);
    let lo = raw_lo.clamp(monitor_span.lo, monitor_span.hi);
    let hi = raw_lo
        .saturating_add_unsigned(thick)
        .clamp(monitor_span.lo, monitor_span.hi);
    Span1D { lo, hi }
}

/// Lift a 1-D span on the primary axis (selected by `orientation`)
/// into a 2-D rect that spans the monitor's secondary axis.
fn lift(
    orientation: Orientation,
    span: Span1D,
    monitor: ScreenRect<Logical>,
) -> ScreenRect<Logical> {
    match orientation {
        Orientation::Horizontal => ScreenRect::new(
            Point::<Logical>::new(monitor.origin.x, span.lo),
            monitor.width,
            span.length(),
        ),
        Orientation::Vertical => ScreenRect::new(
            Point::<Logical>::new(span.lo, monitor.origin.y),
            span.length(),
            monitor.height,
        ),
    }
}

/// Compose the bar fill colour, replacing the alpha with the user's
/// live opacity slider. The colour stored in `cfg.bar_color` is the
/// "intent"; `cfg.opacity` is the runtime override surfaced via
/// hotkey, so the runtime value wins at render time.
const fn fill_with_opacity(base: Rgba, opacity: Opacity) -> Rgba {
    Rgba {
        a: opacity.get(),
        ..base
    }
}

fn render_active(
    axes: (Shape, Orientation),
    cursor: Point<Logical>,
    monitor: ScreenRect<Logical>,
    cfg: &OverlayConfig,
) -> OverlayFrame {
    let (shape, orientation) = axes;
    let (cursor_proj, mon_span) = project(orientation, cursor, monitor);
    let slit = slit_span(cursor_proj, mon_span, cfg.thickness);

    let mut frame = OverlayFrame::empty();
    match shape {
        Shape::Bar => {
            // 1 layer: paint the slit itself with `bar_color`.
            let bounds = lift(orientation, slit, monitor);
            let fill = fill_with_opacity(cfg.bar_color, cfg.opacity);
            frame.layers.push(Layer::solid_rect(bounds, fill));
        }
        Shape::Mask => {
            // 2 layers: paint the *complement* of the slit on the
            // primary axis with `mask_color`.
            let lo_complement = Span1D {
                lo: mon_span.lo,
                hi: slit.lo,
            };
            let hi_complement = Span1D {
                lo: slit.hi,
                hi: mon_span.hi,
            };
            for span in [lo_complement, hi_complement] {
                let bounds = lift(orientation, span, monitor);
                frame.layers.push(Layer::solid_rect(bounds, cfg.mask_color));
            }
        }
    }
    frame
}

// ===========================================================================
// State machine — Lifecycle ADT + Action sum + reduce
// ===========================================================================

/// Action vocabulary the state machine accepts.
///
/// `Quit` is *not* an [`Action`] — it is a control-plane signal handled
/// at the platform layer (event-loop exit). See [`HotkeyEffect`] for
/// the broader vocabulary that hotkey bindings are typed against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Action {
    /// Advance to the next [`Mode`] in the cycle.
    CycleMode,
    /// Pause / resume the overlay — temporarily disable all output
    /// without losing the current mode, position, or config. While
    /// paused, [`render`] is short-circuited to
    /// [`OverlayFrame::empty()`]; the bar / mask snaps back into place
    /// when the user toggles pause off again.
    TogglePause,
    /// Increase bar thickness by `step` logical px (saturating at the bound).
    BumpThickness(i16),
    /// Increase opacity by `step` (saturating at the bound).
    BumpOpacity(i16),
}

/// What a hotkey binding produces.
///
/// Either a state-machine [`Action`] (handled by [`reduce`]) or a
/// [`HotkeyEffect::Quit`] control signal (handled at the platform
/// layer as event-loop tear-down). The decomposition keeps `Action`
/// closed under [`reduce`] and the platform-side dispatch tagless on
/// the state-mutation arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HotkeyEffect {
    /// State-machine [`Action`].
    Apply(Action),
    /// Emergency exit; the platform layer tears down the event loop.
    /// Always present in default hotkey bindings so a stuck overlay
    /// can always be recovered.
    Quit,
}

/// Persistent runtime state of the overlay.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct State {
    /// Active vs paused, plus the inner mode in either case.
    pub lifecycle: Lifecycle,
    /// Live overlay configuration (mutated by [`Action::BumpThickness`] etc.).
    pub config: OverlayConfig,
}

/// Description of what changed in [`reduce`].
///
/// The platform layer reads this to reapply only the diff — don't
/// re-create the window for an opacity bump, etc. `lifecycle` is
/// `Some(new)` when the action transitioned the lifecycle (mode cycle
/// or pause toggle) and `None` when it did not. `config` is `true`
/// iff visual config (color/thickness/opacity) changed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct StateDelta {
    /// New lifecycle value, if the action changed it.
    pub lifecycle: Option<Lifecycle>,
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
            let next_mode = cycle(state.lifecycle.mode());
            let next_lc = state.lifecycle.with_mode(next_mode);
            state.lifecycle = next_lc;
            StateDelta {
                lifecycle: Some(next_lc),
                config: false,
            }
        }
        Action::TogglePause => {
            let next_lc = state.lifecycle.toggled_pause();
            state.lifecycle = next_lc;
            StateDelta {
                lifecycle: Some(next_lc),
                config: false,
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
                lifecycle: None,
                config: true,
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
                lifecycle: None,
                config: true,
            }
        }
    }
}
