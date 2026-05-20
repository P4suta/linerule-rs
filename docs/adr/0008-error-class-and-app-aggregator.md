# 0008 — `ErrorClass` 分類と `AppError` aggregator

**Status:** Accepted (Phase H, 2026-05-20).

**See also:** [[0002-architecture-principles]] (closed sum / 一方向依存), [[0003-unsafe-isolation]], [[0007-debug-build-and-panic-strategy]]、Phase H plan の H1/H2。

## 文脈

これまでエラー型は:

```
linerule-core::diagnostics
  ├── CoreError { Opacity, Thickness }
  ├── ChordError { Empty, EmptyToken, UnknownPart, MultipleKeys, NoKey }
  └── LineruleError { Core(CoreError), Chord(ChordError) }
        Severity { Error|Warn|Info|Debug|Trace }   # logging level lattice

linerule-platform-windows::error
  └── PlatformError { NullHandle, BoolFalse, BadHr, LastError, Chord(ChordError) }
```

を備え、`thiserror::Error` + `#[from]` で `?` chain を維持していた。一方で:

1. **エラーの「回復可能性」を型で表現していなかった**。HUD に出すべき `Recoverable` か、プロセス終了で残すべき `Fatal` か、プログラマ誤りの `ProgrammerError` かは、すべて caller が `match` で個別判断する必要があった。
2. **`PlatformError → LineruleError` の経路がなかった**。`?` 1 つで合流できるのは `ChordError` だけ。アプリ層で「core も platform も同じ surface で受けたい」とき、`anyhow::Error` か手動 match に頼っていた。

`linerule-core::LineruleError` に `Platform(PlatformError)` variant を生やせば 2 は解決するが、`linerule-core` が `linerule-platform-windows` に依存することになり、依存方向 `app → platform-windows → core` の純度が崩れる ([[0002-architecture-principles]] §1)。`orphan rule` でも `impl From<PlatformError> for LineruleError` を platform 側に書くのは不可 (LineruleError は core 製で local じゃない)。

## 判断

### 1. `ErrorClass` を `linerule-core::diagnostics` に追加

`Severity` (logging level) とは完全に別 enum。意味論が違うので名前も別:

```rust
pub enum ErrorClass {
    Recoverable,       // log + fallback で継続
    Fatal,             // プロセス終了 + crash report
    ProgrammerError,   // 静的バグ tag (debug_assert! の余地)
}
```

各エラー型に `class()` method を生やす:

```rust
impl CoreError { pub const fn class(self) -> ErrorClass { ProgrammerError } }
impl ChordError { pub const fn class(&self) -> ErrorClass { Recoverable } }
impl LineruleError { pub const fn class(&self) -> ErrorClass { /* delegate */ } }
impl PlatformError { pub fn class(&self) -> ErrorClass { /* operation-aware */ } }
```

`PlatformError::class` は `operation: &'static str` で `RegisterHotKey` / `UnregisterHotKey` 等の既知 recoverable API を白リスト式に分岐し、それ以外は `Fatal` を返す。

### 2. `AppError` aggregator を `linerule-app/src/error.rs` に新設

`linerule-core::LineruleError` には Platform variant を追加せず、合流点を app 層に持たせる:

```rust
// linerule-app/src/error.rs
#[derive(Debug, thiserror::Error)]
pub(crate) enum AppError {
    #[error(transparent)] Core(#[from] LineruleError),
    #[cfg(target_os = "windows")]
    #[error(transparent)] Platform(#[from] PlatformError),
    #[error("I/O: {0}")] Io(#[from] std::io::Error),
    #[error("serde: {0}")] Serde(#[from] serde_json::Error),
}

impl AppError {
    pub(crate) fn class(&self) -> ErrorClass { /* 内部に委譲 */ }
}
```

`Platform` variant は `[target.'cfg(windows)'.dependencies]` の cfg gate 下にあるので、`#[cfg(target_os = "windows")]` で variant 自体を Windows 限定にする。Linux テストでは `AppError::{Core, Io, Serde}` の 3 variant のみが見える。

`main()` は引き続き `anyhow::Result<()>`。`AppError` は thiserror の `#[from]` 経由で `Into<anyhow::Error>` を自動派生するので `?` chain 1 つで anyhow に上がる。`dispatch_command` 等の中層を本 PR では segregate しない (PR-E の HUD notification toast push で `AppError::class()` を消費する箇所から caller を増やしていく)。

## 結果

- 新規 enum + 4 method + 7 unit tests を `linerule-core/src/diagnostics.rs` に追加 (~140 LOC)
- `PlatformError::class()` + recoverable operation 白リスト + 6 unit tests を `linerule-platform-windows/src/error.rs` に追加 (~80 LOC)
- 新規 `linerule-app/src/error.rs` (`AppError` aggregator + tests, ~125 LOC)
- `linerule-app/Cargo.toml` に `thiserror` 依存を追加
- `linerule-core::ErrorClass` を lib.rs から re-export
- Phase H の PR-C スコープ

`linerule-core` は依然として `linerule-platform-windows` を知らない (`cargo xtask dep-graph` で確認)。`app → platform-windows → core` の純度は維持。

## 検討した代替案

### A. `LineruleError::Platform(PlatformError)` を core に追加

却下: `linerule-core` から `linerule-platform-windows` への依存逆転。orphan rule にも該当しない。

### B. `LineruleError::Platform(Box<dyn std::error::Error + Send + Sync>)`

却下: 型情報が消えて downcast に頼る必要が出る。closed sum の精神 ([[0002]] §3) に反する。

### C. `From<PlatformError> for LineruleError` を platform 側に書く

却下: orphan rule で impl 不可 (`LineruleError` も `From` trait も外、`PlatformError` だけ local — Rust が拒否)。

### D. `ErrorClass` を `Severity` に統合する

却下: 意味論が違う。`Severity` は「log の出力フィルタの閾値」、`ErrorClass` は「アプリの反応」。両者は直交していて、`Recoverable + Warn`、`Fatal + Error`、`ProgrammerError + Error` 等のすべての組み合わせが意味を持つ。

## 関連

- ADR-0007 — Debug Build profile (`dist-dev`) を `panic = "unwind"` にすることで `catch_unwind` 経路が live になり、`ProgrammerError` を debug build でも runtime に観測可能に
- Phase H PR-E — `AppError::class()` を消費し、`Recoverable` を HUD notification toast に push する経路を実装
- `linerule-rs-version-bump-cautious` — 本変更は `fix(core):` で patch bump
