//! Custom Win32 message numbers and special hit-test return values used by
//! the overlay. Plain `const` data only; no FFI calls.

#![forbid(unsafe_code)]

/// `WM_NCHITTEST` で「クリックは下層に貫通させる」と答えるための戻り値。
/// `LRESULT(-1)` を `i32` で持つ。
pub const HTTRANSPARENT: i32 = -1;

/// `WM_APP` 帯 (0x8000–0xBFFF) のカスタムメッセージ。pacer thread（Phase F）が
/// UI thread に vsync tick を通知するために使う。Phase C ではまだ送信側がいない
/// ので定数だけ置く。
pub const WM_APP_TICK: u32 = 0x8001;

/// CI smoke test 用の auto-quit message。`--duration <millis>` が指定されたとき、
/// boot.rs が別 thread で `thread::sleep(duration)` 後に `PostMessageW(hwnd,
/// WM_APP_QUIT_TIMER, 0, 0)` を発行し、wndproc が受信して `PostQuitMessage(0)`
/// に変換する。これにより `Ctrl+Alt+Q` 押下時と同じ graceful な終了 flow を
/// 自動化できる (Phase α GUI smoke test、ADR-0004 系)。
pub const WM_APP_QUIT_TIMER: u32 = 0x8002;

#[cfg(test)]
mod tests {
    //! Pin the message constants against the Win32 SDK values.

    use super::*;

    /// `HTTRANSPARENT` is documented as `(LRESULT)-1` in winuser.h.
    #[test]
    fn httransparent_is_negative_one() {
        assert_eq!(HTTRANSPARENT, -1);
    }

    /// Custom `WM_APP_*` messages must sit inside the documented
    /// `WM_APP` (0x8000) … 0xBFFF window.
    #[test]
    fn wm_app_tick_is_inside_wm_app_band() {
        const WM_APP: u32 = 0x8000;
        const WM_APP_END: u32 = 0xBFFF;
        assert!(
            (WM_APP..=WM_APP_END).contains(&WM_APP_TICK),
            "WM_APP_TICK = {WM_APP_TICK:#x} outside [{WM_APP:#x}, {WM_APP_END:#x}]"
        );
    }

    #[test]
    fn wm_app_quit_timer_is_inside_wm_app_band() {
        const WM_APP: u32 = 0x8000;
        const WM_APP_END: u32 = 0xBFFF;
        assert!(
            (WM_APP..=WM_APP_END).contains(&WM_APP_QUIT_TIMER),
            "WM_APP_QUIT_TIMER = {WM_APP_QUIT_TIMER:#x} outside [{WM_APP:#x}, {WM_APP_END:#x}]"
        );
    }

    #[test]
    fn wm_app_messages_are_distinct() {
        assert_ne!(WM_APP_TICK, WM_APP_QUIT_TIMER);
    }
}
