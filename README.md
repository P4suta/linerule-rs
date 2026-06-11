# linerule-rs

Rust 製の Windows 用 reading ruler（読書補助オーバーレイ）。透明 click-through ウィンドウで画面上に水平／垂直のスリットを表示し、視線追跡を助ける。

実機での操作:

- `Ctrl+Alt+R`: モード切替（Off → Horizontal → Vertical → Off）
- `Ctrl+Alt+H`: ON/OFF トグル（Off ⇄ 直前のアクティブモード。最後に使った向きを覚えている）
- `Ctrl+Alt+Up` / `Ctrl+Alt+Down`: スリット厚さ ±（長押しで連続調整）
- `Ctrl+Alt+Right` / `Ctrl+Alt+Left`: 不透明度 ±（長押しで連続調整）
- `Ctrl+Alt+S`: 周囲スタイル切替（Dim 暗転 → Bright 真っ白 → Dim）
- `Ctrl+Alt+K`: HUD 表示切替（常駐チップ ⇄ ホットキーガイドつきフルパネル）
- `Ctrl+Alt+Q`: 終了

「非表示」は `Mode::Off` ただ一つ。Off 中に厚さ・不透明度・スタイルのキーを押しても
設定は変更されず、HUD に「Overlay is off — Ctrl+Alt+H to show」の toast が出る
（見えない状態がこっそり変わるサプライズを排除）。

Bump 系（厚さ・不透明度）は長押しで連続発火する。Mode 切替・ON/OFF トグル・スタイル切替・
終了は誤連打を避けるため 1 押下 1 発火に固定。割り当てが OEM 系キー（`[`/`]`/`=`/`-`）では
なく Arrow キーなのは、Windows の IME / keyboard layout で OEM キーの VK が化けて
`RegisterHotKey` がキャプチャを取り逃すケースがあったため（JIS keyboard × ENG IME で再現）。

**周囲スタイル（`Ctrl+Alt+S`）**: スリット以外の領域の見せ方を切り替える。`Dim`（従来の
暗転）と `Bright`（真っ白に覆う）を、短いクロスフェードで巡回する。`Blur`（背景をぼかす
ガウシアン）は型・ADT（`Brush::Blur` / `SurroundStyle::Blur`）として予約済みで、描画パス
（画面キャプチャ + D2D `CLSID_D2D1GaussianBlur`）は後続タスク。それまでプラットフォーム側は
tint 色のソリッド塗りにフォールバックする。

表示・モード切替・スタイル切替・厚さ / 不透明度の変更は、すべて 200ms 未満の ease-out
トランジションで滑らかに遷移する（長押し中の連続調整は retarget で 1 つの連続モーションに
合流する）。モード表示は HUD チップ（右上）が担う。

**HUD は二段階表示**: 普段は画面右上に極小の常駐チップ（`H · 28px · 67%`、Off 中は
`Off`）だけが出る。起動直後の数秒はホットキーガイドつきのフルパネルが表示され、その後
チップへ自動的に畳まれる。操作を忘れたら `Ctrl+Alt+K` でいつでもフルパネル（Mode /
Thickness / Opacity / Refresh Hz と全ホットキー一覧）に展開できる。スリットやカーソルが
HUD に近づくと譲ってフェードアウトする。multi-monitor 環境では virtual screen 全体に
overlay が広がる。

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

