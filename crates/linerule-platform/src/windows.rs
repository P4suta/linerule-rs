//! Windows implementation of the production [`crate::run`] event loop.
//!
//! v0.1 transparency strategy — see ADR-0009.
//!
//! Two failed iterations established what does NOT work on Windows 11:
//!
//! 1. `WS_EX_LAYERED + LWA_COLORKEY` — the colour key is honoured for
//!    GDI-painted windows but silently ignored for DXGI-swapchain
//!    pixels. Result: solid black background.
//! 2. `SetWindowRgn` on a `WS_EX_LAYERED + DXGI swapchain` window —
//!    DWM's flip-model presentation composites the full DXGI surface
//!    onto the desktop, bypassing the window region. Same black
//!    result.
//!
//! What actually works (since Windows XP) is `UpdateLayeredWindow`
//! with a GDI DIB section in BGRA premultiplied-alpha format and a
//! `BLENDFUNCTION { AlphaFormat = AC_SRC_ALPHA }`. DWM composites the
//! bitmap with true per-pixel alpha. We render the OverlayFrame's
//! few axis-aligned rects directly into the DIB and call
//! `UpdateLayeredWindow` each tick — vello / wgpu are overkill for
//! this and add the swapchain incompatibility above.
//!
//! `WS_EX_LAYERED + WS_EX_TRANSPARENT` styles are restored: the
//! former is required by `UpdateLayeredWindow`, the latter gives
//! click-through over the visible (non-zero-alpha) pixels too.
//!
//! All `unsafe` blocks are FFI calls to Win32 APIs and carry a
//! preceding `// SAFETY:` justification (enforced by
//! `cargo run -p xtask -- strict-code`, look-back 6 lines).

use core::mem::size_of;
use std::ffi::c_void;
use std::ptr;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers},
};
use linerule_core::{
    Brush as CoreBrush, Geometry, HotkeyEffect, Layer, Lifecycle, Logical, Mode, OverlayFrame,
    Point, Rgba, ScreenRect, State, reduce, render,
};
use raw_window_handle::HasWindowHandle;
use windows::Win32::{
    Foundation::{COLORREF, HWND, POINT, SIZE},
    Graphics::Gdi::{
        AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
        CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC,
        HBITMAP, HDC, HGDIOBJ, ReleaseDC, SelectObject,
    },
    UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetCursorPos, GetWindowLongPtrW, SetWindowLongPtrW, ULW_ALPHA,
        UpdateLayeredWindow, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TRANSPARENT,
    },
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    raw_window_handle::HandleError,
    window::{Window, WindowAttributes, WindowId, WindowLevel},
};

/// Cursor poll period. winit's default `ControlFlow::Wait` only wakes
/// the loop on input events targeted at our window, but ours is
/// `WS_EX_TRANSPARENT` (click-through) so it never receives mouse
/// events at all — and `Alt+Tab` away takes the cursor focus to
/// another app, so even keyboard events stop coming. Without an
/// explicit re-poll deadline the overlay freezes the moment the user
/// switches focus or clicks, which is exactly what they reported.
/// `WaitUntil(now + ~16ms)` gives ~60 fps cursor tracking with
/// negligible CPU cost (each tick is a single `GetCursorPos` + the
/// memcpy in `repaint`, only when the cursor actually moved).
const POLL_PERIOD: Duration = Duration::from_millis(16);

use crate::RunError;

// ===========================================================================
// Public entry — `run` from lib.rs delegates here.
// ===========================================================================

