//! Pure tick pipeline: tick inputs -> next [`TickWorld`] + [`TickEffect`] list.

use std::time::Duration;

use serde::Serialize;

use crate::{
    anim::Transition,
    config::{AnimConfig, OverlayConfig},
    geometry::{Logical, Point},
    render::{HudTier, OverlaySample},
    state::{Mode, OverlayAction, RejectReason, State, apply},
};

const MAX_ACTIONS_PER_TICK: usize = 16;
// State-change and rejection telemetry is coalesced to one effect of each kind
// per tick. Quit, draw/clear, HUD opacity, and HUD refresh add at most four.
const MAX_EFFECTS_PER_TICK: usize = 6;

/// Fixed-capacity actions consumed by one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActionBatch {
    actions: [OverlayAction; MAX_ACTIONS_PER_TICK],
    len: usize,
}

impl ActionBatch {
    /// Empty action batch.
    pub const EMPTY: Self = Self {
        actions: [OverlayAction::Quit; MAX_ACTIONS_PER_TICK],
        len: 0,
    };

    /// Maximum number of actions processed by one tick.
    pub const CAPACITY: usize = MAX_ACTIONS_PER_TICK;

    /// Append one action, returning it unchanged when the batch is full.
    ///
    /// # Errors
    /// Returns `action` when [`Self::CAPACITY`] is already reached.
    pub fn try_push(&mut self, action: OverlayAction) -> core::result::Result<(), OverlayAction> {
        let Some(slot) = self.actions.get_mut(self.len) else {
            return Err(action);
        };
        *slot = action;
        self.len += 1;
        Ok(())
    }

    /// Build a batch without truncating an oversized iterator.
    ///
    /// # Errors
    /// Returns the first action that does not fit.
    pub fn try_from_actions(
        actions: impl IntoIterator<Item = OverlayAction>,
    ) -> core::result::Result<Self, OverlayAction> {
        let mut batch = Self::EMPTY;
        for action in actions {
            batch.try_push(action)?;
        }
        Ok(batch)
    }

    /// Actions in arrival order.
    #[must_use]
    pub fn as_slice(&self) -> &[OverlayAction] {
        &self.actions[..self.len]
    }

    /// Number of queued actions.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether no action is queued.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Whether another action must wait for a later tick.
    #[must_use]
    pub const fn is_full(&self) -> bool {
        self.len == Self::CAPACITY
    }

    /// Iterate in arrival order.
    pub fn iter(&self) -> core::slice::Iter<'_, OverlayAction> {
        self.as_slice().iter()
    }
}

impl Default for ActionBatch {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl<'a> IntoIterator for &'a ActionBatch {
    type Item = &'a OverlayAction;
    type IntoIter = core::slice::Iter<'a, OverlayAction>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Per-tick input from the platform.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TickInput {
    /// Current timestamp (millisecond).
    pub now_ms: i64,
    /// Latest cursor sample from the OS (`None` if not yet known).
    pub polled_cursor: Option<Point<Logical>>,
    /// Hotkey actions drained from the platform channel this tick.
    pub drained_hotkeys: ActionBatch,
}

/// Overlay visual-glide transition channels. Integer endpoints keep the world
/// `Eq + Hash` ([`crate::anim`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct OverlayAnim {
    /// Show/hide + mode-switch envelope (`0` = gone, `255` = fully shown).
    pub master: Transition<u8>,
    /// Slit thickness in logical px.
    pub thickness: Transition<u16>,
    /// Mask opacity byte (pre-perceptual).
    pub mask_alpha: Transition<u8>,
    /// Style crossfade (`0` = `DimBlack`, `255` = `WhiteWash`).
    pub style_mix: Transition<u8>,
}

impl OverlayAnim {
    /// Every channel settled at the given state's values (no motion).
    #[must_use]
    pub const fn settled_for(state: State) -> Self {
        let master = match state.mode {
            Mode::Off => 0,
            Mode::Horizontal | Mode::Vertical => u8::MAX,
        };
        Self {
            master: Transition::settled(master),
            thickness: Transition::settled(state.config.thickness.get()),
            mask_alpha: Transition::settled(state.config.opacity.get()),
            style_mix: Transition::settled(state.config.effect.mix_target()),
        }
    }

    /// Sample bundle at `now_ms`, as carried by `DrawOverlay`.
    #[must_use]
    pub fn sample(self, now_ms: i64) -> OverlaySample {
        OverlaySample {
            master: self.master.sample(now_ms),
            thickness_px: self.thickness.sample(now_ms),
            mask_alpha: self.mask_alpha.sample(now_ms),
            style_mix: self.style_mix.sample(now_ms),
        }
    }
}

/// HUD presentation view-state. In `TickWorld`, not `State`: irrelevant to the
/// reducer/`render::frame` and time-coupled (`boot_at_ms`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct HudView {
    /// Current presentation tier.
    pub tier: HudTier,
    /// Set once `ToggleHudDetail` is pressed; suppresses startup auto-demotion.
    pub user_touched: bool,
    /// First-tick time (startup full-display origin); `i64::MIN` = unset.
    pub boot_at_ms: i64,
}

impl HudView {
    /// Boot state: full display (teaching period), boot time unset.
    pub const INITIAL: Self = Self {
        tier: HudTier::Full,
        user_touched: false,
        boot_at_ms: i64::MIN,
    };
}

/// Tick pipeline's persistent state.
//
// `Deserialize` omitted: `Point<S>` carries `PhantomData<fn() -> S>`, which
// blocks the derive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct TickWorld {
    /// Last applied overlay state.
    pub state: State,
    /// Previous tick's cursor sample.
    pub last_cursor: Option<Point<Logical>>,
    /// Monotonically incrementing frame counter.
    pub frame_seq: u64,
    /// Last timestamp at which the HUD was refreshed.
    pub last_hud_refresh_at_ms: i64,
    /// Overlay transition channels (show/hide fade, value glides).
    pub anim: OverlayAnim,
    /// HUD presentation view-state.
    pub hud_view: HudView,
    /// HUD fade envelope (`0` = invisible, `255` = full). Ramps 0 → 255 on boot
    /// and chip ⇄ full swaps; multiplied into `SetOpacity2` so fading needs no
    /// surface redraw.
    pub hud_envelope: Transition<u8>,
}

