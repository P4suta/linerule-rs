# 0019 — ブランチ・タグ保護の rulesets 化と ci-required 集約ゲート

**Status:** Accepted (2026-06-30)。ビルドチャンネル分離は [[0018-build-channel-separation]] に分離。
[[0014-immutable-release-asset-flow]] の immutable release 方針をタグ保護で補完する。

**See also:** [[0014-immutable-release-asset-flow]] / [[0017-release-signing-and-attestation]] /
[[0018-build-channel-separation]]。runbook は `.github/rulesets/README.md`・docs/SUPPLY_CHAIN.md。

## 文脈

ブランチ保護が **GitHub UI 側にしか無い**:

- コード化されておらず、設定がレビュー・履歴・差分の対象外。誰がいつ何を変えたか追えない。
- 障害・誤操作で保護が消えても**復元テンプレが無い**。
- 必須 status check を UI で個別ジョブ名で列挙していると、CI ジョブの追加・改名のたびに UI 修正が要り、
  desync する。

find-my-files は `.github/rulesets/*.json` を正本 (兼 DR テンプレ) として持ち、必須 check を集約ジョブ 1 本に
束ねている。同方式を移植する。

## 判断

**ブランチ・タグ保護を `.github/rulesets/*.json` にコード化し、必須 status check を `ci-required` 1 本に集約する。**

### `ci-required` 集約ゲート (`ci.yml`)

- 既存全ジョブを `needs:` に持つ集約ジョブを追加。`join(needs.*.result)` を走査し `failure`/`cancelled` が
  あれば fail。
- `if: always()` で上流が落ちても必ず走る。**`skipped` は pass 扱い**にする — これが肝。`dependency-review`・
  `conventional-commits` は PR 限定 (`merge_group`/`push` では skip) なので、skip=pass にしないと**マージ
  キューが永久に待つ**。
- ruleset は `ci-required` という**単一 context**だけを必須にする。ジョブの増減で branch protection が
  desync しない。サードパーティ action 不要 (`re-actors/alls-green` は代替案)。

### `.github/rulesets/` (3 ファイル)

| ファイル | target | 主なルール |
|---|---|---|
| `protect-default-branch.json` | `main` | deletion 禁止 / non_fast_forward / required_linear_history / pull_request (承認 0・thread 解決必須・**squash のみ**) / required_status_checks (strict, `ci-required`) |
| `require-signed-commits.json` | `~ALL` (除 gh-pages) | required_signatures |
| `protect-release-tags.json` | `refs/tags/v*` | deletion / non_fast_forward / update (published tag を不変化) |

フィールド選定の根拠:

- **squash のみ**: CONTRIBUTING の「squash-merge only」を機械化 (find-my-files は merge/squash/rebase の 3 種だが
  本リポジトリは squash 一本)。
- **承認 0**: solo maintainer (CODEOWNERS `* @P4suta`)。ただし `required_review_thread_resolution: true` で
  未解決コメントのまま merge は防ぐ。
- **署名必須**: 供給網ハードニング ([[0017-release-signing-and-attestation]]) のコミット側。`gh-pages` 除外は
  テンプレ parity (本リポジトリは Pages を artifact deploy するので gh-pages ブランチは無く、実害無し)。
- **タグ不変化**: published `v*` を delete/move 不可にし、[[0014-immutable-release-asset-flow]] の immutable
  release を ref 側からも担保。
- **タグ作成は運用ポリシー**: `v*` タグは release-please が `GITHUB_TOKEN` (github-actions[bot]) で push する。
  `creation` をハードルールに入れると release-please を**ブロック**するため入れない。「`v*` は release 自動化
  だけが作る」は ADR / runbook 上のポリシーとして明文化する (find-my-files と同方針)。

### 正本性と DR

GitHub は**ツリー内の repo ruleset を自動適用しない** (org-level だけが import 可)。よってこれらの JSON は
正本兼 DR テンプレであり、`gh api` で import する。障害後は import を再実行すれば保護を復元できる。

### 移行手順 (classic は最後に外す)

1. `ci-required` ジョブと 3 JSON を先にマージ。
2. throwaway PR で `ci-required` という context が実際にレポートされることを確認。
3. `gh api --method POST .../rulesets --input <file>` を 3 本 import。
4. テスト PR で squash マージが通り、未署名 push / タグ削除が弾かれることを確認 (rulesets と classic は加算・
   最も厳格が勝つ)。
5. **確認後に初めて** classic UI branch protection を削除
   (`gh api --method DELETE .../branches/main/protection`)。main を一瞬も無防備にしない。

**前提**: `required_signatures` on `~ALL` は未署名 push をブロックする。import 前に署名 (GPG/SSH/gitsign) または
GitHub UI マージ (web-flow 署名) を整える。

## 影響

| 項目 | Before | After (本 ADR) |
|---|---|---|
| branch 保護 | UI のみ・履歴外 | `.github/rulesets/*.json` (レビュー可・DR テンプレ) |
| 必須 check | UI で個別ジョブ列挙 (desync 源) | `ci-required` 1 本 |
| タグ保護 | 無し | `v*` 不変 (delete/move 不可) |
| 署名コミット | 任意 | `~ALL` で必須 |
| merge queue | (個別 check で skip 扱い不定) | `ci-required` が skip=pass で安定 |

## 検証

- 静的: `actionlint` で `ci.yml` の `ci-required` を検査。3 JSON が `gh api` スキーマでパースできること
  (`jq -e .`)、biome JSON format 緑。
- 動的 (移行手順 2–5): throwaway PR で `ci-required` 表示 → 3 ファイル import →
  `gh api repos/P4suta/linerule-rs/rulesets` で 3 ruleset active → テスト PR で squash 緑・未署名 push 拒否・
  `v*` タグ削除拒否を確認 → classic protection 削除。
- docs-only PR (一部ジョブ skip) でも `ci-required` が緑になること。

## Open questions / Followup

- **タグ `update` ルール**: import 時に `{ "type": "update" }` が API で弾かれる場合は `deletion` +
  `non_fast_forward` に縮約する (両者だけでも実用上の不変化は確保できる)。
- 将来 CodeQL (`analyze`) を入れるなら `protect-default-branch.json` の required_status_checks に context を
  追加する (find-my-files は `analyze` を持つ)。
- bypass_actors は空 (admin も含め例外なし)。緊急時の一時 bypass 運用が要るなら別途追記する。
