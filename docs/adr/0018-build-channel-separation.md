# 0018 — ビルドチャンネル分離 (dev / nightly / stable) とバージョン埋め込み

**Status:** Accepted (2026-06-30)。[[0017-release-signing-and-attestation]] の stable 配布を前提に、
**リリース以外**のビルドを識別可能にする層を足す (置換ではない)。ブランチ/タグ保護の rulesets 化は
[[0019-branch-and-tag-protection-as-code]] に分離。

**See also:** [[0011-phase-j-slim-down]] (1 binary 配布) / [[0014-immutable-release-asset-flow]] /
[[0017-release-signing-and-attestation]]。runbook は docs/SUPPLY_CHAIN.md・CONTRIBUTING.md。

## 文脈

stable リリースパイプライン (release-please → release-assets、署名 + SBOM + attestation) は完成済み。
一方で **リリース以外のビルドが自分を識別できない**:

- 普段の `cargo build`・CI の release-build・出荷 EXE がすべて `linerule 0.4.1` と名乗る。
- 開発で吐き出したビルドが「リリースではない」と自己申告できず、stable と取り違えうる。
- nightly 的な「最新 main の試用ビルド」を配る公式の置き場が無い (artifact を都度 download するしかない)。

姉妹プロジェクト find-my-files は **dev / nightly / stable** の 3 チャンネルを *バージョン文字列のサフィックス*
だけで識別し、nightly を未署名 Actions artifact、stable を署名付き Release として配置済み。同じ構成を
linerule の単一 Rust ワークスペース事情に合わせて移植する。

## 判断

**チャンネルをバージョン文字列で識別し、ビルド時にバイナリへ埋め込む。** GitHub の prerelease フラグは使わない。

### バージョン書式 (`cargo xtask version --channel <dev|nightly|stable> [--date YYYYMMDD]`)

| channel | 書式 | 用途 |
|---|---|---|
| stable | `X.Y.Z` | リリース tag そのもの (クリーン) |
| dev | `X.Y.Z-dev+g<sha>` (git 無し→`-dev`、dirty→`.dirty`) | 普段の `cargo build` の既定 |
| nightly | `X.Y.Z-nightly.<date>+g<sha>` (date は 8 桁厳格検証) | 日次の未署名ビルド |

- base `X.Y.Z` は `env!("CARGO_PKG_VERSION")`。xtask も linerule-app も `version.workspace = true` を継承する
  ので、これは `[workspace.package].version` (release-please が bump する値) に等しい。**TOML パース不要**
  (find-my-files は別 crate 構成のため `toml_edit` を使うが、本リポジトリでは不要)。
- 書式の単一ソースは `xtask/src/version.rs` の純粋関数 `compute()` (git/FS に触れずユニットテスト)。

### 埋め込みは別 crate ではなく `linerule-app/build.rs` 拡張

- find-my-files が `fmf-buildstamp` を**別 crate**にしたのは、それがバイナリ群の*依存*で、`.git` 変化時に
  `fmf-core`/`fmf-ffi` を巻き込み再ビルドさせない隔離が要るため。
- linerule では `linerule-app` が**依存グラフの頂点 (leaf)** (`xtask dep-graph` が app → platform-windows →
  core の一方向を強制)。`.git/HEAD` の変化は linerule-app の build script 再実行と app 自身の再コンパイル
  だけで完結し、その懸念が**存在しない**。
- よって `build.rs` を拡張し `LINERULE_VERSION` を `cargo:rustc-env` で emit、`src/version.rs` の
  `const VERSION = env!("LINERULE_VERSION")` で受ける。新規 member 不要、`xtask dep-graph`・`docs/modules/`・
  `docs/dep-graph.svg` を**一切汚さない** (別 crate なら 3 つ全部 + drift 再生成が必要だった)。

### `LINERULE_VERSION` 優先順位 (build.rs)

1. env `LINERULE_VERSION` (非空) → **そのまま採用** (CI が stable/nightly でセット)。
2. `{CARGO_PKG_VERSION}-dev+g{short_sha}[.dirty]` (git 到達可)。
3. `{CARGO_PKG_VERSION}-dev` (git / `.git` 不在)。

