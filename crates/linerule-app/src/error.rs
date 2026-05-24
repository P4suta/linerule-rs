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
/// PR-C で型を導入し、PR-E (HUD notification toast push, ADR-0013) で
/// `boot::run_overlay` のエラーパスから `class()` で分類して使う。
/// `boot::run_overlay` は `cfg(target_os = "windows")` 限定なので Linux build
/// では本型の caller が存在しない (test を除く)。`dead_code` allow は Linux
/// だけに条件付与して、Windows build では実消費パスを保証する。
#[cfg_attr(
    not(target_os = "windows"),
    allow(
        dead_code,
        reason = "Linux build では `boot::run_overlay` (caller) が cfg gate で消えるが、\
                  Windows build では classify_and_log 経由で必ず消費される"
    )
)]
#[derive(Debug, Error)]
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
    ///
    /// Linux build では `classify_and_log` 自体が cfg gate で消えるため、
    /// この method の唯一の caller がいなくなる (test を除く)。`dead_code` allow
    /// を Linux 限定で付ける。
    #[cfg_attr(
        not(target_os = "windows"),
        allow(
            dead_code,
            reason = "Linux build では classify_and_log (caller) が cfg gate で消える"
        )
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

/// `AppError` を [`ErrorClass`] に応じて log + 必要なら HUD notification として
/// 通知する helper。`Recoverable` は呼び出し側に「続行可能」を `Continue` で
/// 伝え、`Fatal` / `ProgrammerError` は `Stop` を返す。HUD push 経路自体は
/// 呼び出し側の closure で渡す (overlay handle が文脈依存のため)。ADR-0013 参照。
#[cfg(target_os = "windows")]
pub(crate) fn classify_and_log(err: &AppError) -> RunDecision {
    let class = err.class();
    match class {
        ErrorClass::Recoverable => {
            tracing::warn!(error = %err, class = "recoverable", "AppError classified recoverable; continuing");
            RunDecision::Continue
        },
        ErrorClass::Fatal => {
            tracing::error!(error = %err, class = "fatal", "AppError classified fatal");
            RunDecision::Stop
        },
        ErrorClass::ProgrammerError => {
            tracing::error!(error = %err, class = "programmer", "AppError classified as programmer error; this is a bug");
            debug_assert!(false, "ProgrammerError reached classify_and_log: {err}");
            RunDecision::Stop
        },
    }
}

/// [`classify_and_log`] の戻り値。`Continue` は HUD に push して続行、`Stop` は
/// `?` 経由で main に bubble up することを期待する。
#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunDecision {
    /// 続行可能 (Recoverable)。呼び出し側で HUD notification を push する。
    Continue,
    /// 中断 (Fatal / ProgrammerError)。呼び出し側で `Err(_)?` する。
    Stop,
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
