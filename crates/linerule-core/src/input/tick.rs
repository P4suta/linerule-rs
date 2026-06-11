//! Pure tick pipeline: turns a tick's inputs (drained hotkeys, polled cursor,
//! timestamp) into the next [`TickWorld`] and a list of [`TickEffect`] for the
//! platform layer to apply.

use std::time::Duration;

use serde::Serialize;

use crate::{
    anim::Transition,
    config::{AnimConfig, OverlayConfig},
    geometry::{Logical, Point},
    render::{HudTier, OverlaySample},
    state::{Mode, OverlayAction, RejectReason, State, reduce},
};

/// Per-tick input from the platform.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TickInput {
    /// Current timestamp (millisecond).
    pub now_ms: i64,
    /// Latest cursor sample from the OS (`None` if not yet known).
    pub polled_cursor: Option<Point<Logical>>,
    /// Hotkey actions drained from the platform channel this tick.
    pub drained_hotkeys: Vec<OverlayAction>,
}

/// Transition channels driving the overlay's visual glides. Lives inside
/// [`TickWorld`]; endpoints are integers so the world stays `Eq + Hash`
/// (see [`crate::anim`]).
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

    /// Sample bundle at the current time; the values carried by the
    /// `DrawOverlay` effect.
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

/// HUD presentation view-state.
///
/// Lives in `TickWorld` rather than `State`: it is irrelevant to the reducer
/// and `render::frame`, and carries time-coupled display state (`boot_at_ms`)
/// — same family as `last_hud_refresh_at_ms`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct HudView {
    /// Current presentation tier (chip / full).
    pub tier: HudTier,
    /// Whether the user has ever pressed `ToggleHudDetail`. Once `true`, the
    /// startup auto-demotion (Full → Chip) never fires.
    pub user_touched: bool,
    /// Time of the first tick (origin of the startup full display).
    /// `i64::MIN` is the unset sentinel.
    pub boot_at_ms: i64,
}

impl HudView {
    /// Right after boot: full display (hotkey-guide teaching period), boot
    /// time unset.
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
    /// HUD presentation tier (chip / full) view-state.
    pub hud_view: HudView,
    /// HUD fade envelope (`0` = invisible, `255` = full). Ramps 0 → 255 on
    /// boot and on chip ⇄ full swaps. The platform multiplies it into
    /// `SetOpacity2` (visual layer), so no surface redraw happens while
    /// fading.
    pub hud_envelope: Transition<u8>,
}

impl TickWorld {
    /// Initial state. `last_hud_refresh_at_ms = i64::MIN` forces the first tick
    /// to refresh the HUD regardless of clock origin. The HUD envelope starts
    /// at `0` and fades in on the first tick.
    pub const INITIAL: Self = Self {
        state: State::DEFAULT,
        last_cursor: None,
        frame_seq: 0,
        last_hud_refresh_at_ms: i64::MIN,
        anim: OverlayAnim::settled_for(State::DEFAULT),
        hud_view: HudView::INITIAL,
        hud_envelope: Transition::settled(0),
    };

    /// Initial state with a caller-supplied [`State`], for booting directly
    /// into a non-default mode.
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
        /// Axis to render. While fading out after `→ Off` this is the
        /// `last_active` axis (the state's mode is already `Off`).
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
    /// presentation tier (chip / full).
    RefreshHud {
        /// State snapshot to lay out.
        state: State,
        /// Presentation tier (chip / full).
        tier: HudTier,
    },
    /// Update HUD opacity for the current cursor distance and fade envelope.
    SetHudOpacity {
        /// Current state (for `mode` / slit-geometry checks).
        state: State,
        /// Cursor position used for the distance calculation.
        cursor: Point<Logical>,
        /// HUD fade envelope sample (`0` = invisible, `255` = full). The
        /// platform multiplies it into the distance fade through the
        /// perceptual curve ([`crate::input::hud_fade::apply_envelope`]).
        envelope: u8,
    },
    /// Log a `LogStateChanged` event after a successful reduce.
    LogStateChanged {
        /// Action that caused the change.
        action: OverlayAction,
        /// New mode.
        mode: Mode,
    },
    /// Surface a rejected action to the user. The platform layer formats the
    /// reason (with the actual configured hotkey strings) and toasts it on
    /// the HUD. Always followed by a forced `RefreshHud` in the same tick so
    /// the toast appears immediately.
    NotifyRejected {
        /// Why the reducer refused the action.
        reason: RejectReason,
    },
}

