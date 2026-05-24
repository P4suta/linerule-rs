# 0012 — ForegroundHook + HUD Telemetry: Phase J 後の cs port 漏れ機能補完

**Status:** Proposed (post Phase J / Phase η planning).

**See also:** [[0001-port-from-csharp]] (cs → rs port の前提)、[[0002-architecture-principles]] (§2 純粋性、§3 抽象の遅延、§8 単方向データフロー)、[[0003-unsafe-isolation]] (FFI 局所化方針)、[[0011-phase-j-slim-down]] (本 ADR が意図的に「外していない」ことを明示するため参照)。

## 文脈

linerule-rs は C# 版 linerule-cs の port として始まり、Phase A〜J を経て v0.3.0 を見込む状態に到達した。Phase J (ADR-0011) で AppData ログ / `dist-dev` profile / PDB 配布を slim-down したが、これは「凝った OS 統合の撤廃」が目的で、cs にあったコア UX 機能を削った訳ではない。

cs と rs の実コードを突き合わせると、以下 2 つの機能が「Phase J で外した訳ではなく、最初から port されていない」状態にある:

1. **ForegroundHook**: cs `ForegroundHook.cs` は `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` で前景アプリ変更を監視し、overlay の z-order を `HWND_TOPMOST` に再 assert する。`WS_EX_TOPMOST` 単独だと Alt+Tab や `SetForegroundWindow` 直後に他アプリが overlay より前に出るケースがある。
2. **HUD Telemetry**: cs `HudRenderer.cs::408` は `"{Hz}Hz · p99 {ms:F2}ms · drops {} · stalls {}"` 形式で frame timing を HUD 右上に表示する。rs では `DisplayHz` (`refresh_hz`) だけが `hud_frame()` 引数として既に流れているが、p99 tick latency / dropped frames / commit timeouts の 3 指標は計測経路自体が無い。

両機能とも portable exe-dir 直書きや AppData 書き込み等の OS 統合を導入しないので、ADR-0011 の「薄い読書ツール」doctrine と矛盾しない。

## 判断

### 1. ForegroundHook を追加 (PR 3)

`WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS` で `SetWinEventHook` を仕掛け、callback は OS hook thread から `PostMessageW(hwnd, WM_APP_REASSERT_TOPMOST, 0, 0)` のみ実行する。実 `SetWindowPos(HWND_TOPMOST, ..., SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE)` は UI thread の wndproc が処理する。

- `WINEVENT_SKIPOWNPROCESS` 指定で OS が自プロセス前景化を抑制してくれるため、callback 側で HWND 比較する必要が無い (cs `ForegroundHook.cs:42` と同戦略)。
- `unsafe` は `crates/linerule-platform-windows/src/win32_ffi/accessibility.rs` に集約 (ADR-0003)。`crates/linerule-platform-windows/src/foreground_hook.rs` 本体は `#![forbid(unsafe_code)]` を維持。
- callback は `catch_unwind(AssertUnwindSafe(...))` で panic を OS thread に漏らさない (`win32_ffi/core.rs::overlay_wnd_proc` 同様)。
- callback と UI thread 間で HWND を共有するために `AtomicIsize` (HWND は `!Send`) を使う。既存 `render_clock.rs:43` の HWND 越境パターンと同じ。
- 失敗 (`SetWinEventHook` → null) は fatal にせず log 出力のみ。`WS_EX_TOPMOST` 単独でも多くのケースで動作する。

### 2. HUD Telemetry を追加 (PR 4)

cs フォーマット `"{Hz}Hz · p99 {ms:F2}ms · drops {} · stalls {}"` と byte-for-byte 一致で 3 指標 (`tick_p99_ms`, `frames_dropped`, `commit_timeouts`) を HUD に表示する。

