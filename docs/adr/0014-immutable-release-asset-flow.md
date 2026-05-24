# 0014 — Immutable release との asset flow 再設計 (draft → upload → publish)

**Status:** Accepted (2026-05-24)。supersedes [[0010-release-assets-workflow]] の trigger 設計部分。命名規則・SBOM 添付・build 戦略は ADR-0010 から継承する。

**Errata (2026-05-24, same day):** 初版で `release-please-config.json` の `"skip-github-release": true` を「tag は push される、Release object だけ skip」と解釈していたが、`googleapis/release-please-action@v5` の `action.yml` を直接読むと description は `"if set to true, then do not try to tag releases"` で **tag 自体も push しない**。v0.4.1 リリース時 (2026-05-24, PR #89 merge 後) に tag が push されず release-please workflow が `untagged, merged release PRs outstanding - aborting` で抜けて stuck した。本セッションでは手動で `git tag -s v0.4.1 24545ed` + `git push origin v0.4.1` を打って解消し release-assets workflow の draft → upload → publish flow が initial fire することは検証した。次回以降の release で同じ stuck を避けるため、`release-please.yml` 側で「release-please-action 実行後、`tag_name` output と現 tag 一覧を照合し、未 push なら自前で `git tag` + `git push origin $tag` を打つ補助 step」を追加することを後続 PR で実装する (= 本 ADR を完全に supersede する ADR-0015 が立つまでの暫定運用)。

**See also:** [[0010-release-assets-workflow]] (supersede 元)、[[0011-phase-j-slim-down]] (薄い読書ツール志向)、[[release-please-token-workaround]] (memory: token 問題)、[[immutable-release-asset-block]] (memory: 422 経緯)。

## 文脈

GitHub が 2025-10-28 GA とした **immutable releases** 機能が本リポジトリで ON のまま運用される (owner 方針: 「on/off を繰り返すものじゃないので一切いじるつもりない」、2026-05-24)。immutable release は **publish 済み release への asset 追加・変更・削除を 422 で禁止する**。

旧 ADR-0010 の設計は:

1. `release-please-action` が `chore(main): release X.Y.Z` PR を merge した瞬間に tag + GitHub Release を **publish** で作る
2. release-assets.yml が `on: release: types: [published]` で trigger され、`gh release upload` で asset を **後付け**

これは immutable release 前提では破綻する: step 2 で発火する upload が常に 422 で失敗する。2026-05-24 リリース cycle (v0.3.0 + v0.4.0) で実際に両 release とも asset 0 個で終わった。

更に二次的に、`release-please-action` が `secrets.GITHUB_TOKEN` で release を作るため、GitHub の policy で他 workflow を trigger しない仕様 ([[release-please-token-workaround]]) も重なって、自動 attach は事実上 dead path だった (workflow_dispatch で手動 trigger しても同じ 422)。

## 判断

**immutable は ON のまま受け入れ、release-assets.yml を `push: tags: ["v*"]` トリガに切り替え、draft create → upload → publish の 3-step で release + asset を 1 度に組み立てる。**

GitHub の公式推奨ルートはこれだけ — draft 状態は mutable、publish (= `draft=false`) の瞬間に immutable lock が掛かるため、「publish 前に asset を全部揃える」ことだけが安全に attach できる方法。

### Workflow 連携の再設計

```
release-please.yml                 release-assets.yml
─────────────────────              ─────────────────────────────────────
on: push: branches: [main]         on: push: tags: ["v*"]
                                       workflow_dispatch (inputs.tag)
↓                                  ↓
release-please-action              gh release create $tag --draft --generate-notes
  └ skip-github-release: true      gh release upload $tag <files> --clobber
  └ tag を push (release は作らない)  gh release edit $tag --draft=false --latest
```

`release-please-config.json` の package config に **`"skip-github-release": true`** を追加することで release-please に release を作らせない。release-please は CHANGELOG / version bump / tag push までを担い、Release ページの生成は release-assets.yml に委ねる。

### Release notes の sourcing

`gh release create --generate-notes` で commit 範囲 (前 tag → 現 tag) から PR タイトル一覧を auto 生成する。release-please が CHANGELOG.md に書く形式とは厳密一致しないが、PR 番号 + タイトルの core 情報は両者で同じ。slim doctrine ([[0011-phase-j-slim-down]]) に従い、CHANGELOG section の精密抽出は採用しない (= awk script の追加メンテを避ける)。

### 冪等性 / 手動再試行

ジョブ冒頭で `gh release view $tag --json isDraft` を probe し:

- `release が存在しない` → `gh release create $tag --draft` (新規)
- `存在 + draft` → そのまま再利用 (`--clobber` upload で冪等)
- `存在 + published` → **error で停止** (immutable lock 済み、手動で別 tag を使うか release を delete + tag 削除 + retag が必要)

`workflow_dispatch (inputs.tag)` は build 失敗時の手動 retry / 過去 tag の再 build に使う。draft が残っていれば idempotent に再 upload できる。

### release-please の `release-please-action` token 問題との関係

`secrets.GITHUB_TOKEN` で release-please が tag を push しても、本 ADR の workflow は `push: tags` (= 必ず trigger 発火する event) を使うため、token PAT 化を待たずに自動 attach が動く。token PAT 化 ([[release-please-token-workaround]]) は **PR check (release-please の release PR で `ci.yml` を発火させる)** の問題が残っているが、本 ADR の scope 外。

### 既存 release (v0.2.0–v0.4.0) への遡及 attach

**実施しない**。immutable 前提で publish 済み release には asset を追加できないため、過去 release は asset 無しのまま残す。本 ADR 適用後 (v0.4.1 以降の最初の release) から asset 付きで配布される。SBOM の遡及配布が必要になったら、別 commit / 別 release tag (`v0.4.0-sbom` など) で発行する案を残すが現状は scope 外。

### Branch protection (required check) への追加判断

本 workflow は **release-assets を必須 check に入れない**。tag push event は PR check ではないため、main branch protection の required check リストには出現しない。release 失敗時の修正は (a) 失敗した draft を手動 delete + tag 削除、(b) feat / fix commit を main に積んで release-please の次バージョン PR を待つ、の手順で対応する。

### 命名規則 (ADR-0010 から継承)

```
linerule-vX.Y.Z-win-x64.exe          (release profile: stripped, panic=abort)
linerule-vX.Y.Z-sbom.cdx.json        (CycloneDX 1.6 JSON)
```

## 影響

| 項目 | Before (ADR-0010) | After (本 ADR) |
|---|---|---|
| trigger | `release: [published]` | `push: tags: ["v*"]` |
| release 作成主体 | release-please-action | release-assets.yml の `gh release create --draft` |
| asset attach | publish 後 (失敗、422) | draft 中に upload (成功) |
| release-please-config | `skip-github-release` 未設定 | **`skip-github-release: true`** を追加 |
| immutable 互換性 | ❌ 不可 | ✅ 仕様準拠 |
| release notes 出所 | release-please の CHANGELOG | `gh release ... --generate-notes` |

## 検証

次のリリース cycle (PR #89 = release 0.4.1 が現在 open) の merge をもって live 検証する:

1. release-please PR (#89) を merge
2. release-please workflow が tag `v0.4.1` を push (release は作らない)
3. push: tags trigger で release-assets.yml が起動
4. draft `v0.4.1` 作成 → EXE + SBOM upload → publish
5. `gh release view v0.4.1` で asset 2 個が確認できれば成功

失敗時の rollback は `gh release delete v0.4.1 --cleanup-tag` で全消し、原因修正後に release-please の次バージョンを待つか、`workflow_dispatch (tag=v0.4.1)` で手動再実行。

## Open questions / Followup

- release-please の `secrets.GITHUB_TOKEN` 起因の **PR check 問題** ([[release-please-token-workaround]]) は本 ADR で解消されない。release PR の `ci.yml` 14 check を毎回 close+reopen で発火させる暫定運用を継続する。
- `gh release ... --generate-notes` の出力が user expectation を満たさないと判明したら CHANGELOG.md からの section 抽出に切り替える (awk script を追加)。
- `--latest` flag は `gh release edit --draft=false` 時に最新タグ判定を強制する。複数 main 系列 (`v0.4.x` と `v0.5.x` 並列) を運用する場合は再検討。現状は単系列なので OK。