/// Pure tick step.
#[must_use]
pub fn step(
    world: TickWorld,
    input: &TickInput,
    telemetry_refresh: Duration,
    anim_config: AnimConfig,
) -> (TickWorld, Vec<TickEffect>) {
    let mut effects = Vec::with_capacity(4);

    let prev_state = world.state;
    let mut state = world.state;

    let now = input.now_ms;
    let mut hud_view = world.hud_view;
    let mut hud_envelope = world.hud_envelope;
    if hud_view.boot_at_ms == i64::MIN {
        // First tick: fix the origin of the startup full display (teaching
        // period) and fade the HUD in 0 → 255.
        hud_view.boot_at_ms = now;
        hud_envelope = hud_envelope.retarget(now, u8::MAX, anim_config.hud_swap_ms);
    }

    let mut quit_requested = false;
    let mut rejected_this_tick = false;
    for action in &input.drained_hotkeys {
        if matches!(action, OverlayAction::Quit) {
            quit_requested = true;
        }
        if matches!(action, OverlayAction::ToggleHudDetail) {
            // View-layer action: the reducer no-ops; the tier flips here.
            // Once touched, the startup auto-demotion never fires again.
            hud_view.tier = hud_view.tier.toggle();
            hud_view.user_touched = true;
        }
        let (next, delta) = reduce::apply(state, *action);
        if let Some(reason) = delta.rejected {
            rejected_this_tick = true;
            effects.push(TickEffect::NotifyRejected { reason });
        }
        if delta.is_any() {
            effects.push(TickEffect::LogStateChanged {
                action: *action,
                mode: next.mode,
            });
        }
        state = next;
    }

    // The startup full display auto-demotes to chip once the teaching period
    // (startup_full_hud_ms) has passed. After an explicit user toggle,
    // respect their choice and leave the tier alone.
    if !hud_view.user_touched
        && matches!(hud_view.tier, HudTier::Full)
        && now.saturating_sub(hud_view.boot_at_ms) >= i64::from(anim_config.startup_full_hud_ms)
    {
        hud_view.tier = HudTier::Chip;
    }

    if quit_requested {
        effects.push(TickEffect::Quit);
    }

    let anim = retarget_channels(world.anim, prev_state, state, now, anim_config);

    let state_changed = state != prev_state;
    let tier_changed = hud_view.tier != world.hud_view.tier;
    if tier_changed {
        // Chip ⇄ full swap: content and size change instantly (RefreshHud)
        // and the new look fades in 0 → 255. A short ramp-up is less noisy
        // than crossfading panels of different sizes.
        hud_envelope = Transition {
            from: 0,
            to: u8::MAX,
            start_ms: now,
            duration_ms: anim_config.hud_swap_ms,
        };
    }

    let cursor_moved = input.polled_cursor != world.last_cursor;
    let next_cursor = input.polled_cursor;

    effects.push(draw_or_clear(state, anim, next_cursor, now));

    // While the envelope is live, push opacity every tick even with a
    // stationary cursor (only the visual layer's SetOpacity2 updates, so
    // redraw cost is zero).
    if let Some(cursor) = next_cursor
        && (cursor_moved || hud_envelope.is_live(now) || tier_changed)
    {
        effects.push(TickEffect::SetHudOpacity {
            state,
            cursor,
            envelope: hud_envelope.sample(now),
        });
    }
    let interval_ms = i64::try_from(telemetry_refresh.as_millis()).unwrap_or(i64::MAX);
    let interval_elapsed = input.now_ms.saturating_sub(world.last_hud_refresh_at_ms) >= interval_ms;
    // `rejected_this_tick` forces a refresh so the NotifyRejected toast shows
    // this tick instead of waiting out the telemetry interval. Effect order
    // matters: NotifyRejected was already pushed above, so the platform sees
    // it before this RefreshHud.
    let next_last_hud_refresh =
        if state_changed || tier_changed || interval_elapsed || rejected_this_tick {
            effects.push(TickEffect::RefreshHud {
                state,
                tier: hud_view.tier,
            });
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

    // Invariants: frame_seq advances by exactly 1, and last_hud_refresh_at_ms
    // is monotonic (except the i64::MIN sentinel).
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
) -> TickEffect {
    let master_now = anim.master.sample(now);
    match (state.mode, next_cursor) {
        (Mode::Horizontal | Mode::Vertical, Some(cursor)) => TickEffect::DrawOverlay {
            mode: state.mode,
            cursor,
            config: state.config,
            sample: anim.sample(now),
        },
        (Mode::Off, Some(cursor)) if master_now > 0 => TickEffect::DrawOverlay {
            mode: Mode::from(state.last_active),
            cursor,
            config: state.config,
            sample: anim.sample(now),
        },
        _ => TickEffect::ClearOverlay,
    }
}

/// Retargets each transition channel from the `prev → next` state diff.
/// Retarget re-bases from the current sample (`crate::anim`), so held-key
/// repeats landing mid-flight keep the value gliding continuously.
fn retarget_channels(
    mut anim: OverlayAnim,
    prev: State,
    next: State,
    now_ms: i64,
    cfg: AnimConfig,
) -> OverlayAnim {
    if next.mode != prev.mode {
        anim.master = match (prev.mode.active(), next.mode.active()) {
            // Off → active: fade in (re-bases mid-fade-out, so a quick double
            // toggle rises smoothly from wherever the fade-out got to).
            (None, Some(_)) => anim.master.retarget(now_ms, u8::MAX, cfg.overlay_fade_ms),
            // active → Off: fade out.
            (Some(_), None) => anim.master.retarget(now_ms, 0, cfg.overlay_fade_ms),
            // H ⇄ V: soft cut — the new axis fades in from 0. A hard reset
            // (not a retarget) because the axis being faded changed identity.
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
            // Flat ⇄ Blur: the brush kind flips Solid ⇄ Blur, which rebuilds
            // the renderer's sprite pool — a color crossfade cannot bridge
            // that. Ride the master envelope instead (the same soft cut as
            // H ⇄ V) and settle the style channel immediately.
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
    // `config.blur` (σ) deliberately has no transition channel: σ is baked
    // into the effect brush and is part of the sprite-pool signature, so
    // gliding it would rebuild the pool (and recompile the effect factory)
    // every tick. `BlurAmount` steps are perceptually uniform, so stepping is
    // already smooth enough.
    anim
}

#[cfg(test)]
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
            drained_hotkeys: Vec::new(),
        }
    }

    #[test]
    fn empty_tick_clears_and_refreshes_hud() {
        let (next, fx) = step(world(), &input(0), TELEMETRY, ANIM);
        assert_eq!(fx[0], TickEffect::ClearOverlay);
        assert!(matches!(fx.last(), Some(TickEffect::RefreshHud { .. })));
        assert_eq!(next.frame_seq, 1);
    }

    #[test]
    fn toggle_on_emits_log_and_draws_overlay() {
        let mut input = input(0);
        input.drained_hotkeys.push(OverlayAction::ToggleOnOff);
        input.polled_cursor = Some(Point::new(100, 100));
        let (next, fx) = step(world(), &input, TELEMETRY, ANIM);
        assert!(matches!(fx[0], TickEffect::LogStateChanged { .. }));
        assert!(
            fx.iter()
                .any(|e| matches!(e, TickEffect::DrawOverlay { .. }))
        );
        assert_eq!(next.state.mode, Mode::Horizontal);
    }

    #[test]
    fn quit_action_emits_quit_effect() {
        let mut input = input(0);
        input.drained_hotkeys.push(OverlayAction::Quit);
        let (_, fx) = step(world(), &input, TELEMETRY, ANIM);
        assert!(fx.contains(&TickEffect::Quit));
    }

    /// An adjust key while Off emits `NotifyRejected` and forces `RefreshHud`
    /// in the same tick even inside the telemetry interval (pins the
    /// `rejected_this_tick` clause). No state change, so no
    /// `LogStateChanged`.
    #[test]
    fn bump_while_off_emits_notify_rejected_and_forces_hud_refresh() {
        // Tick 1: consume the initial refresh so last_hud_refresh_at_ms is
        // current.
        let (w1, _) = step(world(), &input(0), TELEMETRY, ANIM);
        // Tick 2: BumpThickness while still Off, inside the interval (200ms).
        let mut i2 = input(50);
        i2.drained_hotkeys.push(OverlayAction::BumpThickness(8));
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
        on.drained_hotkeys.push(OverlayAction::ToggleOnOff);
        let (w1, _) = step(world(), &on, TELEMETRY, ANIM);
        // Drop opacity straight to MIN, then bump down again to saturate.
        let mut i2 = input(50);
        i2.drained_hotkeys
            .push(OverlayAction::BumpOpacity(-100_000));
        let (w2, _) = step(w1, &i2, TELEMETRY, ANIM);
        let mut i3 = input(100);
        i3.drained_hotkeys.push(OverlayAction::BumpOpacity(-8));
        let (_, fx) = step(w2, &i3, TELEMETRY, ANIM);
        assert!(
            !fx.iter()
                .any(|e| matches!(e, TickEffect::NotifyRejected { .. })),
            "saturation must stay silent, got {fx:?}"
        );
    }

    /// `ToggleOnOff` round trip: Off → Draw in the restored mode. Right after
    /// toggling back off the fade-out is still running, so it **still Draws**
    /// (along the `last_active` axis); Clear only comes on a tick after the
    /// fade completes.
    #[test]
    fn toggle_on_off_fades_out_then_clears() {
        let cursor = Some(Point::new(100, 100));
        let mut on = input(0);
        on.polled_cursor = cursor;
        on.drained_hotkeys.push(OverlayAction::ToggleOnOff);
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
        off.drained_hotkeys.push(OverlayAction::ToggleOnOff);
        let (w2, fx2) = step(w1, &off, TELEMETRY, ANIM);
        assert_eq!(w2.state.mode, Mode::Off);
        let draw = fx2.iter().find_map(|e| match e {
            TickEffect::DrawOverlay { mode, sample, .. } => Some((*mode, *sample)),
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
        on.drained_hotkeys.push(OverlayAction::ToggleOnOff);
        let (w1, fx1) = step(world(), &on, TELEMETRY, ANIM);
        let s1 = fx1
            .iter()
            .copied()
            .find_map(sample_of)
            .expect("draw on activation");
        assert_eq!(s1.master, 0, "fade-in starts from 0 at the trigger tick");

        let mut mid = input(i64::from(ANIM.overlay_fade_ms) / 2);
        mid.polled_cursor = cursor;
        let (w2, fx2) = step(w1, &mid, TELEMETRY, ANIM);
        let s2 = fx2
            .iter()
            .copied()
            .find_map(sample_of)
            .expect("draw mid-fade");
        assert!(
            s2.master > 0 && s2.master < 255,
            "mid-fade master must be between, got {}",
            s2.master
        );

        let mut done = input(i64::from(ANIM.overlay_fade_ms) + 1);
        done.polled_cursor = cursor;
        let (_, fx3) = step(w2, &done, TELEMETRY, ANIM);
        let s3 = fx3
            .iter()
            .copied()
            .find_map(sample_of)
            .expect("draw after fade");
        assert_eq!(s3.master, 255, "fade-in must land exactly at 255");
    }

    /// The thickness sample never regresses when the next bump lands
    /// mid-flight (pipeline-level check of the retarget glide-continuity
    /// guarantee).
    #[test]
    fn held_bumps_glide_thickness_without_regression() {
        let cursor = Some(Point::new(100, 100));
        let mut w = TickWorld::with_initial_state(State::with_mode(Mode::Horizontal));
        let mut last = 0_u16;
        let mut now = 0_i64;
        // Four +8 bumps at 50ms intervals → each lands mid-glide (130ms).
        for _ in 0..4 {
            let mut i = input(now);
            i.polled_cursor = cursor;
            i.drained_hotkeys.push(OverlayAction::BumpThickness(8));
            let (next, fx) = step(w, &i, TELEMETRY, ANIM);
            let s = fx
                .iter()
                .copied()
                .find_map(sample_of)
                .expect("active mode draws");
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
        let s = fx.iter().copied().find_map(sample_of).expect("draw");
        assert_eq!(s.thickness_px, 60);
    }

    /// After all transitions complete, the sample equals
    /// `OverlaySample::settled(config)` exactly (no steady-state drift).
    #[test]
    fn settled_sample_matches_config_after_transitions_complete() {
        let cursor = Some(Point::new(100, 100));
        let mut on = input(0);
        on.polled_cursor = cursor;
        on.drained_hotkeys.push(OverlayAction::ToggleOnOff);
        let (w1, _) = step(world(), &on, TELEMETRY, ANIM);
        let mut later = input(10_000);
        later.polled_cursor = cursor;
        let (_, fx) = step(w1, &later, TELEMETRY, ANIM);
        let s = fx.iter().copied().find_map(sample_of).expect("draw");
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

    /// Boots in full display (teaching period), auto-demotes to chip on the
    /// first tick after `startup_full_hud_ms` elapses, and the demotion tick
    /// emits `RefreshHud { tier: Chip }`.
    #[test]
    fn hud_boots_full_then_auto_demotes_to_chip() {
        let (w1, fx1) = step(world(), &input(0), TELEMETRY, ANIM);
        assert_eq!(
            fx1.iter().copied().find_map(refresh_tier_of),
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
        assert_eq!(w3.hud_view.tier, HudTier::Chip);
        assert_eq!(
            fx3.iter().copied().find_map(refresh_tier_of),
            Some(HudTier::Chip),
            "demotion tick must refresh in Chip tier, got {fx3:?}"
        );
    }

    /// `ToggleHudDetail` flips the tier and sets `user_touched`. From then
    /// on, no auto-demotion even past the teaching period (respects the
    /// user's choice).
    #[test]
    fn user_toggle_flips_tier_and_disables_auto_demote() {
        let (w1, _) = step(world(), &input(0), TELEMETRY, ANIM);

        // User collapses to Chip during the teaching period.
        let mut i2 = input(100);
        i2.drained_hotkeys.push(OverlayAction::ToggleHudDetail);
        let (w2, fx2) = step(w1, &i2, TELEMETRY, ANIM);
        assert_eq!(w2.hud_view.tier, HudTier::Chip);
        assert!(w2.hud_view.user_touched);
        assert_eq!(
            fx2.iter().copied().find_map(refresh_tier_of),
            Some(HudTier::Chip),
            "tier change must force a refresh, got {fx2:?}"
        );

        // Toggle again back to Full.
        let mut i3 = input(200);
        i3.drained_hotkeys.push(OverlayAction::ToggleHudDetail);
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
        i.drained_hotkeys.push(OverlayAction::ToggleHudDetail);
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

    fn envelope_of(fx: &[TickEffect]) -> Option<u8> {
        fx.iter().copied().find_map(|e| match e {
            TickEffect::SetHudOpacity { envelope, .. } => Some(envelope),
            _ => None,
        })
    }

    /// Pins the `SetHudOpacity` emit conditions: while the HUD envelope is
    /// live, emit every tick even with a stationary cursor; once settled,
    /// only on ticks where the cursor moved. (A `!=` → `==` mutation or an
    /// `is_live` mix-up cannot satisfy all the asserts at once.)
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

    /// On a chip ⇄ full swap tick, the envelope restarts from 0 (the new
    /// look fades in).
    #[test]
    fn tier_swap_restarts_hud_envelope_from_zero() {
        let p1 = Point::new(100, 100);
        let mut i1 = input(0);
        i1.polled_cursor = Some(p1);
        let (w1, _) = step(world(), &i1, TELEMETRY, ANIM);

        // Let the boot fade-in complete.
        let mut i2 = input(i64::from(ANIM.hud_swap_ms) + 10);
        i2.polled_cursor = Some(p1);
        let (w2, _) = step(w1, &i2, TELEMETRY, ANIM);

        // Toggle: the tier-swap tick emits even with a stationary cursor,
        // with value 0.
        let mut i3 = input(i64::from(ANIM.hud_swap_ms) + 100);
        i3.polled_cursor = Some(p1);
        i3.drained_hotkeys.push(OverlayAction::ToggleHudDetail);
        let (_, fx3) = step(w2, &i3, TELEMETRY, ANIM);
        assert_eq!(
            envelope_of(&fx3),
            Some(0),
            "tier swap must restart the envelope from 0, got {fx3:?}"
        );
    }
}
