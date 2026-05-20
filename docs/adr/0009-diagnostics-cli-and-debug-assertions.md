# 0009 — `linerule diagnostics` CLI 拡張と `debug_assertions` 哨戒

**Status:** Accepted (Phase H, 2026-05-20).

**See also:** [[0004-coverage-policy]], [[0007-debug-build-and-panic-strategy]], [[0008-error-class-and-app-aggregator]]、Phase H plan の PR-E2。

## 文脈

Phase H の前半 PR-A〜E1 までで:

- `crash-<run_id>-*.json` に panic location + backtrace + `recent_events` tail (PR-D #52) が同梱されるようになり、
- `events.jsonl` に `run_id` span field (PR-B #49) が乗るようになった。

しかし開発者が「最新の crash を見たい」「直近の event tail を grep したい」とするとき、いまだに `%APPDATA%\linerule\` を手で開いて `jq` でパースする手数が要る。`just crash-latest` / `just logs-pretty` の helper はあるが、これは Windows host から呼ぶには WSL 経由などで不便。

加えて `linerule-rs` には `#[cfg(debug_assertions)]` の使用が **ゼロ件** だった。Debug Build profile (`dist-dev`, PR-A #48) を `panic = "unwind"` にして `catch_unwind` 経路を実機検証可能にしたのに、その debug build で強化される invariant チェックが何もない。

## 判断

### 1. `linerule diagnostics` に 4 つの flag を追加

```rust
Diagnostics {
    #[arg(long)] dry_run: bool,                          // 既存 (data_dir 列挙のみ)
    #[arg(long)] last_crash: bool,                       // 新規
    #[arg(long, value_name = "N")] recent_events: Option<usize>,  // 新規
    #[arg(long)] data_dir: bool,                         // 新規
}
```

意味論:

- **`--data-dir`**: `%APPDATA%\linerule\` の絶対 path を **1 行だけ stdout に書く**。script から `linerule diagnostics --data-dir | xargs -I {} ls {}` のように pipe で繋ぐ用途。
- **`--last-crash`**: 最新 `crash-*.json` (mtime 最大) を `serde_json::Value` 経由で pretty-print。Windows host 単体で grep が完結する。
- **`--recent-events N`**: 最新 `events.jsonl.<today>` の末尾 `N` 行を 1 行ずつ JSON pretty-print。`just logs-tail` の Windows host 版。
- **`--dry-run`**: 既存挙動 (data dir 列挙のみ、I/O write なし) を明示的にドキュメント化。

これらは **互いに排他ではない** が、CLI フローでは `--data-dir → --last-crash → --recent-events → default` の優先順で 1 つだけ実行する (シンプル化のため)。複合表示が必要なら shell で 3 回呼ぶ。

### 2. `#[cfg(debug_assertions)]` 哨戒を 2 箇所追加

| 場所 | 不変条件 | 違反時の挙動 |
|---|---|---|
| `OverlayWndState::record_hotkey` | 同 id を二重 register しない | `debug_assert!(prev.is_none(), ...)` で即 panic |
| `tick::step` の return 前 | `next_world.frame_seq == prev.wrapping_add(1)` | `debug_assert!(...)` |
| 同上 | `last_hud_refresh_at_ms` 単調増加 (初回 `i64::MIN` 除く) | `debug_assert!(...)` |

選別根拠:

- **採用**: 上 3 つは「invariant に違反したら必ずバグ」かつ runtime check のコストが小さい (HashMap::insert の戻り値、u64/i64 比較)。
- **不採用**: `RefCell::borrow_mut` の重複借用は既に runtime panic になる (RefCell の標準挙動)。重複検出は冗長。
- **不採用**: `OverlayAction` の `id_to_action` lookup と `registered_hotkey_ids` の整合性は record_hotkey の不変性から自動的に保たれる (二重 register 検出で十分)。

`debug_assert!` は release build (= profile.release) で完全に消える (zero runtime cost)。Debug Build (`dist-dev`) と dev profile では fire し、開発者がローカルで気づける。`catch_unwind` 経路下 (overlay_wnd_proc) では、これらの panic は visual 一瞬欠ける程度の影響に閉じる (ADR-0007)。

## 結果

- `linerule-app/src/cli.rs`: `Command::Diagnostics` に 3 つの新 flag (`last_crash`, `recent_events`, `data_dir`) を追加 (~30 LOC + 6 unit tests)
- `linerule-app/src/boot.rs`: `DiagnosticsArgs` struct + `print_last_crash` / `print_recent_events` 実装 (~130 LOC)
- `linerule-platform-windows/src/overlay_state.rs::record_hotkey`: `debug_assert!` 追加 (~5 LOC)
- `linerule-core/src/input/tick.rs::step`: `debug_assert!` 2 件追加 (~15 LOC)
- Phase H の PR-E2 スコープ

`linerule.exe diagnostics --last-crash` / `--recent-events 20` / `--data-dir` がローカルで使えるようになり、`just crash-latest` / `just logs-tail` の Windows host 等価機能が CLI に内製化される。

## 検討した代替案

### A. クラッシュレポート用の専用サブコマンド `linerule crash` を追加

却下: 既存の `diagnostics` を拡張する方が discoverability が高い (`linerule diagnostics --help` で全部見える)。サブコマンドが分散するより flag で揃える方が clap の典型パターン。

### B. JSON Lines を tail せず ring buffer を永続化

却下: PR-D の `event_ring` は in-memory ring buffer で、プロセス再起動で消える。永続化は別 PR の責務 (将来 `event_ring::flush_to_file` を生やす余地)。`--recent-events` は当面 `events.jsonl.<today>` を tail する方が確実。

### C. `debug_assert!` の代わりに `tracing::error!` で記録するだけ

却下: invariant 違反は「観測したい」よりも「気づきたい」状態。release では消えるが、debug build では即 panic させる方が CI / 実機検証で確実に拾える。

## 関連

- ADR-0004 — coverage gate は `linerule-core` + `linerule-app` のみ。`tick::step` の `debug_assert!` は core 経由で coverage 対象に乗る
- ADR-0007 — Debug Build profile (`dist-dev`) で `panic = "unwind"` のため `debug_assert!` panic も catch_unwind で吸収可能
- ADR-0008 — `ErrorClass::ProgrammerError` と `debug_assert!` の対応 (`Opacity::try_new(0)` 等の不正入力は `ProgrammerError` クラス、debug build で先回り検知)
- `linerule-rs-version-bump-cautious` — 本変更は `fix(app):` で patch bump
