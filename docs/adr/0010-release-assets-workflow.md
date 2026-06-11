# 0010 — Release artifact auto-attach via `release-assets.yml`

**Status:** **Superseded by [[0014-immutable-release-asset-flow]]** (2026-05-24)。Accepted (2026-05-20)。後に PDB / `-debug.{exe,pdb}` 添付を撤廃し release profile の 1 binary のみに縮退、さらに immutable releases との両立のため trigger 設計を ADR-0014 で再設計した。命名規則 (`linerule-vX.Y.Z-win-x64.exe`) は維持。

**See also:** [[0011-phase-j-slim-down]]、[[0014-immutable-release-asset-flow]] (現行設計).

## 文脈

`release-please-action` (`.github/workflows/release-please.yml`) は conventional commit から version bump + CHANGELOG 更新 + tag push + GitHub Release 作成までを自動化するが、**binary asset の attach は対象外**。結果として `https://github.com/P4suta/linerule-rs/releases/latest` を訪れたユーザーは:

- ✅ Release notes (auto-generated changelog)
- ❌ Downloadable EXE
- ❌ Downloadable PDB
- ❌ Source code zip/tar (これは GitHub が自動付与するが、ビルド済みではない)

を見ることになり、「リリースされたバージョンの動作確認」が GitHub Actions の per-run artifact を漁る (90 日後消える、CI runs の中から該当 commit を探す必要) という不便な体験になっていた。

CI の `release-build (win-x64, native)` job は `linerule-win-x64` artifact (EXE のみ、90 日保持) を生成し、`debug-build (win-x64, native, PDB)` job は `linerule-win-x64-debug` (EXE + PDB、14 日保持) を生成しているが、これらは **per-CI-run** で **GitHub Release ページには無関係**だった。

ユーザー要求 (2026-05-20):

> 最新の成功したビルドとか Pages に GitHub のリポ画面から直接飛べるといいなあ。いまは Releases/Deployments 自体はあるけれど、いまいちなんだよねえ。

## 判断

**新規 workflow `.github/workflows/release-assets.yml` を追加し、`release: types: [published]` event で `release` と `dist-dev` 両 profile を build → `gh release upload --clobber` で attach する。**

### Trigger 設計

```yaml
on:
  release:
    types: [published]
  workflow_dispatch:
    inputs:
      tag:
        description: "Release tag to attach assets to (e.g. v0.2.13)"
        required: true
```

- `release: types: [published]` — release-please-action が `chore(main): release X.Y.Z` PR を merge した瞬間、tag push + GitHub Release 作成 → `published` event 発火 → 本 workflow が trigger される
- `workflow_dispatch (inputs.tag)` — (a) 過去 release への遡及 attach (b) `published` event 起動の build が失敗したときの手動 retry に使う

`types: [created]` ではなく `[published]` を選ぶ理由: draft release は除外する (release-please は published を直接作るので実害はないが、将来 draft → publish 2-phase に切り替えた場合の保険)。

### Build 戦略

release profile 1 本を build する:

```yaml
- run: cargo build --release -p linerule-app
```

> 過去には `dist-dev` profile も併走させ `-debug.exe` / `-debug.pdb` を同時添付していたが撤廃済み。詳細は [[0011-phase-j-slim-down]]。

### 命名規則

```
linerule-vX.Y.Z-win-x64.exe          (release profile: stripped, panic=abort)
```

理由:
- **version を file 名に埋め込む**: download 後 file 名だけでバージョンが分かる
- **platform/arch を埋め込む**: 将来 linux build 等を追加した時に並列で扱える (本 ADR では Windows x64 のみ)
- **`-debug` 系 (`.exe` / `.pdb`) は撤廃**: 「薄い読書ツール」志向への回帰 ([[0011-phase-j-slim-down]])

### `--clobber` flag

```yaml
gh release upload $tag <files> --clobber
```

`--clobber` は同名 asset を上書きする。理由:
- `workflow_dispatch` で手動 retry したとき冪等に動く
- `release: published` event の race condition (同時に複数 trigger される稀なケース) でも 1 度のうち 1 つは成功する

### Branch protection への追加判断

**追加しない**。理由:
- `release-assets.yml` は `release: types: [published]` event で trigger され、PR 中には走らない
- 必須 check に追加すると PR が永遠に pending になる
- merge 後の release tag タイミングで動くので、PR レビュー段階での gate は不要

release event 起動の workflow は branch protection の必須 check から除外する。

## 結果

- 新規 `.github/workflows/release-assets.yml` (~70 LOC)
- ADR-0010 (本ファイル) で設計判断と命名規則を文書化
- 次回 release-please tag push 時から、`https://github.com/P4suta/linerule-rs/releases/latest` に 3 binary assets が自動添付される

ユーザーは Releases ページから version-specific な EXE / PDB を 1-click で download できるようになり、「いまいち」だった Releases 体験が解消される。

## 検討した代替案

### A. release-please-action 内蔵の `extra-files` で binary を attach

却下: `extra-files` は **source file の version bump** (例: Cargo.toml の version field を書き換える) 専用で、build 済 binary を attach する機能ではない。release-please は asset upload を提供していない。

### B. `release-build` / `debug-build` job (ci.yml) を release event でも trigger するよう拡張

却下: ci.yml は branch push / PR を主目的に設計されており、release event を混ぜると条件分岐が複雑になる (`if: github.event_name == 'release'` で artifact 名や upload 先を切り替える等)。release 専用 workflow に分離する方が責務が明確。

### C. CI per-run artifact から download して attach (rebuild しない)

却下: per-run artifact は CI run ID 経由でしか download できず、release-please が tag を切ったタイミングで「直近 main の成功 run」を機械的に特定するロジックが必要になる。rebuild する方が単純で、`Swatinem/rust-cache` のおかげで実時間は ~6 min と短い。

### D. crates.io publish

却下: 本 repo は app crate (`linerule-app` で binary を produce) であり、library crate ではない。crates.io publish は将来 `linerule-core` 単独 publish する場合の検討事項。

## チェックリスト

- [x] `release-assets.yml` を SHA-pin した actions のみ使用 (`actions/checkout@de0fac2e...` / `Swatinem/rust-cache@c19371...`)
- [x] `permissions: contents: write` を最小限で宣言 (`gh release upload` に必要)
- [x] `workflow_dispatch` で過去 release 遡及 attach の経路を用意
- [x] `--clobber` で冪等性を確保
- [x] 命名規則 (`linerule-vX.Y.Z-win-x64{-debug}.{exe,pdb}`) を本 ADR で固定

## 関連

- ADR-0007 — Debug Build profile (`dist-dev`) と panic 戦略の非対称性 (本 ADR の build 戦略の前提)
- ADR-0014 — immutable release との asset flow 再設計 (現行設計)
