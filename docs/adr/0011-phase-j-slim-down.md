# 0011 — Phase J slim-down: AppData ログ / dist-dev / PDB を撤廃して「薄い読書ツール」志向へ回帰

**Status:** Accepted (Phase J, 2026-05-20).

**Supersedes:** [[0007-debug-build-and-panic-strategy]] (`[profile.dist-dev]` + `panic = "unwind"` + PDB 配布)。

**Amends:** [[0010-release-assets-workflow.md]] (Release asset 添付戦略を release profile 1 binary に縮退)。

**See also:** [[0009-diagnostics-cli-and-debug-assertions]] (`linerule diagnostics` サブコマンド、本 ADR で path は変わるが機能は維持)、[[feedback-enforce-in-code-not-docs]] (本プロジェクトの enforcement 方針)。

## 文脈

Phase H〜I で「実機 crash を解析しやすい」観点から以下を積み上げた:

- ログを `%APPDATA%\linerule\events.jsonl.YYYY-MM-DD` に出す (`directories::ProjectDirs` 経由、`logging.rs:67-70`)
- `[profile.dist-dev]` = `release` + `panic = "unwind"` + `strip = "none"` + `lto = "thin"` で PDB 付き Debug Build artifact を CI から配布 (ADR-0007)
- `release-assets.yml` で `linerule-vX.Y.Z-win-x64-debug.exe` + `*.pdb` を Release に自動添付 (ADR-0010)
- `win32_ffi/core.rs::overlay_wnd_proc` の `catch_unwind` 経路を `dist-dev` でのみ実機検証可能に (ADR-0007)

しかしユーザーの本来の意図 (2026-05-20 フィードバック):

> いまはログがAppDataあたりに吐き出されたりといろいろ凝ったことしているけれども、あくまで薄い存在としての読書ツールを目指したいから普通にexeのディレクトリでjsonで吐いてくれればいいよ。debugビルドもpdb何て凝ったことせずに普通にjsonでいいよ。

ポータブル運用の読書補助ツールという原点に対し、「`%APPDATA%` への書き込み」「2 種類の binary profile」「PDB 配布」「`panic = "unwind"` 非対称」は過剰装備。

## 判断

**配布は `linerule.exe` 1 つ、ログは exe と同じディレクトリ。`dist-dev` profile・PDB 配布・panic 非対称は撤廃する。**

### 1. ログ出力先: `%APPDATA%\linerule\` → exe と同階層

`crates/linerule-app/src/logging.rs::data_dir()`:

```rust
// Before
pub(crate) fn data_dir() -> Result<PathBuf> {
    let pd = ProjectDirs::from("rs", "linerule", "linerule")
        .context("ProjectDirs::from returned None")?;
    Ok(pd.data_dir().to_path_buf())
}

// After
pub(crate) fn data_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("std::env::current_exe failed")?;
    let dir = exe
        .parent()
        .context("current_exe path has no parent directory")?
        .to_path_buf();
    Ok(dir)
}
```

- `directories` crate 依存を workspace + app crate から削除
- daily rolling (`tracing_appender::rolling::daily`) は維持 — tracing-appender 標準機能で「凝ったこと」の対象外、複数 run の log 区切りに有用
- crash dump JSON (`crash-<run_id>-<unix_ms>.json`) も同階層に同居 (`crash_dump::crash_path` は `logging::data_dir()` 経由で自動追随)
- 書き込み権限が無い場合 (Program Files 配下に install) は `init()` が `Err` → 起動失敗。ポータブル前提なので意図したフェイル方法

### 2. `[profile.dist-dev]` 撤廃 / `panic = "abort"` 統一

`Cargo.toml` から `[profile.dist-dev]` ブロックを完全削除。配布 binary は release profile (`panic = "abort"` + `strip = "symbols"` + `lto = "fat"`) のみ。

