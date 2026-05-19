# linerule-rs

Rust 製の Windows 用 reading ruler（読書補助オーバーレイ）。透明 click-through ウィンドウで画面上に水平／垂直のスリットを表示し、視線追跡を助ける。

> ⚠️ 開発中。Phase B.5（品質統合・美磨き）完了時点。Phase C（OverlayWindow + メッセージポンプ）以降は未実装。

## 構成

| Crate | 役割 |
|---|---|
| `linerule-core` | 純粋ロジック層。ADT / reducer / render / chord parser / hold FSM / tick pipeline。`#![forbid(unsafe_code)]` |
| `linerule-platform-windows` | Win32 / COM 実装層。DirectComposition + Direct2D + DXGI + D3D11 を直接叩く。`#![cfg(windows)]` |
| `linerule-app` | 単一バイナリ `linerule.exe` のエントリポイント。`windows_subsystem = "windows"` + サブコマンドで GUI / CLI 切替 |
| `xtask` | ビルド自動化。`lint` / `dep-graph` / `ci` |

依存方向は一方向: `linerule-app → linerule-platform-windows → linerule-core`。`cargo xtask dep-graph` で機械検証する。

## クイックスタート

### 開発環境

ホストには Docker さえあれば良い (`docker compose`)。すべてのツール (`cargo`, `cargo-xwin`, `cargo-deny`, `cargo-nextest`, `cargo-machete`, `cargo-llvm-cov`, `cargo-audit`, `just`, `typos`, `lefthook`, `actionlint`, `commitlint`) はコンテナ内に揃う。

```bash
just docker-build   # 初回 + Dockerfile 変更時
just hooks          # lefthook + commitlint をインストール
```

### ビルド・テスト・リント

```bash
just build          # cargo build --workspace --all-targets
just test           # cargo nextest run --workspace
just lint           # fmt + clippy + cargo-deny + typos + actionlint + cargo-machete + dep-graph
just run            # cargo run -p linerule-app (Windows host のみ動作)
```

### クロスコンパイル確認

Windows ターゲットの型／構文ドリフトを Linux 上で検出するために `cargo-xwin` を使う:

```bash
just cross-check        # cargo xwin check --target x86_64-pc-windows-msvc --workspace
just publish-windows-cross  # 反復用のクロスビルド (shippable ではない)
```

shippable な `linerule.exe` は CI の windows-latest runner からのみ produce される（ABI / SEH 事故回避のため）。

### ログとクラッシュダンプ

ランタイム時に `%APPDATA%\linerule\events.jsonl.YYYY-MM-DD` へ tracing JSON Lines を流す。

```bash
just logs-tail subsystem=wnd_proc  # subsystem フィルタ
just logs-pretty                    # 全件 pretty-print
just crash-list                     # クラッシュダンプ一覧
just crash-latest                   # 最新クラッシュダンプ
```

## Library API overview

下のブロックは `crates/linerule-core/src/lib.rs` の crate-level doc から `cargo rdme` で自動同期されます。手書きで中身を編集しないこと（`just docs` で再生成）。

<!-- cargo-rdme start -->

<!-- cargo-rdme end -->

## モジュールツリー・依存グラフ

- [`docs/modules/`](docs/modules/) — 各クレートの `cargo modules structure` 出力（自動生成）
- [`docs/dep-graph.svg`](docs/dep-graph.svg) — workspace 依存グラフ（`cargo depgraph` 自動生成）

更新は `just docs` で一括実行。`lefthook` の pre-commit が drift を検出し、生成物が古いまま commit されることを防ぐ。

## 設計・運用ドキュメント

- [`docs/adr/0001-port-from-csharp.md`](docs/adr/0001-port-from-csharp.md): 旧 C# 版 (`linerule-cs`) からの Rust 全面リライト判断、旧 ADR 処遇マッピング
- [`docs/adr/0002-architecture-principles.md`](docs/adr/0002-architecture-principles.md): 18 個の merge ブロッカー原則（一方向依存 / RAII / exhaustive match / unsafe 局所化 / `#[non_exhaustive]` を使わない / 等）

## ライセンス

このプロジェクトは MIT または Apache-2.0 のいずれかでデュアルライセンスされます (お好みで)。

- [`LICENSE-MIT`](LICENSE-MIT)
- [`LICENSE-APACHE`](LICENSE-APACHE)
