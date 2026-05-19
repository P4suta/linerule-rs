//! Overlay HWND ごとに 1 つ存在する instance state。
//!
//! `Box::into_raw` で確保したアドレスが `GWLP_USERDATA` に格納され、WndProc
//! から `win32_ffi::get_userdata` 経由で `NonNull<OverlayWndState>` として
//! 取り出される。本ファイル自体は `#![forbid(unsafe_code)]`。

#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicU64, Ordering};

use tracing::Span;

/// WndProc に流れ込むメッセージごとに参照される instance state。
///
/// Phase C 段階では「ログ・診断カウンタ」のみ。Phase E でホットキー Sender、
/// Phase F で tick callback を後追加する想定。
#[derive(Debug)]
pub struct OverlayWndState {
    log_span: Span,
    nchit_count: AtomicU64,
    click_count: AtomicU64,
}

impl OverlayWndState {
    /// 新しい instance state を構築する。`log_span` は WndProc 内で
    /// `entered()` され、当 HWND のメッセージ系列を tracing 上で識別するために
    /// 使われる。
    #[must_use]
    pub fn new(log_span: Span) -> Self {
        Self {
            log_span,
            nchit_count: AtomicU64::new(0),
            click_count: AtomicU64::new(0),
        }
    }

    /// この HWND の tracing span を借りる。WndProc 内で `parent: &state.span()`
    /// のように使う想定。
    pub fn span(&self) -> &Span {
        &self.log_span
    }

    /// `WM_NCHITTEST` を 1 回受信したとカウントし、サンプリング閾値（先頭 3 件
    /// または 200 件ごと）に該当すれば `true` を返す。
    #[must_use]
    pub fn tick_nchit(&self) -> Option<u64> {
        let n = self.nchit_count.fetch_add(1, Ordering::Relaxed) + 1;
        if n <= 3 || n.is_multiple_of(200) {
            Some(n)
        } else {
            None
        }
    }

    /// `WM_LBUTTONDOWN` 系を受信した（= click-through 失敗）。
    pub fn tick_click(&self) -> u64 {
        self.click_count.fetch_add(1, Ordering::Relaxed) + 1
    }
}