/// Build the event loop, register hotkeys, run until exit.
///
/// # Errors
/// Returns a [`RunError`] for any platform-level failure.
pub fn run(initial_state: State, hotkeys: &[(String, HotkeyEffect)]) -> Result<(), RunError> {
    tracing::info!("starting Windows overlay event loop");

    let event_loop = EventLoop::<UserMessage>::with_user_event()
        .build()
        .map_err(|e| RunError::EventLoop(format!("create EventLoop: {e}")))?;

    let manager = GlobalHotKeyManager::new()
        .map_err(|e| RunError::Hotkey(format!("create GlobalHotKeyManager: {e}")))?;

    let mut bindings: Vec<(u32, HotkeyEffect)> = Vec::with_capacity(hotkeys.len());
    for (chord, effect) in hotkeys {
        let hk = parse_chord(chord)?;
        manager
            .register(hk)
            .map_err(|e| RunError::Hotkey(format!("register {chord:?}: {e}")))?;
        bindings.push((hk.id(), *effect));
    }

    let proxy = event_loop.create_proxy();
    spawn_hotkey_forwarder(GlobalHotKeyEvent::receiver().clone(), bindings, proxy);

    let mut app = OverlayApp::new(initial_state);
    event_loop
        .run_app(&mut app)
        .map_err(|e| RunError::EventLoop(format!("run_app: {e}")))?;

    drop(manager);
    Ok(())
}

#[derive(Debug, Clone)]
enum UserMessage {
    Effect(HotkeyEffect),
}

fn spawn_hotkey_forwarder(
    recv: Receiver<GlobalHotKeyEvent>,
    bindings: Vec<(u32, HotkeyEffect)>,
    proxy: winit::event_loop::EventLoopProxy<UserMessage>,
) {
    thread::Builder::new()
        .name("linerule-hotkey-forwarder".into())
        .spawn(move || {
            while let Ok(event) = recv.recv() {
                // global-hotkey 0.8 fires both `Pressed` AND `Released`
                // for every chord. Forwarding both produces a double
                // toggle — the user perceives the action as inert
                // ("only works while the key is held"). Filter on
                // `Pressed` so each chord triggers exactly once.
                if event.state != HotKeyState::Pressed {
                    continue;
                }
                if let Some(&(_, effect)) = bindings.iter().find(|(id, _)| *id == event.id()) {
                    if let Err(e) = proxy.send_event(UserMessage::Effect(effect)) {
                        tracing::warn!(?e, "event loop closed; hotkey forwarder exiting");
                        break;
                    }
                }
            }
        })
        .expect("spawn hotkey forwarder thread");
}

// ===========================================================================
// Chord adapter — `crate::chord::parse` (cross-platform) →
// `global_hotkey::HotKey` (Windows-bound).
// ===========================================================================

fn parse_chord(chord: &str) -> Result<HotKey, RunError> {
    let spec = crate::chord::parse(chord).map_err(|e| RunError::Hotkey(e.to_string()))?;
    Ok(spec_to_hotkey(spec))
}

fn spec_to_hotkey(spec: crate::chord::ChordSpec) -> HotKey {
    let mut mods = Modifiers::empty();
    if spec.modifiers.ctrl {
        mods |= Modifiers::CONTROL;
    }
    if spec.modifiers.alt {
        mods |= Modifiers::ALT;
    }
    if spec.modifiers.shift {
        mods |= Modifiers::SHIFT;
    }
    if spec.modifiers.meta {
        mods |= Modifiers::META;
    }
    let mods_arg = if spec.modifiers.any() {
        Some(mods)
    } else {
        None
    };
    HotKey::new(mods_arg, key_to_code(spec.key))
}