impl TickWorld {
    /// Initial state. `last_hud_refresh_at_ms = i64::MIN` forces a first-tick
    /// HUD refresh regardless of clock origin; the envelope starts at `0`.
    pub const INITIAL: Self = Self {
        state: State::DEFAULT,
        last_cursor: None,
        frame_seq: 0,
        last_hud_refresh_at_ms: i64::MIN,
        anim: OverlayAnim::settled_for(State::DEFAULT),
        hud_view: HudView::INITIAL,
        hud_envelope: Transition::settled(0),
    };

    /// Initial state with a caller-supplied [`State`] (non-default boot mode).
    #[must_use]
    pub const fn with_initial_state(state: State) -> Self {
        Self {
            state,
            last_cursor: None,
            frame_seq: 0,
            last_hud_refresh_at_ms: i64::MIN,
            anim: OverlayAnim::settled_for(state),
            hud_view: HudView::INITIAL,
            hud_envelope: Transition::settled(0),
        }
    }

    /// Choose whether the five-second teaching guide appears at startup.
    ///
    /// The application enables this only while creating the first preferences
    /// document. Later launches start with the HUD fully hidden and idle.
    #[must_use]
    pub const fn with_startup_guide(mut self, show: bool) -> Self {
        if !show {
            self.hud_view = HudView {
                tier: HudTier::Hidden,
                user_touched: false,
                boot_at_ms: 0,
            };
            self.hud_envelope = Transition::settled(0);
        }
        self
    }

    /// Whether another vsync-driven tick is required. Off + hidden + settled
    /// is a true idle state; hotkey messages wake the platform explicitly.
    #[must_use]
    pub fn needs_continuous_ticks(self, now_ms: i64) -> bool {
        !matches!(self.state.mode, Mode::Off)
            || !matches!(self.hud_view.tier, HudTier::Hidden)
            || self.anim.master.is_live(now_ms)
            || self.anim.thickness.is_live(now_ms)
            || self.anim.mask_alpha.is_live(now_ms)
            || self.anim.style_mix.is_live(now_ms)
            || self.hud_envelope.is_live(now_ms)
    }
}

impl Default for TickWorld {
    fn default() -> Self {
        Self::INITIAL
    }
}

/// Effects emitted by [`step`]. Order is significant — the platform applies
/// them sequentially.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case", tag = "effect")]
pub enum TickEffect {
    /// Stop the application.
    Quit,
    /// Draw or update the overlay at the given cursor position.
    DrawOverlay {
        /// Axis to render; the `last_active` axis while fading out after `→ Off`.
        mode: Mode,
        /// Cursor position for slit anchoring.
        cursor: Point<Logical>,
        /// Current overlay config.
        config: OverlayConfig,
        /// Interpolated per-tick render values (transition channels).
        sample: OverlaySample,
    },
    /// Hide the overlay (mode off with fade completed, or no cursor yet).
    ClearOverlay,
    /// Refresh the HUD with the supplied state snapshot, in the given
    /// presentation tier.
    RefreshHud {
        /// State snapshot to lay out.
        state: State,
        /// Presentation tier.
        tier: HudTier,
    },
    /// Update HUD opacity for the current cursor distance and fade envelope.
    SetHudOpacity {
        /// Current state (for `mode` / slit-geometry checks).
        state: State,
        /// Cursor position used for the distance calculation.
        cursor: Point<Logical>,
        /// HUD fade envelope sample (`0` = invisible, `255` = full), multiplied
        /// into the distance fade via [`crate::input::hud_fade::apply_envelope`].
        envelope: u8,
    },
    /// Log the last successful reduce in this tick.
    LogStateChanged {
        /// Action that caused the change.
        action: OverlayAction,
        /// New mode.
        mode: Mode,
    },
    /// Surface the last rejected action in this tick; always followed by a
    /// forced `RefreshHud` so the toast shows immediately.
    NotifyRejected {
        /// Why the reducer refused the action.
        reason: RejectReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum OverlayEffect {
    Clear,
    Draw {
        mode: Mode,
        cursor: Point<Logical>,
        sample: OverlaySample,
    },
}

/// Fixed-capacity ordered output of one tick.
///
/// Payload shared by several effects is stored once. Iteration materializes
/// the public [`TickEffect`] values in their stable application order. This
/// avoids both heap allocation and eagerly initializing six copies of the
/// largest enum variant on every tick.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TickEffects {
    state: State,
    state_change: Option<OverlayAction>,
    rejection: Option<RejectReason>,
    quit: bool,
    overlay: Option<OverlayEffect>,
    hud_opacity: Option<(Point<Logical>, u8)>,
    hud_refresh: Option<HudTier>,
}

impl TickEffects {
    /// Empty effect list.
    pub const EMPTY: Self = Self {
        state: State::DEFAULT,
        state_change: None,
        rejection: None,
        quit: false,
        overlay: None,
        hud_opacity: None,
        hud_refresh: None,
    };

    /// Number of emitted effects.
    #[must_use]
    pub fn len(&self) -> usize {
        usize::from(self.state_change.is_some())
            + usize::from(self.rejection.is_some())
            + usize::from(self.quit)
            + usize::from(self.overlay.is_some())
            + usize::from(self.hud_opacity.is_some())
            + usize::from(self.hud_refresh.is_some())
    }

    /// Whether no effects were emitted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Iterate in application order.
    pub fn iter(&self) -> impl Iterator<Item = TickEffect> + '_ {
        (0_u8..)
            .take(MAX_EFFECTS_PER_TICK)
            .filter_map(|slot| self.effect_at(slot))
    }

    fn effect_at(&self, slot: u8) -> Option<TickEffect> {
        match slot {
            0 => self.state_change.map(|action| TickEffect::LogStateChanged {
                action,
                mode: self.state.mode,
            }),
            1 => self
                .rejection
                .map(|reason| TickEffect::NotifyRejected { reason }),
            2 => self.quit.then_some(TickEffect::Quit),
            3 => self.overlay.map(|effect| match effect {
                OverlayEffect::Clear => TickEffect::ClearOverlay,
                OverlayEffect::Draw {
                    mode,
                    cursor,
                    sample,
                } => TickEffect::DrawOverlay {
                    mode,
                    cursor,
                    config: self.state.config,
                    sample,
                },
            }),
            4 => self
                .hud_opacity
                .map(|(cursor, envelope)| TickEffect::SetHudOpacity {
                    state: self.state,
                    cursor,
                    envelope,
                }),
            5 => self.hud_refresh.map(|tier| TickEffect::RefreshHud {
                state: self.state,
                tier,
            }),
            _ => None,
        }
    }
}