no-git 経路を含め**常に**一つ emit する (さもないと `env!` でコンパイル不能)。

### 出力経路

`version` サブコマンド・clap ネイティブ `--version`/`-V`・boot バナーがすべて `crate::version::VERSION` を読む。
独自 `version` サブコマンドは後方互換で残す。

### nightly = 未署名 Actions artifact (`.github/workflows/nightly.yml`)

- 日次 cron + `workflow_dispatch`。`git log --since='24 hours ago'` で main 無変化ならスキップ (dispatch は
  バイパス)。
- release プロファイルで build、nightly バージョンをスタンプ、CI release-build と同じ Horizontal+Blur GUI
  スモーク、**未署名 exe + SHA256SUMS を 14 日保持の Actions artifact** (`linerule-nightly`) に upload。
- **意図的に** SBOM/署名/attestation/tag/Release を作らない。署名済み・attested・immutable な stable
  ([[0014-immutable-release-asset-flow]]/[[0017-release-signing-and-attestation]]) と明確に区別する。取得は
  `gh run download --name linerule-nightly`。

### `on: schedule` ポリシー例外

`ci.yml` は「No `on: schedule`」を方針としてきた。nightly.yml は**唯一の意図的なスケジュール**であり、
製品バイナリの日次ビルド専用。CI 本体は引き続きスケジュールを持たない。`ci.yml` のヘッダコメントを更新し
本 ADR を参照する。

## 影響

| 項目 | Before | After (本 ADR) |
|---|---|---|
| dev/CI/出荷の version 表記 | 全部 `0.4.1` | dev=`-dev+g<sha>` / nightly=`-nightly.<date>+g<sha>` / stable=`0.4.1` |
| 埋め込み機構 | `env!("CARGO_PKG_VERSION")` のみ | `build.rs` が `LINERULE_VERSION` を emit、`src/version.rs` 経由 |
| `--version` フラグ | 無し (独自 `version` のみ) | clap ネイティブ `--version`/`-V` を追加 (独自 `version` も維持) |
| nightly 配布 | 無し | 未署名 Actions artifact (14 日・checksum 付き) |
| `release-assets.yml` | version env 無し | build 前に `--channel stable` をスタンプ (下記リスク) |
| schedule 方針 | CI に schedule 無し | nightly.yml が唯一の例外 |

## 検証

- `cargo test -p xtask`: `compute()` の 7 ユニットテスト緑。`cargo xtask version --channel {dev,nightly,stable}`
  を手動実行し書式を目視 (nightly は `--date` 必須・8 桁検証、未知 channel は clap が拒否)。
- `cargo build -p linerule-app` 後 `linerule version` / `--version` / `-V` が `0.4.1-dev+g<sha>[.dirty]` を出力。
  既存 `cli_smoke.rs` (`"linerule "` 前置 + base triple 包含) / `boot.rs` (stamped 値の包含) 緑。
- nightly: `gh workflow run nightly.yml` (dispatch で freshness バイパス) → `gh run download --name
  linerule-nightly` → exe が `-nightly.<date>+g<sha>` を名乗り `sha256sum -c SHA256SUMS.txt` 一致。
- stable 回帰: 次リリース (または `workflow_dispatch tag=main publish=false` の署名スモーク) で生成 EXE が
  `-dev` を含まずクリーンな `X.Y.Z` を名乗る。

## Open questions / Followup

- **最重要リスク**: `build.rs` が既定で `-dev` を刻むため、`release-assets.yml` に
  `LINERULE_VERSION=$(cargo xtask version --channel stable)` のスタンプ step を**必ず**入れる。さもないと
  出荷 EXE が `-dev` を名乗り供給網の整合 (self-reported version = tag) が壊れる。マージ後の最初の release
  より前に入れること。
- release-please の version 同期は追加配線不要 (workspace version を bump → 全 crate が継承 → 全チャンネルの
  base が自動追従)。
- nightly の cron 時刻 (04:00 UTC) と保持 14 日は暫定。運用実績を見て調整する。
