//! App 層のエラー集約型 `AppError`。
//!
//! 依存方向 `app → platform-windows → core` を保ったまま、core と platform の
//! エラーを合流させる aggregator。`LineruleError → AppError` と
//! `PlatformError → AppError` は thiserror の `#[from]` で自動派生、I/O と serde
//! 由来の失敗も同じ enum に統合する。
//!
//! 設計判断: なぜ `linerule-core::LineruleError` に `Platform` variant を生やさ
//! ないか — orphan rule + 依存方向の純度。`linerule-core` は `linerule-platform-
//! windows` を知らないままにし、合流点を app 層に持たせる (ADR-0008)。
//!
//! `main()` は `anyhow::Result` を維持。`AppError` は `Into<anyhow::Error>` を
//! thiserror が自動派生するので boundary で `?` 1 つで anyhow に上がる。
//!
//! `Platform` variant は Windows ターゲットでのみ存在する (`linerule-platform-
//! windows` 自体が `[target.'cfg(windows)'.dependencies]` の cfg gate 下にある
//! ため)。

#![forbid(unsafe_code)]

use linerule_core::{ErrorClass, LineruleError};
#[cfg(target_os = "windows")]
use linerule_platform_windows::PlatformError;
use thiserror::Error;

/// linerule-app の集約エラー型。core / platform / I/O / serde を同じ surface に
/// まとめる。
///
/// PR-C で型を導入、PR-E (HUD notification toast push) で `class()` を実際に
/// 消費する。それまでは `dead_code` とみなされるため `#[allow]` で明示。
#[derive(Debug, Error)]
#[allow(
    dead_code,
    reason = "PR-E (HUD notification toast push) で消費する予定の aggregator 型。\
              本 PR (PR-C) では型・From 変換・class() method だけを先に整備する"
)]
pub(crate) enum AppError {
    /// `linerule-core` 由来 (`CoreError` / `ChordError`)。
    #[error(transparent)]
    Core(#[from] LineruleError),
    /// `linerule-platform-windows` 由来。Windows target のみ。
    #[cfg(target_os = "windows")]
    #[error(transparent)]
    Platform(#[from] PlatformError),
    /// 標準入出力 (`std::fs::read_dir` 等の diagnostics 経路で出る)。
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    /// `serde_json::Error` (crash dump 読み書き等)。
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
}

impl AppError {
    /// 内部 error の `class()` に委譲する。`Io` / `Serde` は `Fatal` 既定 —
    /// CLI 経路で `diagnostics --last-crash` 等が失敗すると I/O エラーは
    /// `Fatal` (継続不能) として扱うのが自然。
    #[allow(
        dead_code,
        reason = "PR-E (HUD notification toast push) で消費する予定。本 PR では\
                  まだ caller がいない"
    )]
    pub(crate) fn class(&self) -> ErrorClass {
        match self {
            Self::Core(e) => e.class(),
            #[cfg(target_os = "windows")]
            Self::Platform(e) => e.class(),
            Self::Io(_) | Self::Serde(_) => ErrorClass::Fatal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use linerule_core::CoreError;

    #[test]
    fn app_error_absorbs_linerule_error() {
        let e: AppError = LineruleError::from(CoreError::Opacity { given: 0 }).into();
        assert!(matches!(e, AppError::Core(_)));
        assert_eq!(e.class(), ErrorClass::ProgrammerError);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn app_error_absorbs_platform_error() {
        let e: AppError = PlatformError::NullHandle {
            operation: "CreateWindowExW",
        }
        .into();
        assert!(matches!(e, AppError::Platform(_)));
        assert_eq!(e.class(), ErrorClass::Fatal);
    }

    #[test]
    fn app_error_absorbs_io_error() {
        let io = std::io::Error::other("test io error");
        let e: AppError = io.into();
        assert!(matches!(e, AppError::Io(_)));
        assert_eq!(e.class(), ErrorClass::Fatal);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn chord_error_via_platform_is_recoverable() {
        // ChordError は PlatformError::Chord 経由でも AppError::Platform 経由でも
        // `Recoverable` に流れる
        use linerule_core::ChordError;
        let e: AppError = PlatformError::from(ChordError::Empty).into();
        assert_eq!(e.class(), ErrorClass::Recoverable);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn app_error_converts_into_anyhow_via_question_mark() {
        // `?` chain で anyhow に変換できることの compile-time check。
        fn try_chain() -> anyhow::Result<()> {
            let app: AppError = PlatformError::NullHandle { operation: "test" }.into();
            Err(app)?;
            Ok(())
        }
        let err = try_chain().unwrap_err();
        assert!(err.to_string().contains("test"));
    }
}
