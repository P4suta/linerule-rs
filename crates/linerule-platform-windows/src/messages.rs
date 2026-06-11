//! Custom Win32 message numbers and special hit-test return values used by
//! the overlay. Plain `const` data only; no FFI calls.

#![forbid(unsafe_code)]

/// `WM_NCHITTEST` return that lets clicks pass through to windows below
/// (`LRESULT(-1)` as `i32`).
pub const HTTRANSPARENT: i32 = -1;

/// `WM_APP` band message: pacer thread notifies the UI thread of a vsync tick.
pub const WM_APP_TICK: u32 = 0x8001;

/// Auto-quit message for the CI smoke test. The wndproc converts it to
/// `PostQuitMessage(0)`, matching the `Ctrl+Alt+Q` graceful shutdown.
pub const WM_APP_QUIT_TIMER: u32 = 0x8002;

/// Posted from the `ForegroundHook` callback (which runs on the OS hook thread)
/// so the UI thread runs `SetWindowPos(HWND_TOPMOST)` on a foreground change.
pub const WM_APP_REASSERT_TOPMOST: u32 = 0x8003;

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
    fn wm_app_reassert_topmost_is_inside_wm_app_band() {
        const WM_APP: u32 = 0x8000;
        const WM_APP_END: u32 = 0xBFFF;
        assert!(
            (WM_APP..=WM_APP_END).contains(&WM_APP_REASSERT_TOPMOST),
            "WM_APP_REASSERT_TOPMOST = {WM_APP_REASSERT_TOPMOST:#x} outside [{WM_APP:#x}, {WM_APP_END:#x}]"
        );
    }

    #[test]
    fn wm_app_messages_are_distinct() {
        assert_ne!(WM_APP_TICK, WM_APP_QUIT_TIMER);
        assert_ne!(WM_APP_TICK, WM_APP_REASSERT_TOPMOST);
        assert_ne!(WM_APP_QUIT_TIMER, WM_APP_REASSERT_TOPMOST);
    }
}
