# 0013 — AppError::class() を消費して Recoverable を HUD notification に出す (Phase H PR-E)

**Status:** Proposed (Phase η planning).

**See also:** [[0008-error-class-and-app-aggregator]] (`ErrorClass` / `AppError` の型導入、本 ADR がその実消費を担当)、[[0011-phase-j-slim-down]] (本変更が「凝った OS 統合」ではなく既存基盤の活用であることを明示)、[[0012-foreground-hook-and-hud-telemetry]] (HUD frame 拡張は別 PR、本 ADR は notification 経路に focus)。

## 文脈

ADR-0008 (Phase H PR-C) で以下を導入した:

- `linerule-core::ErrorClass { Recoverable, Fatal, ProgrammerError }`
- 各 error 型 (`CoreError` / `ChordError` / `LineruleError` / `PlatformError`) に `class()` method
- `linerule-app::AppError` aggregator + `class()` method

ただし `AppError` と `AppError::class()` は両方とも:

```rust
#[allow(
    dead_code,
    reason = "PR-E (HUD notification toast push) で消費する予定の aggregator 型..."
)]
```

の状態で凍結されていた。**Phase H PR-E** は当初 plan に書かれていたが Phase J slim-down (ADR-0011) 後も未着手のまま、計 3 箇所の `dead_code` allow を生んでいた:

1. `linerule-app/src/error.rs:32-36` — `AppError` enum 自体
2. `linerule-app/src/error.rs:57-61` — `AppError::class()` method
3. `linerule-app/src/logging.rs:83-89` — `tracing_subscriber::fmt::Subscriber` の future-use 抑制
4. `linerule-platform-windows/src/overlay_state.rs:318-321` — `ChordSpec` import の future-use 抑制 (Phase H PR-E と同じ HUD 表示拡張に伴うもの)

すべて Phase H PR-E が終わっていない結果として残った技術的負債。

## 判断

**`AppError::class()` を `boot::run_overlay` のエラーパスで実消費し、`Recoverable` 判定された早期エラーは HUD notification として toast push する。dead_code allow 4 箇所はすべて解消する。**

### 1. `classify_and_log(err: &AppError) -> RunDecision`

`linerule-app/src/error.rs` に classify helper を追加:

```rust
pub(crate) fn classify_and_log(err: &AppError) -> RunDecision {
    match err.class() {
        ErrorClass::Recoverable => {
            tracing::warn!(...);
            RunDecision::Continue
        }
        ErrorClass::Fatal => {
            tracing::error!(...);
            RunDecision::Stop
        }
        ErrorClass::ProgrammerError => {
            tracing::error!(...);
            debug_assert!(false, ...);
            RunDecision::Stop
        }
    }
}
```

- `Recoverable` は呼び出し側で HUD push を行い処理を続行する。
- `Fatal` は `?` で main へ bubble up し crash dump 経路に乗る。
- `ProgrammerError` は debug build で `debug_assert!` を発火し、release では Fatal 同等に扱う (ADR-0009 と整合)。

### 2. `boot::run_overlay` での消費経路

`SetProcessDpiAwarenessContext` 失敗のような早期エラーは:

```rust
if let Err(e) = set_dpi_aware() {
    let app_err: AppError = e.into();
    if classify_and_log(&app_err) == RunDecision::Continue {
        early_recoverable.push(format!("DPI awareness: {app_err}"));
    }
}
// ...
overlay.attach_dcomp()?;
overlay.register_hotkeys(...)?;
for msg in early_recoverable.drain(..) {
    overlay.state().push_notification(NotificationClass::Warn, msg, 10_000);
}
```

`OverlayWindow` ハンドルが存在する文脈 (= dcomp attach 後) で HUD push する。HUD は既に `HudNotification` + `OverlayWndState::push_notification` + `wndproc::build_notifications` 経路が完備で、本変更で新規 platform API は不要。

### 3. dead_code allow の整理

| ファイル | 解消方法 |
|---|---|
| `linerule-app/src/error.rs:32` | `AppError` を `classify_and_log` が実消費するため、allow を削除 |
| `linerule-app/src/error.rs:57` | 同上 |
| `linerule-app/src/logging.rs:83` | future-use の `Subscriber` import は実需要が出るまで撤去 (再追加は trivial) |
| `linerule-platform-windows/src/overlay_state.rs:318` | `ChordSpec` の HUD 表示拡張は別 issue とし、import 自体を撤去 |

ChordSpec import は `chord::parse(spec).map(|c| c.to_string())` のような canonical display 化に使う計画だったが、Phase H PR-E の本筋ではないので別 PR (`HotkeyConflict` の表示強化) に分離する。

## 結果

- `crates/linerule-app/src/error.rs` — dead_code allow 2 件削除、`classify_and_log` + `RunDecision` 追加
- `crates/linerule-app/src/boot.rs` — `set_dpi_aware()` 失敗時に `classify_and_log` 経由で recoverable error を `early_recoverable` に蓄積、`overlay.attach_dcomp()` 後に HUD notification として一括 push
- `crates/linerule-app/src/logging.rs` — `Subscriber` import + dead_code allow + `const _` ブロック削除
- `crates/linerule-platform-windows/src/overlay_state.rs` — `ChordSpec` import + dead_code allow + `const _` ブロック削除

## 検討した代替案

### A. AppError を消費せず log 出力に class を文字列で混ぜるだけ

却下: `AppError::class()` が compile-time に dead-code のまま残る。`#[allow(dead_code)]` を維持することになる。

### B. HUD push を ADR-0008 で同梱

却下: Phase H PR-C の責務分離 (型導入と消費の分離) に反する。PR-E が「実消費」のために独立して計画されていたことを尊重する。

### C. ChordSpec を `HotkeyConflict` に parsed として格納

却下: 本 PR の本筋 (`AppError::class()` 消費) と直交。`HotkeyConflict` の表示改善は別 issue で扱う方が変更が小さい。

## 関連

- [[0008-error-class-and-app-aggregator]]: 本 ADR が完成形を与える先行 ADR。
- [[0009-diagnostics-cli-and-debug-assertions]]: `ProgrammerError` の debug build 挙動 (`debug_assert!`) と整合。
- [[0011-phase-j-slim-down]]: 本変更は portable doctrine に反しない (HUD 既存基盤を活用するだけ)。
- [[0012-foreground-hook-and-hud-telemetry]]: HUD frame 拡張と独立。同 ADR の `HudTelemetry` row と並列に notification rows が表示される。