fn key_to_code(key: crate::chord::KeyCode) -> Code {
    match key {
        crate::chord::KeyCode::Letter(b'A') => Code::KeyA,
        crate::chord::KeyCode::Letter(b'B') => Code::KeyB,
        crate::chord::KeyCode::Letter(b'C') => Code::KeyC,
        crate::chord::KeyCode::Letter(b'D') => Code::KeyD,
        crate::chord::KeyCode::Letter(b'E') => Code::KeyE,
        crate::chord::KeyCode::Letter(b'F') => Code::KeyF,
        crate::chord::KeyCode::Letter(b'G') => Code::KeyG,
        crate::chord::KeyCode::Letter(b'H') => Code::KeyH,
        crate::chord::KeyCode::Letter(b'I') => Code::KeyI,
        crate::chord::KeyCode::Letter(b'J') => Code::KeyJ,
        crate::chord::KeyCode::Letter(b'K') => Code::KeyK,
        crate::chord::KeyCode::Letter(b'L') => Code::KeyL,
        crate::chord::KeyCode::Letter(b'M') => Code::KeyM,
        crate::chord::KeyCode::Letter(b'N') => Code::KeyN,
        crate::chord::KeyCode::Letter(b'O') => Code::KeyO,
        crate::chord::KeyCode::Letter(b'P') => Code::KeyP,
        crate::chord::KeyCode::Letter(b'Q') => Code::KeyQ,
        crate::chord::KeyCode::Letter(b'R') => Code::KeyR,
        crate::chord::KeyCode::Letter(b'S') => Code::KeyS,
        crate::chord::KeyCode::Letter(b'T') => Code::KeyT,
        crate::chord::KeyCode::Letter(b'U') => Code::KeyU,
        crate::chord::KeyCode::Letter(b'V') => Code::KeyV,
        crate::chord::KeyCode::Letter(b'W') => Code::KeyW,
        crate::chord::KeyCode::Letter(b'X') => Code::KeyX,
        crate::chord::KeyCode::Letter(b'Y') => Code::KeyY,
        crate::chord::KeyCode::Letter(b'Z') => Code::KeyZ,
        crate::chord::KeyCode::Letter(_) => Code::KeyR,
        crate::chord::KeyCode::BracketLeft => Code::BracketLeft,
        crate::chord::KeyCode::BracketRight => Code::BracketRight,
        crate::chord::KeyCode::Minus => Code::Minus,
        crate::chord::KeyCode::Equal => Code::Equal,
        crate::chord::KeyCode::ArrowUp => Code::ArrowUp,
        crate::chord::KeyCode::ArrowDown => Code::ArrowDown,
        crate::chord::KeyCode::ArrowLeft => Code::ArrowLeft,
        crate::chord::KeyCode::ArrowRight => Code::ArrowRight,
    }
}

// ===========================================================================
// Win32 window styles + cursor poll
// ===========================================================================

#[expect(
    unsafe_code,
    reason = "Win32 FFI: SetWindowLongPtrW to add WS_EX_LAYERED|WS_EX_TRANSPARENT"
)]
fn apply_window_styles(window: &Window) -> Result<(), RunError> {
    // - `WS_EX_LAYERED` is required for `UpdateLayeredWindow` to succeed.
    // - `WS_EX_TRANSPARENT` makes the window pass mouse events through
    //   on the visible (non-zero-alpha) pixels — without it the bar
    //   would steal clicks.
    // - `WS_EX_NOACTIVATE` prevents the window from getting activated
    //   even when the user clicks on a visible-but-still-pass-through
    //   pixel (some focus-stealing edge cases empirically slipped past
    //   `WS_EX_TRANSPARENT` alone).
    let hwnd = win32_hwnd(window)?;
    // SAFETY: `hwnd` from winit is valid for the window's lifetime.
    // `GWL_EXSTYLE` is the standard extended-style index. The pair
    // GetWindowLongPtrW / SetWindowLongPtrW must run on the window's
    // owner thread (the event-loop thread invoking this).
    let prev_exstyle = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    let new_exstyle = prev_exstyle
        | (WS_EX_LAYERED.0 as isize)
        | (WS_EX_TRANSPARENT.0 as isize)
        | (WS_EX_NOACTIVATE.0 as isize);
    // SAFETY: see above.
    let prev = unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_exstyle) };
    if prev == 0 {
        // SAFETY: same window / index validity as above.
        let confirmed = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
        if confirmed == 0 {
            return Err(RunError::ClickThrough(
                "SetWindowLongPtrW(GWL_EXSTYLE) returned 0 and read-back is 0".into(),
            ));
        }
    }
    Ok(())
}