ホストには Docker と [`just`](https://github.com/casey/just) があれば良い。すべての Rust ツール (`cargo`, `cargo-xwin`, `cargo-deny`, `cargo-nextest`, `cargo-machete`, `cargo-llvm-cov`, `cargo-audit`, `cargo-sort`, `typos`, `taplo`, `biome`, `yamlfmt`, `lefthook`, `actionlint`, `commitlint`) はコンテナ内に揃う。

```bash
just bootstrap      # 一発セットアップ: docker build + git hooks + xwin sysroot prefetch + doctor
```

`just bootstrap` がやること:

1. **dev image を取得** — `docker compose pull` で `ghcr.io/p4suta/linerule-rs-dev:latest` を取りに行き、無ければ `docker compose build` にフォールバック（フォールバック時は `gh auth token` から `GITHUB_TOKEN` を自動拾って cargo-binstall の api.github.com レートリミット回避）。CI (`.github/workflows/dev-image.yml`) が週次 + Dockerfile 変更時にイメージを更新するので、通常は ~30s の pull
2. `docker compose up -d dev` — 永続コンテナを立てて以降の `just <recipe>` を高速化
3. `lefthook install` — pre-commit / commit-msg / pre-push git hooks を `.git/hooks/` に配置
4. `bun install` — commit-msg hook が使う commitlint を入れる（npm ではなく [Bun](https://bun.sh/) を採用、爆速）
5. `just doctor` — 全ツールの疎通確認

Windows クロスコンパイル用の MSVC CRT / Windows SDK（~500 MB）は dev image に焼き込まれているので、初回の `just cross-check` も即座に通る。

困ったら `just doctor` を打てば、どのツールが落ちているかすぐ分かる。

#### 高速化の根拠（測定済み）

| 構成 | フレッシュクローン → cross-check 通る状態まで |
|---|---|
| Token 無しで build（cargo-binstall が api.github.com 60/h で 403 → 120s × N retry → source fallback、その後 xwin 7m download） | **約 20 分** |
| Token 有りで build（cargo-binstall は prebuilt、xwin sysroot を image に焼き込み済み） | **約 2.4 分** |
| ghcr.io pull（CI がビルドして push、xwin sysroot 込みで ~1.7 GB pull） | **約 30 秒** |

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

### 配布物

GitHub Release では tag push のたびに以下が attach される (`.github/workflows/release-assets.yml`、ADR-0010 / ADR-0011)。

- `linerule-vX.Y.Z-win-x64.exe` — `cargo build --release -p linerule-app` の native Windows ビルド (Phase J 以降 1 binary のみ、PDB / `dist-dev` profile は廃止)
- `linerule-vX.Y.Z-sbom.cdx.json` — `cargo-sbom` が生成する CycloneDX 1.6 JSON 形式の Software Bill of Materials。`linerule-app` の依存閉包をスキャナで直接読める

### ログとクラッシュダンプ

ランタイム時に `linerule.exe` と同じディレクトリの `events.jsonl.YYYY-MM-DD` へ tracing JSON Lines を流す（portable 運用、ADR-0011）。panic 時は同階層に `crash-<run_id>-<unix_ms>.json` が出る。

```bash
just logs-tail subsystem=wnd_proc  # subsystem フィルタ
just logs-pretty                    # 全件 pretty-print
just crash-list                     # クラッシュダンプ一覧
just crash-latest                   # 最新クラッシュダンプ
```

## Library API overview

下のブロックは `crates/linerule-core/src/lib.rs` の crate-level doc から `cargo rdme` で自動同期されます。手書きで中身を編集しないこと（`just docs` で再生成）。

<!-- cargo-rdme start -->

linerule-core

純粋ロジック層: ADT、reducer、render、parser、FSM。`#![forbid(unsafe_code)]`
で `unsafe` を完全に排除し、非決定性 (時刻・乱数・I/O) は呼び出し側から引数で
受け取る。

#### 構成

- [`anim`] — 整数エンドポイントの時間遷移 `Transition<T>` と easing
- [`color`] — `Rgba` / `Opacity` / `DimLevel` / `Thickness` と perceptual カーブ
- [`config`] — `UserConfig` ツリー (`OverlayConfig` / `HudConfig` / ...)
- [`diagnostics`] — `LineruleError` / `Severity`
- [`geometry`] — 座標空間タグ付き `Point<S>` / `ScreenRect<S>`
- [`input`] — chord parser / hold FSM / tick pipeline / HUD fade / hotkey map
- [`render`] — `OverlayFrame` ADT と純粋関数 `render::frame`
- [`state`] — `State` / `OverlayAction` / `StateDelta` と `state::reduce::apply`

#### 短い public path

主要型は `lib.rs` で再エクスポートしているので、consumer は
`linerule_core::Rgba` / `linerule_core::frame(...)` のような短い path で
書ける。internal 実装は `linerule_core::color::rgba::Rgba` などの長い
path で書き、リファクタの自由度を残す。

#### 依存方向

`linerule-app` → `linerule-platform-windows` → `linerule-core`。本クレートは
他の linerule-rs クレートに依存しない。

<!-- cargo-rdme end -->

## モジュールツリー・依存グラフ

- [`docs/modules/`](docs/modules/) — 各クレートの `cargo modules structure` 出力（自動生成）
- [`docs/dep-graph.svg`](docs/dep-graph.svg) — workspace 依存グラフ（`cargo depgraph` 自動生成）

更新は `just docs` で一括実行。`lefthook` の pre-commit が drift を検出し、生成物が古いまま commit されることを防ぐ。

## 設計・運用ドキュメント

- [`docs/adr/0001-port-from-csharp.md`](docs/adr/0001-port-from-csharp.md): 旧 C# 版 (`linerule-cs`) からの Rust 全面リライト判断、旧 ADR 処遇マッピング
- [`docs/adr/0002-architecture-principles.md`](docs/adr/0002-architecture-principles.md): 18 個の merge ブロッカー原則（一方向依存 / RAII / exhaustive match / unsafe 局所化 / `#[non_exhaustive]` を使わない / 等）
- [`docs/roadmap/phase-eta.md`](docs/roadmap/phase-eta.md): Phase η の draft ロードマップ（code signing / Winget / i18n / accessibility 候補と Out of Scope の整理）

## ライセンス

このプロジェクトは MIT または Apache-2.0 のいずれかでデュアルライセンスされます (お好みで)。

- [`LICENSE-MIT`](LICENSE-MIT)
- [`LICENSE-APACHE`](LICENSE-APACHE)