impl Default for TickEffects {
    fn default() -> Self {
        Self::EMPTY
    }
}

/// Pure tick step.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "the tick reducer is one ordered, exhaustive state transition"
)]
pub fn step(
    world: TickWorld,
    input: &TickInput,
    telemetry_refresh: Duration,
    anim_config: AnimConfig,
) -> (TickWorld, TickEffects) {
    let prev_state = world.state;
    let mut state = world.state;

    let now = input.now_ms;
    let mut hud_view = world.hud_view;
    let mut hud_envelope = world.hud_envelope;
    if hud_view.boot_at_ms == i64::MIN {
        // First tick: anchor the teaching-period origin and fade the HUD in.
        hud_view.boot_at_ms = now;
        hud_envelope = hud_envelope.retarget(now, u8::MAX, anim_config.hud_swap_ms);
    }

    let mut quit_requested = false;
    let mut last_rejection = None;
    let mut last_state_change = None;
    for action in &input.drained_hotkeys {
        if matches!(action, OverlayAction::Quit) {
            quit_requested = true;
        }
        if matches!(action, OverlayAction::ToggleHudDetail) {
            // View-layer action: reducer no-ops, tier flips here, auto-demote
            // disabled once touched.
            hud_view.tier = hud_view.tier.toggle();
            hud_view.user_touched = true;
        }
        let (next, delta) = apply(state, *action);
        if let Some(reason) = delta.rejected {
            last_rejection = Some(reason);
        }
        if delta.is_any() {
            last_state_change = Some(*action);
        }
        state = next;
    }

    // Auto-hide the first-run guide once startup_full_hud_ms passes, unless the user
    // has toggled.
    if !hud_view.user_touched
        && matches!(hud_view.tier, HudTier::Full)
        && now.saturating_sub(hud_view.boot_at_ms) >= i64::from(anim_config.startup_full_hud_ms)
    {
        hud_view.tier = HudTier::Hidden;
    }

    let anim = retarget_channels(world.anim, prev_state, state, now, anim_config);

    let state_changed = state != prev_state;
    let tier_changed = hud_view.tier != world.hud_view.tier;
    if tier_changed {
        // Chip ⇄ full swap: content/size change instantly (RefreshHud), new
        // look fades in. Ramp-up is less noisy than crossfading panels of
        // different sizes.
        hud_envelope = if matches!(hud_view.tier, HudTier::Hidden) {
            hud_envelope.retarget(now, 0, anim_config.hud_swap_ms)
        } else {
            Transition {
                from: 0,
                to: u8::MAX,
                start_ms: now,
                duration_ms: anim_config.hud_swap_ms,
            }
        };
    }

    let cursor_moved = input.polled_cursor != world.last_cursor;
    let next_cursor = input.polled_cursor;

    let overlay = draw_or_clear(state, anim, next_cursor, now);

    // While the envelope is live, push opacity every tick even with a
    // stationary cursor (only SetOpacity2 updates, so redraw cost is zero).
    let hud_opacity = if let Some(cursor) = next_cursor
        && (cursor_moved || hud_envelope.is_live(now) || tier_changed)
    {
        Some((cursor, hud_envelope.sample(now)))
    } else {
        None
    };
    let interval_ms = i64::try_from(telemetry_refresh.as_millis()).unwrap_or(i64::MAX);
    let interval_elapsed = input.now_ms.saturating_sub(world.last_hud_refresh_at_ms) >= interval_ms;
    // A rejection forces a refresh so the toast shows now, not after the
    // telemetry interval. Iteration keeps NotifyRejected before RefreshHud.
    let hud_refresh =
        (state_changed || tier_changed || interval_elapsed || last_rejection.is_some())
            .then_some(hud_view.tier);
    let next_last_hud_refresh = if hud_refresh.is_some() {
        input.now_ms
    } else {
        world.last_hud_refresh_at_ms
    };

    let next_world = TickWorld {
        state,
        last_cursor: next_cursor,
        frame_seq: world.frame_seq.wrapping_add(1),
        last_hud_refresh_at_ms: next_last_hud_refresh,
        anim,
        hud_view,
        hud_envelope,
    };
    let effects = TickEffects {
        state,
        state_change: last_state_change,
        rejection: last_rejection,
        quit: quit_requested,
        overlay: Some(overlay),
        hud_opacity,
        hud_refresh,
    };

    // Invariants: frame_seq +1, last_hud_refresh_at_ms monotonic (except the
    // i64::MIN sentinel).
    debug_assert!(
        next_world.frame_seq == world.frame_seq.wrapping_add(1),
        "frame_seq must be wrapping_add(1) of previous: prev={}, next={}",
        world.frame_seq,
        next_world.frame_seq
    );
    debug_assert!(
        next_world.last_hud_refresh_at_ms >= world.last_hud_refresh_at_ms
            || world.last_hud_refresh_at_ms == i64::MIN,
        "last_hud_refresh_at_ms must be monotonic: prev={}, next={}",
        world.last_hud_refresh_at_ms,
        next_world.last_hud_refresh_at_ms
    );

    (next_world, effects)
}

/// Draw gate: active mode draws; `Off` keeps drawing along the `last_active`
/// axis while the master fade-out is still landing, then clears once the
/// envelope reaches 0.
fn draw_or_clear(
    state: State,
    anim: OverlayAnim,
    next_cursor: Option<Point<Logical>>,
    now: i64,
) -> OverlayEffect {
    let master_now = anim.master.sample(now);
    match (state.mode, next_cursor) {
        (Mode::Horizontal | Mode::Vertical, Some(cursor)) => OverlayEffect::Draw {
            mode: state.mode,
            cursor,
            sample: anim.sample(now),
        },
        (Mode::Off, Some(cursor)) if master_now > 0 => OverlayEffect::Draw {
            mode: Mode::from(state.last_active),
            cursor,
            sample: anim.sample(now),
        },
        _ => OverlayEffect::Clear,
    }
}

