# ADR 0002 — アーキテクチャ原則

- 日付: 2026-05-20
- ステータス: accepted
- 提案者: P4suta

## 文脈

`linerule-rs` の最重要事項は **アーキテクチャの美しさ** である ([[0001-port-from-csharp]])。
動けば良い・移植網羅性が高ければ良い、ではなく、モダン・スマート・クリーンな実装を
優先する。本 ADR は「美しさ」を機械的に検証可能な原則に分解する。

## 決定

### 原則 1 — 一方向依存

依存方向は `linerule-app → linerule-platform-windows → linerule-core` の一方向に
限定する。逆引きや peer-to-peer 依存は禁止。

検証: `cargo xtask dep-graph` が `cargo metadata` を解析して違反を検出する。CI 必須ゲート。

### 原則 2 — クレートごとの不変条件

| クレート | 役割 | クレート属性 | 持たないもの |
|---|---|---|---|
| `linerule-core` | 純粋ロジック、ADT、reducer、parser | `#![forbid(unsafe_code)]` | 副作用、`windows` クレート、`std::env`/`std::fs`、グローバル状態 |
| `linerule-platform-windows` | Win32 / COM 実装 | `#![cfg(windows)] #![deny(unsafe_op_in_unsafe_fn)]` | ドメインロジック (reducer/render は core から呼ぶだけ) |
| `linerule-app` | 結線とエントリーポイント | `#![forbid(unsafe_code)]` (`console` のみ局所例外) | ドメインロジック |
| `xtask` | ビルド自動化 | `#![forbid(unsafe_code)]` | 本番ビルドへの影響 |

`std::time::Instant` などの非決定性は `linerule-core` の関数引数として外から渡し、
core 内では一切のグローバルアクセスを行わない。

### 原則 3 — 抽象の遅延

trait / generic を予防的に作らない。**最低 2 実装が現れてから** 抽象に昇格する。
`IOverlaySurface` / `IHotkeyHost` / `IMouseTracker` 風の port-and-adapter trait は
作らない (`linerule-cs` の `Linerule.Platform` の経験から、テスト用 mock は trait
よりも `cfg(test)` の別実装か closure 注入で書く方が読みやすい)。

### 原則 4 — RAII 徹底

COM オブジェクト / HWND / Hook / Hotkey / JoinHandle はすべて `Drop` で確実に解放する。
`ComLifetime` 手動 Release、`ManuallyDrop`、`std::mem::forget`、`Box::leak` の
プロダクション利用を禁止。

### 原則 5 — Exhaustive match

状態遷移とコマンド処理は `match` の exhaustive 性に依拠する。`_ => …` をプロダクション
コードで使わない (test fixture のみ許容)。新ケース追加時にコンパイラが網羅性違反を
検出する状態を保つ。

検証: `xtask strict-code` の `no-wildcard-match` ルール。

### 原則 6 — `Result + ?` の自然な flow

`BootDag<Phase<TIn,TOut>>` のような独自 monad / Kleisli 機構を持ち込まない。順序は
コンパイラ強制 (関数本文の行順) で十分。エラー型は `thiserror::Error` で構造化する。

### 原則 7 — `unsafe` の局所化

`unsafe` ブロックは可能な限り狭く、ブロックの直前にコメントで invariants を明示する。
`#![deny(unsafe_op_in_unsafe_fn)]` により `unsafe fn` 内でも明示を要求する。

ファイル冒頭の `#![allow(unsafe_code)]` は禁止 (`xtask strict-code` の
`no-file-wide-unsafe-allow` で検出)。`linerule-platform-windows` のみ
`#![allow(unsafe_code)]` をクレート属性で許可するが、これは `#![cfg(windows)]` の
直下に限定する。

### 原則 8 — データ駆動 + 単方向

`OverlayAction → State + StateDelta → OverlayFrame → render` の単方向。state mutation は
`StateDelta::apply` の一点に集約する。直接 `state.field = x` を散在させない。

### 原則 9 — WndProc instance 結合

`SetWindowLongPtr(GWLP_USERDATA)` で HWND ↔ 構造体ポインタを結合する。`static mut`、
`OnceLock<Mutex<Option<Box<dyn Fn>>>>` 風の静的ディスパッチャは禁止 (テスト時に
複数 HWND が干渉する経路を作らない)。

検証: `xtask strict-code` の `no-static-mut` ルール、および設計レビュー。

### 原則 10 — `#[allow]` の最小局所化

`#[allow]` はファイル冒頭ではなく属性付与対象の直前に書く。広域 `#[allow(clippy::all)]`
や `#[allow(warnings)]` は禁止。

検証: `xtask strict-code` の `no-broad-allow-clippy` / `no-broad-allow-warnings` ルール。

### 原則 11 — Boundary 限定の panic

`unwrap()` / `expect()` は boundary 限定 (`main.rs`、`xtask` クレート、`#[cfg(test)]`)。
それ以外では `?` + `thiserror` で構造化エラーを返す。clippy `unwrap_used = deny`、
`expect_used = warn` を workspace lints で強制し、`xtask strict-code` の
`no-unwrap-outside-boundary` ルールで補完する。

### 原則 12 — `mod.rs` 禁止

`mod.rs` を使わず `module_name.rs` + `module_name/` の 2018+ 標準形に統一する。
clippy `mod_module_files = deny` を workspace lints で強制。

### 原則 13 — ワイルドカード import 禁止

`use foo::*` を禁止。clippy `wildcard_imports = deny` を workspace lints で強制。
prelude モジュールを作らない (例外: `linerule_core::prelude` を作る場合は本 ADR を
更新)。

## 結果

- `cargo xtask strict-code` と `cargo xtask dep-graph` が CI 必須ゲートとなる
- 新規モジュール追加時に原則を見直す
- 原則の追加・削除・変更は ADR 改訂を必須とする (本 ADR のメンテナンスが原則一覧の
  source of truth)
- C# 由来の dead pattern (`ComLifetime`、`IPathfulSink`、`Phase<TIn,TOut>`、`static mut`)
  を翻訳せず、Rust の慣行で書き直す
