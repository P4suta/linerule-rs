# リリース運用 — release-please を GitHub App で駆動

`release-please.yml` は Conventional Commits からリリースPR・タグ・CHANGELOG を生成する。
認証は **GitHub App のインストールトークン**で行う（`GITHUB_TOKEN` ではない）。

## なぜ GitHub App か（`GITHUB_TOKEN` を使わない理由）

`GITHUB_TOKEN` が作成・push したイベントは**他のワークフローを連鎖起動しない**（GitHub の無限ループ
防止仕様）。この repo では次の2点で致命的になる:

- **リリースPRに CI が走らない** → `ci-required` ルールセット（ADR-0019）の必須チェックが永久に
  埋まらず**マージ不能**。
- **タグ push が `release-assets.yml` を起動しない** → 署名付きアセットが publish されない。

App のインストールトークンは独立した actor なので、その PR は CI を起動し、その tag は release-assets
を起動する。旧構成は `gh workflow run` の明示ディスパッチで後者だけ回避していたが、前者（CI）は
解決できなかった。App 化で両方解消する。

## セットアップ（一度だけ）

### A. GitHub App を作成

1. <https://github.com/settings/apps> → **New GitHub App**。
   - **Name**: 例 `linerule-release-bot`（任意・グローバル一意）
   - **Homepage URL**: repo URL で可
   - **Webhook**: Active のチェックを**外す**
   - **Repository permissions**:
     - **Contents**: Read and write（bump コミット・タグ push）
     - **Pull requests**: Read and write（リリースPR の作成/更新・`autorelease` ラベル張替）
     - 他は No access（Metadata: Read は自動）
   - **Where can this app be installed**: Only on this account
2. 作成後の画面で **App ID** を控える。
3. **Private keys** → **Generate a private key** → ダウンロードした `.pem` を保管。

### B. App を repo にインストール

4. App 設定の **Install App** → 自アカウントにインストール → **Only select repositories** で
   `linerule-rs` を選択。

### C. シークレットを登録

5. repo に2つ登録（`.pem` は中身全体を貼り付け）:

   ```bash
   gh secret set RELEASE_PLEASE_APP_ID --repo P4suta/linerule-rs            # 値: App ID（数値）
   gh secret set RELEASE_PLEASE_APP_PRIVATE_KEY --repo P4suta/linerule-rs < path/to/app.private-key.pem
   ```

   | Secret 名 | 値 |
   |---|---|
   | `RELEASE_PLEASE_APP_ID` | GitHub App の App ID |
   | `RELEASE_PLEASE_APP_PRIVATE_KEY` | 生成した秘密鍵（`.pem` 全体、BEGIN/END 行含む）|

## リリースの流れ（セットアップ後）

1. `main` に `feat:`/`fix:` 等がマージされる → release-please が **リリースPR**（version bump +
   CHANGELOG）を開く/更新する。App 作成なので**この PR で CI が走る**。
2. リリースPR をマージ → 同 run で `autorelease` ラベルをリコンサイルし、`vX.Y.Z` タグを **App
   トークンで push** → タグが `release-assets.yml` を起動。
3. release-assets が `release` 環境の**承認待ち**で停止 → 承認 → 署名付き EXE + SBOM +
   SHA256SUMS + attestation で **publish**（[SIGNING.md](SIGNING.md) / [SUPPLY_CHAIN.md](SUPPLY_CHAIN.md)）。

## 補足

- `skip-github-release: true`（`release-please-config.json`）は immutable releases（ADR-0014）対応の
  draft→publish フローを release-assets 側に任せるため。副作用でリリースPRの `autorelease: pending`
  ラベルが `tagged` に進まないので、`release-please.yml` の「reconcile autorelease label」ステップが
  毎 run で張り替えて queue 詰まりを防ぐ。
- 手動で再評価したいときは `gh workflow run release-please.yml`（`workflow_dispatch`）。
- App トークンは run ごとに発行・失効する短命トークン。PAT のような長期保管トークンは使わない。
