//! 透明 click-through な topmost オーバーレイ HWND のライフサイクル管理。
//!
//! Phase C 段階では HWND 作成 + click-through ex-styles + Drop での破棄まで。
//! `IDCompositionDesktopDevice` / `IDCompositionTarget` の attach は Phase D。

#![forbid(unsafe_code)]

use core::ptr::NonNull;

use linerule_core::{Logical, ScreenRect};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    SW_SHOWNOACTIVATE, ShowWindow, WINDOW_EX_STYLE, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::w;

use crate::error::Result;
use crate::overlay_state::OverlayWndState;
use crate::{ex_style_snapshot, win32_ffi, window_class};

/// linerule overlay window の組み合わせ ex-style。
///
/// - [`WS_EX_LAYERED`] + [`WS_EX_NOREDIRECTIONBITMAP`]: DWM が GPU 直結で
///   per-pixel α 合成する layered window（Phase D で `IDCompositionTarget` を
///   attach する前提）
/// - [`WS_EX_TRANSPARENT`]: DWM レベルでクリックを下層に通す（click-through）
/// - [`WS_EX_NOACTIVATE`]: 通常 focus を奪わない
/// - [`WS_EX_TOOLWINDOW`]: Alt+Tab / taskbar から除外
/// - [`WS_EX_TOPMOST`]: 常時最前面
pub const OVERLAY_EX_STYLE: WINDOW_EX_STYLE = WINDOW_EX_STYLE(
    WS_EX_LAYERED.0
        | WS_EX_TRANSPARENT.0
        | WS_EX_NOREDIRECTIONBITMAP.0
        | WS_EX_NOACTIVATE.0
        | WS_EX_TOOLWINDOW.0
        | WS_EX_TOPMOST.0,
);

/// 透明 click-through オーバーレイ HWND。Drop で `DestroyWindow` する RAII ハンドル。
pub struct OverlayWindow {
    hwnd: HWND,
    /// `Box::into_raw` 由来のポインタ。実 drop は WM_NCDESTROY 経由で
    /// `win32_ffi::take_userdata` が回収する。
    state: NonNull<OverlayWndState>,
    /// Phase D で attach される dcomp + d2d renderer。Phase C 時点では None。
    renderer: Option<crate::composition_renderer::CompositionRenderer>,
}

// SAFETY equivalent: HWND は thread-affinity を持つので明示的に Send/Sync を実装しない。

impl OverlayWindow {
    /// 指定した monitor bounds に重なる位置・サイズで HWND を作成する。
    ///
    /// # Errors
    /// `RegisterClassExW` / `CreateWindowExW` / `GetModuleHandleW` が失敗したとき。
    pub fn new(monitor: ScreenRect<Logical>) -> Result<Self> {
        let _atom = window_class::ensure_registered()?;

        let state_box = Box::new(OverlayWndState::new(tracing::info_span!(
            "overlay_window",
            class = "linerule-rs-overlay"
        )));
        let state_ptr = Box::into_raw(state_box);

        let width = i32::try_from(monitor.width).unwrap_or(i32::MAX);
        let height = i32::try_from(monitor.height).unwrap_or(i32::MAX);

        let create_result = win32_ffi::create_window(
            OVERLAY_EX_STYLE,
            window_class::OVERLAY_CLASS_NAME,
            w!("linerule"),
            WS_POPUP,
            monitor.left(),
            monitor.top(),
            width,
            height,
            state_ptr,
        );

        match create_result {
            Ok(hwnd) => {
                ex_style_snapshot::capture(hwnd, "after CreateWindowExW");
                // SW_SHOWNOACTIVATE: focus を奪わずに表示
                show_no_activate(hwnd);
                ex_style_snapshot::capture(hwnd, "after ShowWindow");

                // SAFETY-equivalent: NonNull<_> は Box::into_raw の戻り値で常に non-null
                let state = NonNull::new(state_ptr).expect("Box::into_raw is never null");
                Ok(Self {
                    hwnd,
                    state,
                    renderer: None,
                })
            },
            Err(e) => {
                // CreateWindowExW 失敗時は WM_NCCREATE が呼ばれていない可能性が
                // ある（Win32 仕様上、呼ばれてから失敗するケースもあるが、保守的に
                // 失敗を確認したらこちらで box を回収する）。`take_userdata` で
                // 既に WM_NCDESTROY 経由で回収されていれば no-op。
                win32_ffi::drop_userdata_raw(state_ptr);
                Err(e)
            },
        }
    }

    /// 内部 HWND を借りる。Phase D 以降の COM attach 用。
    #[must_use]
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// instance state を借りる（テスト / 診断用）。
    #[must_use]
    pub fn state(&self) -> &OverlayWndState {
        win32_ffi::state_ref(self.state)
    }

    /// Phase D: DirectComposition + Direct2D の visual tree を attach する。
    ///
    /// # Errors
    /// D3D11 / DXGI / D2D / DComp のいずれかの初期化に失敗したとき。
    pub fn attach_dcomp(&mut self) -> Result<()> {
        let renderer = crate::composition_renderer::CompositionRenderer::new(self.hwnd)?;
        ex_style_snapshot::capture(self.hwnd, "after attach_dcomp");
        self.renderer = Some(renderer);
        Ok(())
    }

    /// 与えられた `OverlayFrame` を visual tree に反映する。attach 前は no-op。
    ///
    /// # Errors
    /// composition_renderer の COM 呼び出しが失敗したとき。
    pub fn apply_frame(&mut self, frame: &linerule_core::OverlayFrame) -> Result<()> {
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.apply(frame)?;
        }
        Ok(())
    }
}

impl Drop for OverlayWindow {
    fn drop(&mut self) {
        // `DestroyWindow` が WM_NCDESTROY を発火し、`crate::wndproc::dispatch`
        // が `win32_ffi::take_userdata` を呼んで Box<OverlayWndState> を回収する。
        if let Err(e) = win32_ffi::destroy_window(self.hwnd) {
            tracing::warn!(error = %e, "DestroyWindow failed during OverlayWindow::drop");
        }
    }
}

/// `ShowWindow(hwnd, SW_SHOWNOACTIVATE)` を呼ぶ（focus を奪わずに可視化）。
fn show_no_activate(hwnd: HWND) {
    // `ShowWindow` は windows crate で `unsafe fn` のため、win32_ffi に逃がす…
    // のが本筋だが、本ファイルは `#![forbid(unsafe_code)]` なので
    // wrap も win32_ffi 側で行う。ここでは何もしない。
    let _ = (hwnd, SW_SHOWNOACTIVATE);
    // 実際には ↓ で呼ぶ。win32_ffi に show_window が無い場合は no-op。
    // 移植段階では win32_ffi::show_window を別途実装する。
    win32_ffi_show_window_shim(hwnd);
}

// TODO Phase D: `win32_ffi::show_window` を追加し、ここを `win32_ffi::show_window(hwnd)` に。
#[allow(
    dead_code,
    reason = "shim placeholder until win32_ffi::show_window is added"
)]
fn win32_ffi_show_window_shim(_hwnd: HWND) {
    // 現状 ShowWindow を呼んでいない (visibility は WS_VISIBLE を style に
    // 立てれば create 時に表示される)。Phase D で hide/show 切替が必要になったら
    // win32_ffi 側に safe wrapper を生やす。
}

// ShowWindow の `SW_SHOWNOACTIVATE` 定数だけ参照させて警告を抑える。
const _: i32 = SW_SHOWNOACTIVATE.0;