検討した経路案:
- **(a) `OverlayConfig` に同梱**: 不適。`OverlayConfig::DEFAULT` は `const` / `Eq` / `Hash` の決定論的データ doctrine ([[0011]]) を持ち、per-tick の非決定 metric を混ぜると意味論が壊れる。ADR-0002 §2 (core は副作用とグローバル状態を持たない、非決定性は関数引数で渡す) にも反する。
- **(b) `render::frame()` シグネチャ拡張**: 不適。`render::frame` は slit + indicator 専用で HUD テキストと無関係、責務不一致。
- **(c) `hud_frame()` 引数追加**: 採用。既に `refresh_hz: u32` と `notifications: &[HudNotification]` を引数で受けている既存パターンの忠実な拡張。ADR-0002 §3 (抽象の遅延 — 新 trait や Builder を作らない) と §8 (単方向データフロー) を満たす。

実装:
- `HudTelemetry { tick_p99_ms: f32, frames_dropped: u64, commit_timeouts: u64 }` を `linerule-core::render::hud_frame` に純粋 ADT として置く (`HudNotification` の隣)。
- `FrameTimingTracker` は `linerule-platform-windows::frame_timing` に局所化された純粋ロジック (固定窓 256 サンプルで p99 算出)。`Instant::now` は引数化して clock を mock 可能に。
- `wndproc::apply_tick` の先頭で `begin_tick()`、末尾で `end_tick(over_budget)` (over_budget = `RenderConfig::warn_ratio * (1000 / refresh_hz)` 比較)。`RefreshHud(s)` effect で `snapshot()` を取り `hud_frame(..., telemetry)` に渡す。
- `composition_renderer::apply` の `graphics::commit` 失敗 (timeout) を caller に伝え、wndproc 側で `record_timeout()`。

`linerule-core` は依然として platform 側 mechanism を知らない (`cargo xtask dep-graph` で確認)。

## 結果

- `crates/linerule-platform-windows/src/foreground_hook.rs` 新規 (RAII guard) + `win32_ffi/accessibility.rs` 新規 (FFI 集約)
- `messages.rs` に `WM_APP_REASSERT_TOPMOST = 0x8003` 追加
- `wndproc.rs::dispatch` に `WM_APP_REASSERT_TOPMOST` arm 追加
- `linerule-app/src/boot.rs` で `ForegroundHook::install(overlay.hwnd())` を `RenderClock::spawn` より先に呼び、Drop 逆順で安全に解除
- `crates/linerule-platform-windows/src/frame_timing.rs` 新規 (PR 4)
- `crates/linerule-core/src/render/hud_frame.rs` に `HudTelemetry` ADT 追加 + `hud_frame()` 引数拡張 (PR 4)
- `crates/linerule-platform-windows/src/overlay_state.rs` に `FrameTimingTracker` field 追加 (PR 4)

## 検討した代替案

### A. ForegroundHook を polling で代替

却下: heartbeat に poll を生やすと `EVENT_SYSTEM_FOREGROUND` 取得のオーバーヘッドが render budget に乗る。SetWinEventHook は OS が edge-triggered で通知してくれるので、tail latency が安定する。

### B. Telemetry を `linerule-core` の `State` フィールドに足す

却下: `State` は決定論的 reducer の input/output で、`Serialize` / `Eq` を持つ。非決定 metric を混ぜると `apply` の純粋性が壊れる ([[0002]] §2)。

### C. cs と同じく HUD Telemetry を core の `Render` 内に組み込む

却下: rs では `render::frame` (slit/indicator) と `hud_frame()` (HUD panel) を分離している。telemetry は HUD 側だけの責務なので、(c) で十分。

## 関連

- [[0001-port-from-csharp]]: cs port 計画。本 ADR はその port を機能 parity の方向で完結させる。
- [[0011-phase-j-slim-down]]: 本 ADR が削った機能の補完ではないこと、portable doctrine と整合することを明示。
- ADR-0013 (planned): Phase H PR-E。`AppError::class()` 消費 + HUD notification toast push。本 ADR の HUD frame 拡張と独立。