/// Retargets each transition channel from the `prev → next` diff. Re-bases from
/// the current sample (`crate::anim`), so mid-flight repeats keep gliding.
fn retarget_channels(
    mut anim: OverlayAnim,
    prev: State,
    next: State,
    now_ms: i64,
    cfg: AnimConfig,
) -> OverlayAnim {
    if next.mode != prev.mode {
        anim.master = match (prev.mode.active(), next.mode.active()) {
            // Off → active: fade in (re-bases mid-fade-out, so a fast double
            // toggle rises smoothly).
            (None, Some(_)) => anim.master.retarget(now_ms, u8::MAX, cfg.overlay_fade_ms),
            // active → Off: fade out.
            (Some(_), None) => anim.master.retarget(now_ms, 0, cfg.overlay_fade_ms),
            // H ⇄ V: soft cut, new axis fades in from 0. Hard reset (not
            // retarget) since the faded axis changed identity.
            (Some(_), Some(_)) => Transition {
                from: 0,
                to: u8::MAX,
                start_ms: now_ms,
                duration_ms: cfg.overlay_fade_ms,
            },
            // Unreachable: mode changed but both ends are Off.
            (None, None) => anim.master,
        };
    }
    if next.config.thickness != prev.config.thickness {
        anim.thickness =
            anim.thickness
                .retarget(now_ms, next.config.thickness.get(), cfg.value_glide_ms);
    }
    if next.config.opacity != prev.config.opacity {
        anim.mask_alpha =
            anim.mask_alpha
                .retarget(now_ms, next.config.opacity.get(), cfg.value_glide_ms);
    }
    if next.config.effect != prev.config.effect {
        if next.config.effect.is_blur() || prev.config.effect.is_blur() {
            // Flat ⇄ Blur: brush kind flips Solid ⇄ Blur, rebuilding the sprite
            // pool, which a color crossfade can't bridge. Ride the master
            // envelope (soft cut, as H ⇄ V) and settle style immediately.
            anim.style_mix = Transition::settled(next.config.effect.mix_target());
            if next.mode == prev.mode {
                anim.master = Transition {
                    from: 0,
                    to: u8::MAX,
                    start_ms: now_ms,
                    duration_ms: cfg.overlay_fade_ms,
                };
            }
        } else {
            // Flat ⇄ flat (DimBlack ⇄ WhiteWash): RGB crossfade.
            anim.style_mix = anim.style_mix.retarget(
                now_ms,
                next.config.effect.mix_target(),
                cfg.overlay_fade_ms,
            );
        }
    }
    // `config.blur` (σ) has no transition channel: σ is part of the sprite-pool
    // signature, so gliding it would rebuild the pool every tick. `BlurAmount`
    // steps are perceptually uniform, so stepping is smooth enough.
    anim
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    const TELEMETRY: Duration = Duration::from_millis(200);
    const ANIM: AnimConfig = AnimConfig::DEFAULT;

    fn world() -> TickWorld {
        TickWorld::INITIAL
    }

    fn input(now_ms: i64) -> TickInput {
        TickInput {
            now_ms,
            polled_cursor: None,
            drained_hotkeys: ActionBatch::EMPTY,
        }
    }

    fn hidden_idle_world() -> TickWorld {
        TickWorld {
            hud_view: HudView {
                tier: HudTier::Hidden,
                user_touched: true,
                boot_at_ms: 0,
            },
            last_hud_refresh_at_ms: 0,
            ..TickWorld::INITIAL
        }
    }

    #[test]
    fn action_batch_is_bounded_ordered_and_never_truncates_silently() {
        let mut batch = ActionBatch::EMPTY;
        assert_eq!(ActionBatch::default(), ActionBatch::EMPTY);
        assert!(batch.is_empty());
        assert!(!batch.is_full());
        for index in 0..ActionBatch::CAPACITY {
            let action = if index.is_multiple_of(2) {
                OverlayAction::CycleMode
            } else {
                OverlayAction::ToggleOnOff
            };
            batch.try_push(action).expect("action fits");
        }
        assert!(!batch.is_empty());
        assert!(batch.is_full());
        assert_eq!(batch.len(), ActionBatch::CAPACITY);
        assert_eq!(batch.as_slice()[0], OverlayAction::CycleMode);
        assert_eq!(batch.as_slice()[1], OverlayAction::ToggleOnOff);
        assert_eq!(
            batch.try_push(OverlayAction::Quit),
            Err(OverlayAction::Quit)
        );

        let oversized =
            std::iter::repeat_n(OverlayAction::BumpOpacity(8), ActionBatch::CAPACITY + 1);
        assert_eq!(
            ActionBatch::try_from_actions(oversized),
            Err(OverlayAction::BumpOpacity(8))
        );
    }

    #[test]
    fn tick_effect_collection_reports_length_emptiness_and_iteration() {
        let empty = TickEffects::EMPTY;
        assert_eq!(TickEffects::default(), empty);
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
        assert_eq!(empty.iter().count(), 0);

        let (_, effects) = step(world(), &input(0), TELEMETRY, ANIM);
        assert!(!effects.is_empty());
        assert_eq!(effects.len(), 2);
        assert_eq!(effects.iter().count(), effects.len());
    }

    #[test]
    fn maximum_effect_tick_fills_the_exact_fixed_capacity() {
        let mut full = input(0);
        full.polled_cursor = Some(Point::new(960, 540));
        for action in [
            OverlayAction::ToggleOnOff,
            OverlayAction::ToggleOnOff,
            OverlayAction::BumpThickness(8),
        ] {
            full.drained_hotkeys
                .try_push(action)
                .expect("capacity-sized action sequence");
        }
        for _ in full.drained_hotkeys.len()..ActionBatch::CAPACITY {
            full.drained_hotkeys
                .try_push(OverlayAction::Quit)
                .expect("capacity-sized action sequence");
        }

        let (_, effects) = step(world(), &full, TELEMETRY, ANIM);
        assert_eq!(effects.len(), MAX_EFFECTS_PER_TICK);
        assert_eq!(
            effects
                .iter()
                .filter(|effect| matches!(effect, TickEffect::LogStateChanged { .. }))
                .count(),
            1
        );
        assert_eq!(
            effects
                .iter()
                .filter(|effect| matches!(effect, TickEffect::NotifyRejected { .. }))
                .count(),
            1
        );
        let materialized = effects.iter().collect::<Vec<_>>();
        assert!(matches!(
            materialized[0],
            TickEffect::LogStateChanged {
                action: OverlayAction::ToggleOnOff,
                mode: Mode::Off
            }
        ));
        assert!(matches!(
            materialized[1],
            TickEffect::NotifyRejected {
                reason: RejectReason::AdjustWhileOff
            }
        ));
        assert!(matches!(
            materialized[MAX_EFFECTS_PER_TICK - 3],
            TickEffect::ClearOverlay
        ));
        assert!(matches!(
            materialized[MAX_EFFECTS_PER_TICK - 2],
            TickEffect::SetHudOpacity { .. }
        ));
        assert!(matches!(
            materialized[MAX_EFFECTS_PER_TICK - 1],
            TickEffect::RefreshHud { .. }
        ));
    }

    #[test]
    fn state_change_telemetry_keeps_only_the_last_change_in_a_tick() {
        let mut actions = input(0);
        for action in [
            OverlayAction::ToggleOnOff,
            OverlayAction::CycleMode,
            OverlayAction::BumpThickness(8),
        ] {
            actions
                .drained_hotkeys
                .try_push(action)
                .expect("test action");
        }

        let (_, effects) = step(world(), &actions, TELEMETRY, ANIM);
        let logs = effects
            .iter()
            .filter_map(|effect| match effect {
                TickEffect::LogStateChanged { action, mode } => Some((action, mode)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            logs,
            vec![(OverlayAction::BumpThickness(8), Mode::Vertical)]
        );
    }

    #[test]
    fn continuous_ticks_are_required_by_each_independent_visual_source() {
        let now = 50;
        let idle = hidden_idle_world();
        assert_eq!(TickWorld::default(), TickWorld::INITIAL);
        assert!(!idle.needs_continuous_ticks(now));

        let active = TickWorld {
            state: State::with_mode(Mode::Horizontal),
            ..idle
        };
        assert!(active.needs_continuous_ticks(now));

        let visible_hud = TickWorld {
            hud_view: HudView {
                tier: HudTier::Full,
                ..idle.hud_view
            },
            ..idle
        };
        assert!(visible_hud.needs_continuous_ticks(now));

        let live_u8 = Transition {
            from: 0,
            to: u8::MAX,
            start_ms: 0,
            duration_ms: 100,
        };
        let live_u16 = Transition {
            from: 1,
            to: 2,
            start_ms: 0,
            duration_ms: 100,
        };

        let mut candidate = idle;
        candidate.anim.master = live_u8;
        assert!(candidate.needs_continuous_ticks(now));

        let mut candidate = idle;
        candidate.anim.thickness = live_u16;
        assert!(candidate.needs_continuous_ticks(now));

        let mut candidate = idle;
        candidate.anim.mask_alpha = live_u8;
        assert!(candidate.needs_continuous_ticks(now));

        let mut candidate = idle;
        candidate.anim.style_mix = live_u8;
        assert!(candidate.needs_continuous_ticks(now));

        let candidate = TickWorld {
            hud_envelope: live_u8,
            ..idle
        };
        assert!(candidate.needs_continuous_ticks(now));
    }

    #[test]
    fn later_launch_starts_with_the_guide_hidden_and_idle() {
        let world = TickWorld::with_initial_state(State::DEFAULT).with_startup_guide(false);
        assert_eq!(world.hud_view.tier, HudTier::Hidden);
        assert_eq!(world.hud_view.boot_at_ms, 0);
        assert!(!world.needs_continuous_ticks(0));

        let (next, effects) = step(world, &input(0), TELEMETRY, ANIM);
        assert_eq!(next.hud_view.tier, HudTier::Hidden);
        assert!(!next.hud_envelope.is_live(0));
        assert!(!next.needs_continuous_ticks(0));
        assert!(
            effects
                .iter()
                .all(|effect| !matches!(effect, TickEffect::SetHudOpacity { .. }))
        );
    }

    #[test]
    fn first_launch_keeps_the_teaching_guide_active() {
        let world = TickWorld::with_initial_state(State::DEFAULT).with_startup_guide(true);

        assert_eq!(world.hud_view, HudView::INITIAL);
        assert!(world.needs_continuous_ticks(0));
    }

    #[test]
    fn retarget_channels_tracks_each_changed_value() {
        let previous = State::with_mode(Mode::Horizontal);
        let animation = OverlayAnim::settled_for(previous);

        let mut thicker = previous;
        thicker.config.thickness = thicker.config.thickness.saturating_add(7);
        let thickness_animation = retarget_channels(animation, previous, thicker, 25, ANIM);
        assert_eq!(
            thickness_animation.thickness,
            Transition {
                from: previous.config.thickness.get(),
                to: thicker.config.thickness.get(),
                start_ms: 25,
                duration_ms: ANIM.value_glide_ms,
            }
        );

        let mut more_opaque = previous;
        more_opaque.config.opacity = more_opaque.config.opacity.saturating_add(9);
        let opacity_animation = retarget_channels(animation, previous, more_opaque, 40, ANIM);
        assert_eq!(
            opacity_animation.mask_alpha,
            Transition {
                from: previous.config.opacity.get(),
                to: more_opaque.config.opacity.get(),
                start_ms: 40,
                duration_ms: ANIM.value_glide_ms,
            }
        );

        let mut white_wash = previous;
        white_wash.config.effect = crate::SurroundEffect::WhiteWash;
        let _ = retarget_channels(animation, previous, white_wash, 50, ANIM);

        let mut blur = previous;
        blur.config.effect = crate::SurroundEffect::Blur;
        let mut dim_after_blur = blur;
        dim_after_blur.config.effect = crate::SurroundEffect::DimBlack;
        let _ = retarget_channels(
            OverlayAnim::settled_for(blur),
            blur,
            dim_after_blur,
            60,
            ANIM,
        );

        let mut vertical_blur = previous;
        vertical_blur.mode = Mode::Vertical;
        vertical_blur.config.effect = crate::SurroundEffect::Blur;
        let _ = retarget_channels(animation, previous, vertical_blur, 70, ANIM);
    }

    #[test]
    fn flat_to_blur_soft_cut_restarts_master_in_the_same_mode() {
        let previous = State::with_mode(Mode::Horizontal);
        let mut blur = previous;
        blur.config.effect = crate::SurroundEffect::Blur;
        let animation =
            retarget_channels(OverlayAnim::settled_for(previous), previous, blur, 60, ANIM);
        assert_eq!(
            animation.master,
            Transition {
                from: 0,
                to: u8::MAX,
                start_ms: 60,
                duration_ms: ANIM.overlay_fade_ms,
            }
        );
        assert_eq!(
            animation.style_mix,
            Transition::settled(blur.config.effect.mix_target())
        );
    }

    #[test]
    fn empty_tick_clears_and_refreshes_hud() {
        let (next, fx) = step(world(), &input(0), TELEMETRY, ANIM);
        assert_eq!(fx.iter().next(), Some(TickEffect::ClearOverlay));
        assert!(matches!(
            fx.iter().last(),
            Some(TickEffect::RefreshHud { .. })
        ));
        assert_eq!(next.frame_seq, 1);
    }

    #[test]
    fn toggle_on_emits_log_and_draws_overlay() {
        let mut input = input(0);
        input
            .drained_hotkeys
            .try_push(OverlayAction::ToggleOnOff)
            .expect("test action");
        input.polled_cursor = Some(Point::new(100, 100));
        let (next, fx) = step(world(), &input, TELEMETRY, ANIM);
        assert!(matches!(
            fx.iter().next(),
            Some(TickEffect::LogStateChanged { .. })
        ));
        assert!(
            fx.iter()
                .any(|e| matches!(e, TickEffect::DrawOverlay { .. }))
        );
        assert_eq!(next.state.mode, Mode::Horizontal);
    }

    #[test]
    fn quit_action_emits_quit_effect() {
        let mut input = input(0);
        input
            .drained_hotkeys
            .try_push(OverlayAction::Quit)
            .expect("test action");
        let (_, fx) = step(world(), &input, TELEMETRY, ANIM);
        assert!(fx.iter().any(|effect| effect == TickEffect::Quit));
    }

    /// Adjust key while Off: `NotifyRejected` + forced `RefreshHud` inside the
    /// telemetry interval; no state change, so no `LogStateChanged`.
    #[test]
    fn bump_while_off_emits_notify_rejected_and_forces_hud_refresh() {
        // Tick 1: consume the initial refresh so last_hud_refresh_at_ms is
        // current.
        let (w1, _) = step(world(), &input(0), TELEMETRY, ANIM);
        // Tick 2: BumpThickness while still Off, inside the interval (200ms).
        let mut i2 = input(50);
        i2.drained_hotkeys
            .try_push(OverlayAction::BumpThickness(8))
            .expect("test action");
        let (w2, fx) = step(w1, &i2, TELEMETRY, ANIM);
        assert_eq!(w2.state, w1.state, "rejected action must not change state");
        let notify_pos = fx.iter().position(|e| {
            matches!(
                e,
                TickEffect::NotifyRejected {
                    reason: RejectReason::AdjustWhileOff
                }
            )
        });
        let refresh_pos = fx
            .iter()
            .position(|e| matches!(e, TickEffect::RefreshHud { .. }));
        assert!(notify_pos.is_some(), "expected NotifyRejected, got {fx:?}");
        assert!(
            refresh_pos.is_some(),
            "rejection must force RefreshHud within the interval, got {fx:?}"
        );
        assert!(
            notify_pos < refresh_pos,
            "NotifyRejected must precede RefreshHud so the toast renders this tick"
        );
        assert!(
            !fx.iter()
                .any(|e| matches!(e, TickEffect::LogStateChanged { .. })),
            "no state change → no LogStateChanged"
        );
    }

    /// Saturation (a no-op edge within an active mode) is not a rejection,
    /// so no `NotifyRejected`.
    #[test]
    fn saturated_bump_in_active_mode_emits_no_notify() {
        let mut on = input(0);
        on.drained_hotkeys
            .try_push(OverlayAction::ToggleOnOff)
            .expect("test action");
        let (w1, _) = step(world(), &on, TELEMETRY, ANIM);
        // Drop opacity straight to MIN, then bump down again to saturate.
        let mut i2 = input(50);
        i2.drained_hotkeys
            .try_push(OverlayAction::BumpOpacity(-100_000))
            .expect("test action");
        let (w2, _) = step(w1, &i2, TELEMETRY, ANIM);
        let mut i3 = input(100);
        i3.drained_hotkeys
            .try_push(OverlayAction::BumpOpacity(-8))
            .expect("test action");
        let (_, fx) = step(w2, &i3, TELEMETRY, ANIM);
        assert!(
            !fx.iter()
                .any(|e| matches!(e, TickEffect::NotifyRejected { .. })),
            "saturation must stay silent, got {fx:?}"
        );
    }

    /// `ToggleOnOff` round trip: toggling off still Draws (along `last_active`)
    /// while the fade-out runs; Clear comes only after the fade completes.
    #[test]
    fn toggle_on_off_fades_out_then_clears() {
        let cursor = Some(Point::new(100, 100));
        let mut on = input(0);
        on.polled_cursor = cursor;
        on.drained_hotkeys
            .try_push(OverlayAction::ToggleOnOff)
            .expect("test action");
        let (w1, fx1) = step(world(), &on, TELEMETRY, ANIM);
        assert_eq!(
            w1.state.mode,
            Mode::Horizontal,
            "DEFAULT restores Horizontal"
        );
        assert!(
            fx1.iter()
                .any(|e| matches!(e, TickEffect::DrawOverlay { .. })),
            "restored mode must draw, got {fx1:?}"
        );

        // Toggle Off after the fade-in completes.
        let fade = i64::from(ANIM.overlay_fade_ms);
        let mut off = input(fade + 10);
        off.polled_cursor = cursor;
        off.drained_hotkeys
            .try_push(OverlayAction::ToggleOnOff)
            .expect("test action");
        let (w2, fx2) = step(w1, &off, TELEMETRY, ANIM);
        assert_eq!(w2.state.mode, Mode::Off);
        let draw = fx2.iter().find_map(|e| match e {
            TickEffect::DrawOverlay { mode, sample, .. } => Some((mode, sample)),
            _ => None,
        });
        let (mode, sample) = draw.expect("fade-out must keep drawing");
        assert_eq!(mode, Mode::Horizontal, "fade-out renders the last axis");
        assert_eq!(sample.master, 255, "retarget re-bases from current value");

        // An empty tick after the fade-out completes Clears.
        let mut after = input(fade + 10 + fade + 1);
        after.polled_cursor = cursor;
        let (_, fx3) = step(w2, &after, TELEMETRY, ANIM);
        assert!(
            fx3.iter().any(|e| matches!(e, TickEffect::ClearOverlay)),
            "after the fade completes, Off must clear, got {fx3:?}"
        );
    }

    /// Off → active fade-in: master starts at 0 and reaches 255 at
    /// completion.
    #[test]
    fn mode_on_fade_ramps_master_to_full() {
        let cursor = Some(Point::new(100, 100));
        let mut on = input(0);
        on.polled_cursor = cursor;
        on.drained_hotkeys
            .try_push(OverlayAction::ToggleOnOff)
            .expect("test action");
        let (w1, fx1) = step(world(), &on, TELEMETRY, ANIM);
        let s1 = fx1.iter().find_map(sample_of).expect("draw on activation");
        assert_eq!(s1.master, 0, "fade-in starts from 0 at the trigger tick");

        let mut mid = input(i64::from(ANIM.overlay_fade_ms) / 2);
        mid.polled_cursor = cursor;
        let (w2, fx2) = step(w1, &mid, TELEMETRY, ANIM);
        let s2 = fx2.iter().find_map(sample_of).expect("draw mid-fade");
        assert!(
            s2.master > 0 && s2.master < 255,
            "mid-fade master must be between, got {}",
            s2.master
        );

        let mut done = input(i64::from(ANIM.overlay_fade_ms) + 1);
        done.polled_cursor = cursor;
        let (_, fx3) = step(w2, &done, TELEMETRY, ANIM);
        let s3 = fx3.iter().find_map(sample_of).expect("draw after fade");
        assert_eq!(s3.master, 255, "fade-in must land exactly at 255");
    }

    /// Thickness sample never regresses when a bump lands mid-flight
    /// (glide-continuity, pipeline level).
    #[test]
    fn held_bumps_glide_thickness_without_regression() {
        let cursor = Some(Point::new(100, 100));
        let mut w = TickWorld::with_initial_state(State::with_mode(Mode::Horizontal));
        let mut last = 0_u16;
        let mut now = 0_i64;
        // Four +8 bumps at 50ms → each lands mid-glide (130ms).
        for _ in 0..4 {
            let mut i = input(now);
            i.polled_cursor = cursor;
            i.drained_hotkeys
                .try_push(OverlayAction::BumpThickness(8))
                .expect("test action");
            let (next, fx) = step(w, &i, TELEMETRY, ANIM);
            let s = fx.iter().find_map(sample_of).expect("active mode draws");
            assert!(
                s.thickness_px >= last,
                "thickness sample regressed: {last} -> {}",
                s.thickness_px
            );
            last = s.thickness_px;
            w = next;
            now += 50;
        }
        // After the final glide completes, the sample lands exactly on the
        // target (28 + 4*8 = 60).
        let mut i = input(now + i64::from(ANIM.value_glide_ms) + 1);
        i.polled_cursor = cursor;
        let (_, fx) = step(w, &i, TELEMETRY, ANIM);
        let s = fx.iter().find_map(sample_of).expect("draw");
        assert_eq!(s.thickness_px, 60);
    }

    /// After all transitions complete, the sample equals
    /// `OverlaySample::settled(config)` exactly (no steady-state drift).
    #[test]
    fn settled_sample_matches_config_after_transitions_complete() {
        let cursor = Some(Point::new(100, 100));
        let mut on = input(0);
        on.polled_cursor = cursor;
        on.drained_hotkeys
            .try_push(OverlayAction::ToggleOnOff)
            .expect("test action");
        let (w1, _) = step(world(), &on, TELEMETRY, ANIM);
        let mut later = input(10_000);
        later.polled_cursor = cursor;
        let (_, fx) = step(w1, &later, TELEMETRY, ANIM);
        let s = fx.iter().find_map(sample_of).expect("draw");
        assert_eq!(s, crate::render::OverlaySample::settled(w1.state.config));
    }

    fn sample_of(e: TickEffect) -> Option<crate::render::OverlaySample> {
        match e {
            TickEffect::DrawOverlay { sample, .. } => Some(sample),
            _ => None,
        }
    }

    fn refresh_tier_of(e: TickEffect) -> Option<HudTier> {
        match e {
            TickEffect::RefreshHud { tier, .. } => Some(tier),
            _ => None,
        }
    }

    // ---- HUD tier FSM ------------------------------------------------------

    /// Boots full, auto-demotes to chip on the first tick after
    /// `startup_full_hud_ms`, which emits `RefreshHud { tier: Hidden }`.
    #[test]
    fn hud_boots_full_then_auto_hides() {
        let (w1, fx1) = step(world(), &input(0), TELEMETRY, ANIM);
        assert_eq!(
            fx1.iter().find_map(refresh_tier_of),
            Some(HudTier::Full),
            "boot tick must refresh in Full tier"
        );
        assert_eq!(w1.hud_view.boot_at_ms, 0, "boot origin set on first tick");

        // Within the teaching period: tier stays Full.
        let (w2, _) = step(w1, &input(1_000), TELEMETRY, ANIM);
        assert_eq!(w2.hud_view.tier, HudTier::Full);

        // Teaching period over: auto-demote to Chip + RefreshHud that tick.
        let demote_at = i64::from(ANIM.startup_full_hud_ms);
        let (w3, fx3) = step(w2, &input(demote_at), TELEMETRY, ANIM);
        assert_eq!(w3.hud_view.tier, HudTier::Hidden);
        assert_eq!(
            fx3.iter().find_map(refresh_tier_of),
            Some(HudTier::Hidden),
            "timeout tick must refresh in Hidden tier, got {fx3:?}"
        );
    }

    /// `ToggleHudDetail` flips the tier, sets `user_touched`, and disables
    /// auto-demotion thereafter.
    #[test]
    fn user_toggle_flips_tier_and_disables_auto_demote() {
        let (w1, _) = step(world(), &input(0), TELEMETRY, ANIM);

        // User collapses to Chip during the teaching period.
        let mut i2 = input(100);
        i2.drained_hotkeys
            .try_push(OverlayAction::ToggleHudDetail)
            .expect("test action");
        let (w2, fx2) = step(w1, &i2, TELEMETRY, ANIM);
        assert_eq!(w2.hud_view.tier, HudTier::Hidden);
        assert!(w2.hud_view.user_touched);
        assert_eq!(
            fx2.iter().find_map(refresh_tier_of),
            Some(HudTier::Hidden),
            "tier change must force a refresh, got {fx2:?}"
        );

        // Toggle again back to Full.
        let mut i3 = input(200);
        i3.drained_hotkeys
            .try_push(OverlayAction::ToggleHudDetail)
            .expect("test action");
        let (w3, _) = step(w2, &i3, TELEMETRY, ANIM);
        assert_eq!(w3.hud_view.tier, HudTier::Full);

        // Well past the teaching period: no auto-demotion after
        // user_touched.
        let far = i64::from(ANIM.startup_full_hud_ms) * 3;
        let (w4, _) = step(w3, &input(far), TELEMETRY, ANIM);
        assert_eq!(
            w4.hud_view.tier,
            HudTier::Full,
            "auto-demote must stay disabled after a manual toggle"
        );
    }

    /// `ToggleHudDetail` does not change the overlay `State` (no
    /// `LogStateChanged`, no `NotifyRejected`).
    #[test]
    fn toggle_hud_detail_does_not_touch_overlay_state() {
        let mut i = input(0);
        i.drained_hotkeys
            .try_push(OverlayAction::ToggleHudDetail)
            .expect("test action");
        let (w, fx) = step(world(), &i, TELEMETRY, ANIM);
        assert_eq!(w.state, TickWorld::INITIAL.state);
        assert!(
            !fx.iter().any(|e| matches!(
                e,
                TickEffect::LogStateChanged { .. } | TickEffect::NotifyRejected { .. }
            )),
            "view-layer action must not produce state-change effects, got {fx:?}"
        );
    }

    #[test]
    fn hud_refresh_is_skipped_when_neither_state_changed_nor_interval_elapsed() {
        let (w1, _) = step(world(), &input(0), TELEMETRY, ANIM);
        let (_, fx2) = step(w1, &input(100), TELEMETRY, ANIM);
        assert!(
            !fx2.iter()
                .any(|e| matches!(e, TickEffect::RefreshHud { .. }))
        );
    }

    fn envelope_of(fx: &TickEffects) -> Option<u8> {
        fx.iter().find_map(|e| match e {
            TickEffect::SetHudOpacity { envelope, .. } => Some(envelope),
            _ => None,
        })
    }

    /// `SetHudOpacity` emit conditions: while the envelope is live, emit every
    /// tick even with a stationary cursor; once settled, only when the cursor
    /// moved.
    #[test]
    fn set_hud_opacity_emission_follows_cursor_and_envelope() {
        let p1 = Point::new(100, 100);

        // Tick 1 (boot): envelope ramps 0 → 255 → emit (value 0).
        let mut i1 = input(0);
        i1.polled_cursor = Some(p1);
        let (w1, fx1) = step(world(), &i1, TELEMETRY, ANIM);
        assert_eq!(
            envelope_of(&fx1),
            Some(0),
            "boot tick emits with envelope 0 (fade-in starts)"
        );

        // Tick 2: emit despite the stationary cursor (envelope live); the
        // value advances.
        let mut i2 = input(50);
        i2.polled_cursor = Some(p1);
        let (w2, fx2) = step(w1, &i2, TELEMETRY, ANIM);
        let mid = envelope_of(&fx2).expect("live envelope keeps emitting");
        assert!(
            mid > 0 && mid < 255,
            "mid-fade envelope must be between, got {mid}"
        );

        // Tick 3: envelope settled + stationary cursor → no emit.
        let settled_at = i64::from(ANIM.hud_swap_ms) + 10;
        let mut i3 = input(settled_at);
        i3.polled_cursor = Some(p1);
        let (w3, fx3) = step(w2, &i3, TELEMETRY, ANIM);
        assert!(
            envelope_of(&fx3).is_none(),
            "settled envelope + stationary cursor must not emit, got {fx3:?}"
        );

        // Tick 4: cursor movement emits (envelope at its settled value 255).
        let mut i4 = input(settled_at + 50);
        i4.polled_cursor = Some(Point::new(200, 100));
        let (_, fx4) = step(w3, &i4, TELEMETRY, ANIM);
        assert_eq!(
            envelope_of(&fx4),
            Some(255),
            "moved cursor must emit with the settled envelope"
        );
    }

    /// Hiding fades out from the current envelope; showing restarts at zero.
    #[test]
    fn tier_swap_uses_directional_hud_envelope() {
        let p1 = Point::new(100, 100);
        let mut i1 = input(0);
        i1.polled_cursor = Some(p1);
        let (w1, _) = step(world(), &i1, TELEMETRY, ANIM);

        // Let the boot fade-in complete.
        let mut i2 = input(i64::from(ANIM.hud_swap_ms) + 10);
        i2.polled_cursor = Some(p1);
        let (w2, _) = step(w1, &i2, TELEMETRY, ANIM);

        // Full -> Hidden starts from the current fully-visible value.
        let mut i3 = input(i64::from(ANIM.hud_swap_ms) + 100);
        i3.polled_cursor = Some(p1);
        i3.drained_hotkeys
            .try_push(OverlayAction::ToggleHudDetail)
            .expect("test action");
        let (w3, fx3) = step(w2, &i3, TELEMETRY, ANIM);
        assert_eq!(
            envelope_of(&fx3),
            Some(255),
            "hide must fade out from the current envelope, got {fx3:?}"
        );

        // Once hidden, Hidden -> Full starts the new look at zero.
        let mut i4 = input(i3.now_ms + i64::from(ANIM.hud_swap_ms) + 1);
        i4.polled_cursor = Some(p1);
        i4.drained_hotkeys
            .try_push(OverlayAction::ToggleHudDetail)
            .expect("test action");
        let (_, fx4) = step(w3, &i4, TELEMETRY, ANIM);
        assert_eq!(envelope_of(&fx4), Some(0), "show must fade in, got {fx4:?}");
    }
}