#[expect(
    unsafe_code,
    reason = "Win32 FFI: GetCursorPos writes the screen-space cursor into &mut POINT"
)]
fn poll_cursor_logical(monitor_origin_physical: (i32, i32), scale: f32) -> Option<Point<Logical>> {
    let mut p = POINT { x: 0, y: 0 };
    // SAFETY: `&mut p` is a valid POINT* receiver. GetCursorPos has no
    // thread-affinity requirement.
    let ok = unsafe { GetCursorPos(&mut p) };
    if ok.is_err() {
        return None;
    }
    // `GetCursorPos` returns the system-wide *virtual screen* coordinate.
    // The renderer stores the monitor as `origin = (0, 0)` (everything is
    // relative to the bitmap's own top-left), so we shift the cursor by
    // the monitor's physical position before scaling. Without this, the
    // *vertical* mode silently breaks on any non-primary monitor: the
    // monitor that the overlay covers might sit at, say, screen X 1920,
    // and a cursor at screen X 2880 (right half of the second monitor)
    // would land at logical X 2880 against a `monitor.width = 1920` —
    // `slit_span` clamps to a zero-length interval and the renderer
    // emits no layer. Horizontal modes happen to dodge this when the
    // cursor.y stays inside the secondary's height, which is why the
    // bug looked orientation-specific.
    let local_x = p.x.saturating_sub(monitor_origin_physical.0);
    let local_y = p.y.saturating_sub(monitor_origin_physical.1);
    let logical_x = (local_x as f32 / scale).round() as i32;
    let logical_y = (local_y as f32 / scale).round() as i32;
    Some(Point::<Logical>::new(logical_x, logical_y))
}

fn win32_hwnd(window: &Window) -> Result<HWND, RunError> {
    let handle = window
        .window_handle()
        .map_err(|e: HandleError| RunError::Window(format!("window_handle: {e}")))?;
    match handle.as_raw() {
        raw_window_handle::RawWindowHandle::Win32(h) => Ok(HWND(h.hwnd.get() as *mut _)),
        other => Err(RunError::Window(format!(
            "expected Win32 window handle, got {other:?}",
        ))),
    }
}

// ===========================================================================
// Layered-window bitmap renderer (BGRA premultiplied DIB +
// UpdateLayeredWindow). Replaces the wgpu/vello path entirely on
// Windows — see ADR-0009.
// ===========================================================================

/// Convert straight-alpha [`Rgba`] into premultiplied BGRA, the format
/// `UpdateLayeredWindow` expects when `BLENDFUNCTION.AlphaFormat =
/// AC_SRC_ALPHA`.
fn premultiply_bgra(c: Rgba) -> [u8; 4] {
    let a = u32::from(c.a);
    let pm = |x: u8| -> u8 { ((u32::from(x) * a) / 255) as u8 };
    [pm(c.b), pm(c.g), pm(c.r), c.a]
}

/// GDI memory bitmap (BGRA premultiplied) wired through a memory DC,
/// ready to feed `UpdateLayeredWindow`.
struct LayeredBitmap {
    hbitmap: HBITMAP,
    mem_dc: HDC,
    old_bitmap: HGDIOBJ,
    pixels: *mut u8,
    width: u32,
    height: u32,
    monitor_pos: (i32, i32),
}

