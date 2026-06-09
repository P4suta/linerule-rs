# 0012 — ForegroundHook + HUD Telemetry

**Status:** Accepted.

**See also:** [[0002-architecture-principles]] (純粋性 / 抽象の遅延 / 単方向データフロー)、[[0003-unsafe-isolation]] (FFI 局所化)。

## 文脈

2 つの機能が未実装だった:

1. **ForegroundHook**: `WS_EX_TOPMOST` 単独だと Alt+Tab や `SetForegroundWindow` 直後に他アプリが overlay より前に出る。前景アプリ変更を監視して z-order を再 assert する必要がある。
2. **HUD Telemetry**: frame timing 指標 (p99 tick latency / dropped frames / commit timeouts) の計測経路が無い。`refresh_hz` だけは既に `hud_frame()` に流れている。

## 判断

### ForegroundHook

`WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS` で `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` を仕掛け、callback は OS hook thread から `PostMessageW(hwnd, WM_APP_REASSERT_TOPMOST, 0, 0)` のみ。実 `SetWindowPos(HWND_TOPMOST, ..., SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE)` は UI thread の wndproc が処理する。

- `WINEVENT_SKIPOWNPROCESS` で自プロセス前景化は OS が抑制するので callback 側で HWND 比較不要。
- `unsafe` は `win32_ffi/accessibility.rs` に集約。`foreground_hook.rs` 本体は `#![forbid(unsafe_code)]`。
- callback は `catch_unwind` で panic を OS thread に漏らさない。HWND は `!Send` なので `AtomicIsize` で共有。
- `SetWinEventHook` 失敗は fatal にせず log のみ。

### HUD Telemetry

3 指標 (`tick_p99_ms`, `frames_dropped`, `commit_timeouts`) を HUD に表示する。非決定 metric を `OverlayConfig` や `render::frame()` に混ぜると決定論性 / 責務が壊れるため、既存の `refresh_hz` / `notifications` と同じく `hud_frame()` の**引数**で受ける。

- `HudTelemetry { tick_p99_ms: f32, frames_dropped: u64, commit_timeouts: u64 }` を `linerule-core::render::hud_frame` に純粋 ADT として置く。
- `FrameTimingTracker` は `linerule-platform-windows::frame_timing` に局所化 (固定窓 256 サンプルで p99 算出)。`Instant::now` は引数化して clock を mock 可能に。
- `wndproc::apply_tick` の先頭で `begin_tick()`、末尾で `end_tick(over_budget)`。`RefreshHud(s)` effect で `snapshot()` を `hud_frame(..., telemetry)` に渡す。
- `composition_renderer::apply` の `graphics::commit` 失敗を caller に伝え、wndproc 側で `record_timeout()`。

`linerule-core` は platform 側 mechanism を知らないまま (`cargo xtask dep-graph` で確認)。

## 結果

- `foreground_hook.rs` (RAII guard) + `win32_ffi/accessibility.rs` (FFI 集約) 新規
- `messages.rs` に `WM_APP_REASSERT_TOPMOST = 0x8003`、`wndproc.rs::dispatch` に arm 追加
- `boot.rs` で `ForegroundHook::install` を `RenderClock::spawn` より先に呼び、Drop 逆順で解除
- `frame_timing.rs` 新規、`hud_frame.rs` に `HudTelemetry` ADT + 引数拡張、`overlay_state.rs` に `FrameTimingTracker` field 追加
