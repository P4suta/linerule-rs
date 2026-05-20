//! linerule-platform-windows
//!
//! Win32 / COM 実装層。HWND ライフサイクル、DirectComposition + Direct2D + D3D11
//! 描画 (Phase D 以降)、ホットキー、`DwmFlush` ペーシング、`tracing` への構造化
//! イベント発行のみを担う。
//!
//! このクレートは `#![cfg(windows)]` でクレートトップから Windows 専用にゲートされる。
//! Linux 上では空クレートとしてコンパイル通過させ、本物のビルドは windows-latest
//! CI と `cargo xwin check` の双方で行う。
//!
//! ## `unsafe` ポリシー (ADR-0003)
//!
//! `unsafe` は **`win32_ffi.rs` 1 ファイルに集約**する。他のモジュール
//! (`overlay_window`, `wndproc`, `monitor_info`, `windows_app`, ...) は
//! `#![forbid(unsafe_code)]` を強制し、Win32 / COM API は `win32_ffi` の薄い
//! safe wrapper 経由でのみ呼ぶ。詳細は [`docs/adr/0003-unsafe-isolation.md`]
//! を参照。
//!
//! ## 不変条件
//!
//! - ロジックを書かない (`linerule-core` の reducer / render を呼ぶだけ)
//! - `Drop` で COM オブジェクト・HWND・Hook・`JoinHandle` を確実に解放する

#![cfg(windows)]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod composition_renderer;
pub mod cursor_tracker;
pub mod error;
pub mod ex_style_snapshot;
pub mod hud_renderer;
pub mod messages;
pub mod monitor_info;
pub mod overlay_state;
pub mod overlay_window;
pub mod render_clock;
pub mod render_timing;
pub mod win32_ffi;
pub mod window_class;
pub mod windows_app;
pub mod wndproc;

pub use error::{PlatformError, Result};
pub use overlay_state::{HotkeyConflict, HotkeyFailure, OverlayWndState};
pub use overlay_window::OverlayWindow;
pub use render_clock::RenderClock;
pub use windows_app::run_message_pump;