#[expect(
    unsafe_code,
    reason = "Win32 GDI: bitmap allocation, DC management, raw pixel writes"
)]
impl LayeredBitmap {
    fn new(monitor_pos: (i32, i32), width: u32, height: u32) -> Result<Self, RunError> {
        // SAFETY: `GetDC(None)` returns the screen DC handle; we
        // release it after creating the compatible memory DC.
        let screen_dc = unsafe { GetDC(None) };
        if screen_dc.is_invalid() {
            return Err(RunError::Renderer("GetDC(None) returned NULL".into()));
        }
        // SAFETY: `CreateCompatibleDC` returns a memory DC compatible
        // with `screen_dc`; we own the result and must `DeleteDC` it
        // (handled in Drop).
        let mem_dc = unsafe { CreateCompatibleDC(Some(screen_dc)) };
        if mem_dc.is_invalid() {
            // SAFETY: release the screen DC we acquired above.
            let _ = unsafe { ReleaseDC(None, screen_dc) };
            return Err(RunError::Renderer("CreateCompatibleDC failed".into()));
        }

        let mut bmi = BITMAPINFO::default();
        bmi.bmiHeader.biSize = u32::try_from(size_of::<BITMAPINFOHEADER>()).unwrap_or(0);
        bmi.bmiHeader.biWidth = i32::try_from(width).unwrap_or(0);
        // Negative height = top-down DIB so pixel (0,0) is top-left.
        bmi.bmiHeader.biHeight = -i32::try_from(height).unwrap_or(0);
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB.0;

        let mut pixels: *mut c_void = ptr::null_mut();
        // SAFETY: `bmi` lives on the stack for the duration of the
        // call. `pixels` is overwritten with the bitmap's owned bits
        // pointer (no separate free needed; freed when the HBITMAP is
        // deleted). hsection = None.
        let hbitmap =
            unsafe { CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut pixels, None, 0) }
                .map_err(|e| {
                // SAFETY: clean up the DCs we acquired before failure.
                let _ = unsafe { DeleteDC(mem_dc) };
                let _ = unsafe { ReleaseDC(None, screen_dc) };
                RunError::Renderer(format!("CreateDIBSection: {e}"))
            })?;

        // SAFETY: select our DIB into the memory DC; remember the
        // previous selection to restore on Drop.
        let old_bitmap = unsafe { SelectObject(mem_dc, HGDIOBJ(hbitmap.0)) };
        // SAFETY: release the screen DC; future presents acquire a
        // fresh one each call.
        let _ = unsafe { ReleaseDC(None, screen_dc) };

        Ok(Self {
            hbitmap,
            mem_dc,
            old_bitmap,
            pixels: pixels.cast::<u8>(),
            width,
            height,
            monitor_pos,
        })
    }

    fn clear_transparent(&mut self) {
        let n = (self.width as usize) * (self.height as usize) * 4;
        // SAFETY: `pixels` points to a contiguous BGRA buffer of length
        // exactly `width*height*4` bytes owned by the DIB section.
        unsafe { ptr::write_bytes(self.pixels, 0, n) };
    }

    /// Fill axis-aligned rectangle with premultiplied BGRA color.
    /// Coordinates clamped into [0, width) × [0, height).
    fn fill_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, premult: [u8; 4]) {
        let w = i32::try_from(self.width).unwrap_or(0);
        let h = i32::try_from(self.height).unwrap_or(0);
        let x0 = x0.clamp(0, w);
        let x1 = x1.clamp(0, w);
        let y0 = y0.clamp(0, h);
        let y1 = y1.clamp(0, h);
        for y in y0..y1 {
            let row = (y as isize) * (w as isize);
            for x in x0..x1 {
                let idx = (row + x as isize) * 4;
                // SAFETY: `idx` ∈ [0, width*height*4) by clamp above.
                unsafe {
                    *self.pixels.offset(idx) = premult[0];
                    *self.pixels.offset(idx + 1) = premult[1];
                    *self.pixels.offset(idx + 2) = premult[2];
                    *self.pixels.offset(idx + 3) = premult[3];
                }
            }
        }
    }

    fn present(&self, hwnd: HWND) -> Result<(), RunError> {
        let pos = POINT {
            x: self.monitor_pos.0,
            y: self.monitor_pos.1,
        };
        let size = SIZE {
            cx: i32::try_from(self.width).unwrap_or(0),
            cy: i32::try_from(self.height).unwrap_or(0),
        };
        let src_pos = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // SAFETY: acquire a fresh screen DC for this present, hand it
        // to UpdateLayeredWindow, release. All pointers are stack
        // locals living past the call.
        let screen_dc = unsafe { GetDC(None) };
        if screen_dc.is_invalid() {
            return Err(RunError::Renderer(
                "GetDC(None) returned NULL on present".into(),
            ));
        }
        let result = unsafe {
            UpdateLayeredWindow(
                hwnd,
                Some(screen_dc),
                Some(&pos),
                Some(&size),
                Some(self.mem_dc),
                Some(&src_pos),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        };
        // SAFETY: release screen DC even on error.
        let _ = unsafe { ReleaseDC(None, screen_dc) };
        result.map_err(|e| RunError::Renderer(format!("UpdateLayeredWindow: {e}")))
    }
}

