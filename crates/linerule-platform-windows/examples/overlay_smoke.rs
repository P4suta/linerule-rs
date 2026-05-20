//! Phase C/D smoke test。
//!
//! `cargo run --example overlay_smoke` (Windows host) で起動すると、
//! プライマリモニタ全面を覆う透明 click-through オーバーレイが立ち上がり、
//! `WM_QUIT` を受信するまでメッセージポンプがブロックする。タスクマネージャ
//! から `linerule-platform-windows.exe` を選んで終了させる、または `DestroyWindow`
//! を発火させて `PostQuitMessage` を流せばクリーンに抜ける。
//!
//! 期待する動作:
//! - 画面に「何も見えない」が、Spy++ で `linerule-rs-overlay` クラスの HWND が
//!   見える
//! - 他のウィンドウのクリックがオーバーレイを貫通する
//! - Alt+Tab に当オーバーレイが表示されない (`WS_EX_TOOLWINDOW` の効果)
//! - 故意の panic を `WndProc` に挿しても overlay は生き続ける (`catch_unwind` の効果)
//!
//! Phase E/F の hotkey + tick 結線は `linerule.exe run` 側で検証する。本 example
//! は最小構成（HWND + dcomp attach のみ）で `WndProc` の生存性を見る目的。

#![forbid(unsafe_code)]
#![allow(
    clippy::print_stderr,
    reason = "smoke example が non-windows ターゲットで実行された場合のガイド出力"
)]

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    use linerule_core::HudConfig;
    use linerule_platform_windows::{OverlayWindow, monitor_info, run_message_pump};

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(true)
        .init();

    let monitor = monitor_info::primary_bounds()?;
    tracing::info!(
        width = monitor.width,
        height = monitor.height,
        "creating overlay"
    );

    let _overlay = OverlayWindow::new(monitor, HudConfig::DEFAULT)?;
    run_message_pump()?;
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("overlay_smoke is Windows-only; build with --target x86_64-pc-windows-msvc");
}
