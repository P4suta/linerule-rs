# 0012 — ForegroundHook + HUD Telemetry

**Status:** Accepted.

**See also:** [[0002-architecture-principles]], [[0003-unsafe-isolation]].

## Context

1. **ForegroundHook**: `WS_EX_TOPMOST` alone lets another app jump in front of the overlay right after Alt+Tab or `SetForegroundWindow`. We must watch foreground changes and re-assert z-order.
2. **HUD Telemetry**: no path to measure frame timing metrics (p99 tick latency / dropped frames / commit timeouts).

## Decision

### ForegroundHook

`SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` with `WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS`. The callback runs on the OS hook thread and only does `PostMessageW(hwnd, WM_APP_REASSERT_TOPMOST, 0, 0)`. The actual `SetWindowPos(HWND_TOPMOST, ..., SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE)` is handled by the UI thread wndproc.

- `WINEVENT_SKIPOWNPROCESS` makes the OS suppress our own foreground events → no HWND comparison needed in the callback.
- `unsafe` is concentrated in `win32_ffi/accessibility.rs`. The `foreground_hook.rs` body itself is `#![forbid(unsafe_code)]`.
- The callback uses `catch_unwind` so a panic never leaks to the OS thread. HWND is `!Send`, so it is shared via `AtomicIsize`.
- `SetWinEventHook` failure is logged only, not fatal.

### HUD Telemetry

3 metrics (`tick_p99_ms`, `frames_dropped`, `commit_timeouts`). Mixing non-deterministic metrics into `OverlayConfig` or `render::frame()` would break determinism / separation of concerns, so — like `refresh_hz` / `notifications` — they are taken as **arguments** to `hud_frame()`.

- `HudTelemetry { tick_p99_ms: f32, frames_dropped: u64, commit_timeouts: u64 }` lives in `linerule-core::render::hud_frame` as a pure ADT.
- `FrameTimingTracker` is localized in `linerule-platform-windows::frame_timing` (p99 over a fixed 256-sample window). `Instant::now` is parameterized so the clock can be mocked.
- `begin_tick()` at the start of `wndproc::apply_tick`, `end_tick(over_budget)` at the end. The `RefreshHud(s)` effect passes `snapshot()` into `hud_frame(..., telemetry)`.
- `composition_renderer::apply` propagates `graphics::commit` failure to the caller, and the wndproc side calls `record_timeout()`.

`linerule-core` does not know about the platform-side mechanism (verified with `cargo xtask dep-graph`).

## Consequences

- New `foreground_hook.rs` (RAII guard) + `win32_ffi/accessibility.rs` (FFI concentration)
- Add `WM_APP_REASSERT_TOPMOST = 0x8003` to `messages.rs`, add an arm to `wndproc.rs::dispatch`
- In `boot.rs`, call `ForegroundHook::install` before `RenderClock::spawn`; release in reverse Drop order
- New `frame_timing.rs`, `HudTelemetry` ADT + argument extension in `hud_frame.rs`, add a `FrameTimingTracker` field to `overlay_state.rs`