#[expect(unsafe_code, reason = "GDI handle cleanup on Drop")]
impl Drop for LayeredBitmap {
    fn drop(&mut self) {
        // SAFETY: restore the original bitmap selection, then delete
        // our DIB and memory DC. Handles all created in `new`.
        unsafe {
            let _ = SelectObject(self.mem_dc, self.old_bitmap);
            let _ = DeleteObject(HGDIOBJ(self.hbitmap.0));
            let _ = DeleteDC(self.mem_dc);
        }
    }
}

// ===========================================================================
// ApplicationHandler — wires winit events to bitmap fills + present
// ===========================================================================

struct OverlayApp {
    state: State,
    window: Option<Arc<Window>>,
    bitmap: Option<LayeredBitmap>,
    monitor_logical: ScreenRect<Logical>,
    /// Physical position of the bound monitor's top-left corner on the
    /// virtual screen. `poll_cursor_logical` subtracts this so the
    /// renderer always sees a cursor in monitor-local coordinates.
    monitor_origin_physical: (i32, i32),
    dpi: f32,
    last_cursor: Point<Logical>,
}

impl OverlayApp {
    fn new(state: State) -> Self {
        Self {
            state,
            window: None,
            bitmap: None,
            monitor_logical: ScreenRect::<Logical>::new(Point::<Logical>::new(0, 0), 0, 0),
            monitor_origin_physical: (0, 0),
            dpi: 1.0,
            last_cursor: Point::<Logical>::new(0, 0),
        }
    }
}