副作用:
- `win32_ffi/core.rs::overlay_wnd_proc` の `catch_unwind(AssertUnwindSafe(...))` 経路は再び effectively dead (panic 即 abort)。**コード自体は残す** — 削除する積極的理由がなく、将来 unwind に戻したくなったときの「無害な保険」として `unsafe` 境界の defensive コードを保持
- crash dump 機能は `panic = "abort"` 下でも動く (panic hook は abort 前に fire する) ため維持
- ADR-0007 の release/dist-dev 非対称表は履歴のみ。実コードベース上の非対称は消失

### 3. CI / Release artifact の縮退

- `.github/workflows/ci.yml` の `debug-build (win-x64, native, PDB)` job 全体を削除
- `.github/workflows/release-assets.yml` から `cargo build --profile dist-dev` ステップ + `-debug.exe` / `-debug.pdb` の Copy/upload 行を削除。`linerule-vX.Y.Z-win-x64.exe` 1 つだけ添付
- `Justfile` の `build-debug` recipe を削除

### 4. 触らない範囲 (= 残す)

「凝ったこと」の境界を明示する:

| 項目 | 判断 | 理由 |
|---|---|---|
| daily rolling (`events.jsonl.YYYY-MM-DD`) | 残す | tracing-appender 標準機能、複数 run の log 区切りに有用 |
| crash dump JSON | 残す | panic hook 1 つで 1 ファイル書くだけ、abort 下でも fire する |
| `linerule diagnostics` サブコマンド | 残す | path だけ自動追随、機能は維持 |
| `event_ring` (recent events ring buffer) | 残す | crash dump の `recent_events` field の supplier |
| `win32_ffi/core.rs::overlay_wnd_proc` の `catch_unwind` | 残す | abort 下で dead だが無害、将来の保険 |
| `LINERULE_LOG` 環境変数 | 残す | `EnvFilter` 1 行、tracing-subscriber 標準 |

## 結果

- `crates/linerule-app/src/logging.rs::data_dir()` を exe-relative に書き換え (~5 LOC)
- `crates/linerule-app/Cargo.toml` から `directories` 依存削除
- `Cargo.toml` (workspace) から `directories = "6"` と `[profile.dist-dev]` ブロック削除 (~13 LOC)
- `Justfile` から `build-debug` recipe 削除 (~5 LOC)
- `.github/workflows/ci.yml` から `debug-build` job 削除 (~40 LOC)
- `.github/workflows/release-assets.yml` の縮退 (~10 LOC 減)
- `docs/adr/0007-debug-build-and-panic-strategy.md` の Status を Superseded に変更
- `docs/adr/0010-release-assets-workflow.md` の Build 戦略 / 命名規則表を縮退に追従
- `README.md` の log path 記述更新

## 検討した代替案

### A. AppData は維持、PDB だけ撤廃

却下: ユーザーは「ログが AppData あたりに吐き出されたり」を「凝ったこと」と明確に挙げており、両方撤廃するのが筋。AppData 維持は「OS 統合があり portable じゃない」状態を残す。

### B. dist-dev は維持、PDB だけ削る

却下: `[profile.dist-dev]` 単独の存在意義 (= PDB + unwind + 非対称) を抜くと残るのは「ほぼ release な別 profile」になり、二重管理コストに見合わない。

### C. crash dump JSON も廃止 (panic hook 自体を削る)

却下: panic 時の「何が起きたか」を残す機構は 1 ファイルの JSON write のみで「凝ったこと」の対象外。診断ツールとして minimum viable。本 ADR では維持し、別タスクで議論する。

## 関連

- ADR-0007 — supersede 元 (Debug Build profile / panic 戦略)
- ADR-0010 — amend 元 (Release asset workflow、Build 戦略を縮退)
- ADR-0009 — diagnostics サブコマンド (path 変更のみで機能維持)
- memory `feedback-enforce-in-code-not-docs` — 本 task でも「ドキュメントでなくコードで強制」方針を踏襲 (本 ADR は履歴記録、enforcement そのものは Cargo.toml/CI 等のコード変更で達成)