impl ApplicationHandler<UserMessage> for OverlayApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = WindowAttributes::default()
            .with_title("linerule")
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop);

        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                tracing::error!(error = %e, "create_window failed");
                event_loop.exit();
                return;
            }
        };

        if let Err(e) = apply_window_styles(&window) {
            tracing::error!(error = %e, "apply_window_styles failed");
            event_loop.exit();
            return;
        }

        let Some(monitor) = window.current_monitor() else {
            tracing::error!("no current monitor — bailing");
            event_loop.exit();
            return;
        };
        let mon_size = monitor.size();
        let mon_pos = monitor.position();
        self.dpi = window.scale_factor() as f32;
        self.monitor_origin_physical = (mon_pos.x, mon_pos.y);
        self.monitor_logical = ScreenRect::<Logical>::new(
            Point::<Logical>::new(0, 0),
            (mon_size.width as f32 / self.dpi).round() as u32,
            (mon_size.height as f32 / self.dpi).round() as u32,
        );
        tracing::info!(
            mon_pos = ?mon_pos,
            mon_size = ?mon_size,
            dpi = self.dpi,
            monitor_logical = ?self.monitor_logical,
            "bound to monitor",
        );

        let bitmap =
            match LayeredBitmap::new((mon_pos.x, mon_pos.y), mon_size.width, mon_size.height) {
                Ok(b) => b,
                Err(e) => {
                    tracing::error!(error = %e, "LayeredBitmap::new failed");
                    event_loop.exit();
                    return;
                }
            };

        self.window = Some(window);
        self.bitmap = Some(bitmap);

        // First-launch policy:
        //   1. If the seeded mode is `Off` (the structural "no choice
        //      yet"), promote to horizontal Mask — the typoscope is
        //      the most useful reading-aid default.
        //   2. Force resume from any serialised `Paused(_)` lifecycle
        //      so a fresh process always starts visible (the user can
        //      always re-pause via Ctrl+Alt+P).
        let mode = match self.state.lifecycle.mode() {
            Mode::Off => Mode::MASK,
            other => other,
        };
        self.state.lifecycle = Lifecycle::Active(mode);

        self.repaint();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.repaint(),
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserMessage) {
        let UserMessage::Effect(effect) = event;
        match effect {
            HotkeyEffect::Quit => {
                tracing::info!("Quit hotkey received — exiting event loop");
                event_loop.exit();
            }
            HotkeyEffect::Apply(action) => {
                let _delta = reduce(&mut self.state, action);
                self.repaint();
            }
            // `HotkeyEffect` is `#[non_exhaustive]`; future variants
            // (e.g. `Reload`) land here additively. Default to no-op
            // so an unknown signal cannot wedge the event loop.
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // While paused, the overlay renders nothing — but we still
        // poll the cursor and update `last_cursor` so resuming snaps
        // to the user's *current* cursor position rather than the
        // stale one from when pause was pressed.
        if let Some(pos) = poll_cursor_logical(self.monitor_origin_physical, self.dpi) {
            if pos != self.last_cursor {
                self.last_cursor = pos;
                self.repaint();
            }
        }
        // Schedule the next wakeup so cursor polling continues even
        // when our click-through window receives no input events
        // (e.g. user is interacting with a different app via
        // Alt+Tab / mouse click). See the `POLL_PERIOD` doc comment.
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + POLL_PERIOD));
    }
}

impl OverlayApp {
    fn repaint(&mut self) {
        let Some(bitmap) = self.bitmap.as_mut() else {
            return;
        };
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let Ok(hwnd) = win32_hwnd(window) else {
            return;
        };

        // Render only when the lifecycle is `Active(_)`; any other
        // shape (currently just `Paused(_)`, future additions land
        // additively under `#[non_exhaustive]`) short-circuits to an
        // empty frame.
        let frame = if let Lifecycle::Active(mode) = self.state.lifecycle {
            render(
                mode,
                self.last_cursor,
                self.monitor_logical,
                &self.state.config,
            )
        } else {
            OverlayFrame::empty()
        };

        tracing::trace!(
            lifecycle = ?self.state.lifecycle,
            cursor = ?self.last_cursor,
            monitor = ?self.monitor_logical,
            layers = frame.layers.len(),
            first_layer = ?frame.layers.first(),
            "repaint",
        );

        bitmap.clear_transparent();
        for layer in &frame.layers {
            stamp_layer(bitmap, layer, self.dpi);
        }

        if let Err(e) = bitmap.present(hwnd) {
            tracing::warn!(error = %e, "UpdateLayeredWindow failed");
        }
    }
}

fn stamp_layer(bitmap: &mut LayeredBitmap, layer: &Layer, scale: f32) {
    let Geometry::Rect(b) = layer.geometry else {
        return;
    };
    let CoreBrush::Solid(c) = layer.brush else {
        return;
    };
    let s = f64::from(scale);
    let x0 = (f64::from(b.origin.x) * s).round() as i32;
    let y0 = (f64::from(b.origin.y) * s).round() as i32;
    let x1 = (f64::from(b.origin.x + i32::try_from(b.width).unwrap_or(0)) * s).round() as i32;
    let y1 = (f64::from(b.origin.y + i32::try_from(b.height).unwrap_or(0)) * s).round() as i32;
    bitmap.fill_rect(x0, y0, x1, y1, premultiply_bgra(c));
}
