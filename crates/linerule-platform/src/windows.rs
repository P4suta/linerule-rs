//! Windows implementation of the production [`crate::run`] event loop.
//!
//! Wires together:
//!
//! - **winit 0.30** for window creation, the OS event loop, and the
//!   `ApplicationHandler` callback structure.
//! - **wgpu 29 + vello 0.8 + peniko 0.6** for GPU 2D rendering. Each
//!   [`OverlayFrame`] coming out of `linerule_core::render` is mapped
//!   1-1 onto a `vello::Scene` of axis-aligned solid-fill rects.
//! - **`windows` crate** for the Win32 calls that make the layered
//!   overlay window click-through (`SetWindowLongPtrW` to add
//!   `WS_EX_LAYERED | WS_EX_TRANSPARENT`, then
//!   `SetLayeredWindowAttributes` for compositor blending) and for
//!   `GetCursorPos` on the polling tick.
//! - **global-hotkey 0.8** for system-wide chord registration. A
//!   dedicated thread forwards `GlobalHotKeyEvent`s into the winit
//!   user-event channel.
//!
//! All `unsafe` blocks are FFI calls to Win32 / raw window handle APIs
//! and carry a preceding `// SAFETY:` justification (enforced by
//! `cargo run -p xtask -- strict-code`).

use core::num::NonZeroUsize;
use std::sync::Arc;
use std::thread;

use crossbeam_channel::Receiver;
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager,
    hotkey::{Code, HotKey, Modifiers},
};
use linerule_core::{
    Action, Brush as CoreBrush, Geometry, Layer, Logical, Mode, OverlayFrame, Point, Rgba,
    ScreenRect, State, reduce, render,
};
use peniko::{
    Brush, Color, Fill,
    kurbo::{Affine, Rect},
};
use raw_window_handle::HasWindowHandle;
use vello::{
    AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene,
    util::{RenderContext, RenderSurface},
    wgpu,
};
use windows::Win32::{
    Foundation::{COLORREF, HWND, POINT},
    UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetCursorPos, GetWindowLongPtrW, LWA_COLORKEY, SetLayeredWindowAttributes,
        SetWindowLongPtrW, WS_EX_LAYERED, WS_EX_TRANSPARENT,
    },
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    raw_window_handle::HandleError,
    window::{Window, WindowAttributes, WindowId, WindowLevel},
};

use crate::RunError;

// ===========================================================================
// Public entry — `run` from lib.rs delegates here.
// ===========================================================================

/// Build the event loop, register hotkeys, run until exit.
///
/// # Errors
/// Returns a [`RunError`] for any platform-level failure.
pub fn run(initial_state: State, hotkeys: &[(String, Action)]) -> Result<(), RunError> {
    tracing::info!("starting Windows overlay event loop");

    let event_loop = EventLoop::<UserMessage>::with_user_event()
        .build()
        .map_err(|e| RunError::EventLoop(format!("create EventLoop: {e}")))?;

    // Register each chord and remember the hotkey id → action mapping. The
    // GlobalHotKeyManager value must outlive `run`; it owns the OS
    // registration tickets.
    let manager = GlobalHotKeyManager::new()
        .map_err(|e| RunError::Hotkey(format!("create GlobalHotKeyManager: {e}")))?;

    let mut bindings: Vec<(u32, Action)> = Vec::with_capacity(hotkeys.len());
    for (chord, action) in hotkeys {
        let hk = parse_chord(chord)?;
        manager
            .register(hk)
            .map_err(|e| RunError::Hotkey(format!("register {chord:?}: {e}")))?;
        bindings.push((hk.id(), *action));
    }

    // Forward GlobalHotKeyEvent → winit UserMessage on a dedicated thread.
    let proxy = event_loop.create_proxy();
    spawn_hotkey_forwarder(GlobalHotKeyEvent::receiver().clone(), bindings, proxy);

    let mut app = OverlayApp::new(initial_state);
    event_loop
        .run_app(&mut app)
        .map_err(|e| RunError::EventLoop(format!("run_app: {e}")))?;

    // Hold `manager` alive until exit so registrations are not torn down
    // mid-loop. `drop(manager)` releases all chords cleanly.
    drop(manager);
    Ok(())
}

#[derive(Debug, Clone)]
enum UserMessage {
    HotkeyAction(Action),
}

fn spawn_hotkey_forwarder(
    recv: Receiver<GlobalHotKeyEvent>,
    bindings: Vec<(u32, Action)>,
    proxy: winit::event_loop::EventLoopProxy<UserMessage>,
) {
    thread::Builder::new()
        .name("linerule-hotkey-forwarder".into())
        .spawn(move || {
            while let Ok(event) = recv.recv() {
                if let Some(&(_, action)) = bindings.iter().find(|(id, _)| *id == event.id()) {
                    if let Err(e) = proxy.send_event(UserMessage::HotkeyAction(action)) {
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
// `global_hotkey::HotKey` (Windows-bound). Keeping the parser
// platform-agnostic lets the Linux dev container exercise every
// corner of the grammar; this adapter is the only OS-specific glue.
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
        // Letter() always carries an ASCII uppercase byte by parser
        // construction; any other byte would be a parser bug, not a
        // runtime concern.
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
// Win32 click-through + cursor poll
// ===========================================================================

/// The Win32 colour key. Pixels rendered with this exact RGB value
/// become fully transparent under the layered-window compositor; every
/// other pixel is fully opaque. We picked PURE BLACK because vello's
/// `Color::TRANSPARENT` (the unrendered region of every frame) maps
/// to `(0, 0, 0, *)` after blitting — automatic background hole-out.
///
/// Colours we *do* want to draw must therefore avoid pure black; the
/// mask region uses `Rgba::DEFAULT_MASK` (a near-black) so it survives
/// the colour-key test as opaque mask. See `linerule_core::Rgba`.
pub(crate) const COLORKEY_TRANSPARENT: COLORREF = COLORREF(0x00_00_00);

#[expect(
    unsafe_code,
    reason = "Win32 FFI: SetWindowLongPtrW + SetLayeredWindowAttributes for click-through + colour-key transparency"
)]
fn apply_click_through(window: &Window) -> Result<(), RunError> {
    // v0.1 transparency strategy — see ADR-0009.
    //
    // wgpu's `CreateSwapChainForHwnd` path on DX12 does NOT route through
    // Direct Composition, so the swapchain's `CompositeAlphaMode` is
    // ignored by DWM for layered windows. The only Win32 mechanism that
    // produces transparent pixels in this configuration is
    // `LWA_COLORKEY`: pixels rendered with the colour key become fully
    // transparent, every other pixel is fully opaque.
    //
    // We pick `COLORKEY_TRANSPARENT = pure black` because vello's
    // `Color::TRANSPARENT` clears unrendered regions to `(0, 0, 0, 0)`,
    // which the blitter writes into the swapchain as `(0, 0, 0)` after
    // alpha drops. Mask regions use `Rgba::DEFAULT_MASK` (a near-black
    // shade chosen to NOT match the colour key) so they survive as an
    // opaque dim.
    //
    // `WS_EX_LAYERED + WS_EX_TRANSPARENT` together give per-pixel
    // colour-key transparency AND mouse pass-through.
    //
    // True per-pixel alpha (translucent bar / dim) requires Direct
    // Composition, which wgpu does not yet expose. v0.2 will lift this.
    let hwnd = win32_hwnd(window)?;
    // SAFETY: `hwnd` came from `winit::Window::window_handle()` and is
    // valid for the lifetime of the window. `GWL_EXSTYLE` is the
    // standard window-long index for extended styles.
    // `SetWindowLongPtrW` / `GetWindowLongPtrW` require running on the
    // window's owner thread, which is the event-loop thread here.
    let prev_exstyle = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    let new_exstyle = prev_exstyle | (WS_EX_LAYERED.0 as isize) | (WS_EX_TRANSPARENT.0 as isize);
    // SAFETY: see GetWindowLongPtrW above.
    let prev = unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_exstyle) };
    if prev == 0 {
        // Per MS docs, 0 may mean "previous was 0" or "error".
        // Disambiguate via a follow-up read.
        // SAFETY: same window / index validity as above.
        let confirmed = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
        if confirmed == 0 {
            return Err(RunError::ClickThrough(
                "SetWindowLongPtrW(GWL_EXSTYLE) returned 0 and read-back is 0".into(),
            ));
        }
    }
    // SAFETY: `hwnd` valid; `LWA_COLORKEY` is a stable semantic flag;
    // `bAlpha` is unused under colour-key-only mode but the Win32 ABI
    // requires a value (we pass 255 which would mean "fully opaque" if
    // `LWA_ALPHA` were also set — it is not).
    unsafe {
        SetLayeredWindowAttributes(hwnd, COLORKEY_TRANSPARENT, 255, LWA_COLORKEY).map_err(|e| {
            RunError::ClickThrough(format!("SetLayeredWindowAttributes(LWA_COLORKEY): {e}"))
        })?;
    }
    Ok(())
}

#[expect(
    unsafe_code,
    reason = "Win32 FFI: GetCursorPos writes the screen-space cursor into &mut POINT"
)]
fn poll_cursor_logical(scale: f32) -> Option<Point<Logical>> {
    let mut p = POINT { x: 0, y: 0 };
    // SAFETY: `&mut p` is a valid POINT* receiver. GetCursorPos has no
    // thread-affinity requirement and writes the screen-space cursor.
    let ok = unsafe { GetCursorPos(&mut p) };
    if ok.is_err() {
        return None;
    }
    let logical_x = (p.x as f32 / scale).round() as i32;
    let logical_y = (p.y as f32 / scale).round() as i32;
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
// Vello / wgpu rendering — uses the upstream `vello::util::RenderContext`
// + `RenderSurface` helpers, which own the offscreen `Rgba8Unorm`
// storage texture vello requires AND a `wgpu::util::TextureBlitter` that
// blits it onto the swapchain (handling the Bgra8Unorm/Rgba8Unorm
// format mismatch DX12 forces). Documented at
// https://docs.rs/vello/0.8.0/vello/util/struct.RenderSurface.html
// ===========================================================================

struct Surface {
    window: Arc<Window>,
    render_cx: RenderContext,
    render_surface: RenderSurface<'static>,
    renderer: Renderer,
    scene: Scene,
    monitor: ScreenRect<Logical>,
    dpi: f32,
}

impl Surface {
    fn new(window: Arc<Window>) -> Result<Self, RunError> {
        pollster::block_on(Self::new_async(window))
    }

    async fn new_async(window: Arc<Window>) -> Result<Self, RunError> {
        let size = window.inner_size();
        let scale = window.scale_factor() as f32;

        let mut render_cx = RenderContext::new();
        let render_surface = render_cx
            .create_surface(
                window.clone(),
                size.width.max(1),
                size.height.max(1),
                wgpu::PresentMode::AutoVsync,
            )
            .await
            .map_err(|e| RunError::Renderer(format!("RenderContext::create_surface: {e}")))?;

        // NOTE: we do NOT override `render_surface.config.alpha_mode`.
        // wgpu's `CreateSwapChainForHwnd` path on DX12 ignores
        // `CompositeAlphaMode` for layered windows — DWM only honours
        // alpha when the swapchain is created via Direct Composition
        // (`CreateSwapChainForComposition`), which wgpu does not yet
        // expose. v0.1 transparency goes through the `LWA_COLORKEY`
        // path applied in `apply_click_through`. See ADR-0009.

        let dev_handle = &render_cx.devices[render_surface.dev_id];
        let renderer = Renderer::new(
            &dev_handle.device,
            RendererOptions {
                use_cpu: false,
                antialiasing_support: AaSupport {
                    area: true,
                    msaa8: false,
                    msaa16: false,
                },
                num_init_threads: NonZeroUsize::new(1),
                pipeline_cache: None,
            },
        )
        .map_err(|e| RunError::Renderer(format!("Renderer::new: {e}")))?;

        let monitor = ScreenRect::<Logical>::new(
            Point::<Logical>::new(0, 0),
            (size.width as f32 / scale).round() as u32,
            (size.height as f32 / scale).round() as u32,
        );

        Ok(Self {
            window,
            render_cx,
            render_surface,
            renderer,
            scene: Scene::new(),
            monitor,
            dpi: scale,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.render_cx
            .resize_surface(&mut self.render_surface, width.max(1), height.max(1));
        self.monitor = ScreenRect::<Logical>::new(
            Point::<Logical>::new(0, 0),
            (width as f32 / self.dpi).round() as u32,
            (height as f32 / self.dpi).round() as u32,
        );
    }

    fn present(&mut self, frame: &OverlayFrame) -> Result<(), RunError> {
        self.scene.reset();
        for layer in &frame.layers {
            stamp_layer(&mut self.scene, layer);
        }

        let dev_handle = &self.render_cx.devices[self.render_surface.dev_id];
        let device = &dev_handle.device;
        let queue = &dev_handle.queue;

        // 1. Render vello scene into the offscreen Rgba8Unorm storage texture
        //    that vello requires.
        self.renderer
            .render_to_texture(
                device,
                queue,
                &self.scene,
                &self.render_surface.target_view,
                &RenderParams {
                    base_color: Color::TRANSPARENT,
                    width: self.render_surface.config.width,
                    height: self.render_surface.config.height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .map_err(|e| RunError::Renderer(format!("render_to_texture: {e}")))?;

        // 2. Acquire a swapchain frame, then blit the offscreen target into
        //    it via the format-aware TextureBlitter (handles Rgba8↔Bgra8).
        let surface_texture = self
            .render_surface
            .surface
            .get_current_texture()
            .map_err(|e| RunError::Renderer(format!("get_current_texture: {e}")))?;
        let surface_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("linerule-blit-overlay"),
        });
        self.render_surface.blitter.copy(
            device,
            &mut encoder,
            &self.render_surface.target_view,
            &surface_view,
        );
        queue.submit(Some(encoder.finish()));
        surface_texture.present();
        Ok(())
    }
}

fn stamp_layer(scene: &mut Scene, layer: &Layer) {
    let bounds = match layer.geometry {
        Geometry::Rect(r) => r,
        // `#[non_exhaustive]` reservation for future Geometry variants
        // (Path, Glyph, RoundedRect…). Until they land, the v0.1 render
        // path emits Rect only — see ADR-0002.
        _ => return,
    };
    let brush = match layer.brush {
        CoreBrush::Solid(c) => Brush::Solid(rgba_to_color(c)),
        _ => return,
    };
    let kr = Rect::new(
        f64::from(bounds.origin.x),
        f64::from(bounds.origin.y),
        f64::from(bounds.origin.x + bounds.width as i32),
        f64::from(bounds.origin.y + bounds.height as i32),
    );
    scene.fill(Fill::NonZero, Affine::IDENTITY, &brush, None, &kr);
}

fn rgba_to_color(c: Rgba) -> Color {
    Color::from_rgba8(c.r, c.g, c.b, c.a)
}

// ===========================================================================
// ApplicationHandler — ties everything together
// ===========================================================================

struct OverlayApp {
    state: State,
    surface: Option<Surface>,
    last_cursor: Point<Logical>,
}

impl OverlayApp {
    fn new(state: State) -> Self {
        Self {
            state,
            surface: None,
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

        // Make sure the window covers the whole primary monitor.
        if let Some(monitor) = window.current_monitor() {
            let size = monitor.size();
            let _ = window.request_inner_size(size);
            window.set_outer_position(monitor.position());
        }

        if let Err(e) = apply_click_through(&window) {
            tracing::error!(error = %e, "apply_click_through failed");
            event_loop.exit();
            return;
        }

        match Surface::new(window) {
            Ok(s) => {
                self.surface = Some(s);
                if matches!(self.state.mode, Mode::Off) {
                    self.state.mode = Mode::Bar;
                    self.state.visible = true;
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "Surface::new failed");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(s) = self.surface.as_mut() {
                    s.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserMessage) {
        match event {
            // Emergency-exit hotkey — bypasses the state machine and
            // tears the event loop down. Always responsive, even if
            // the renderer is wedged or the user lost track of which
            // mode they're in.
            UserMessage::HotkeyAction(Action::Quit) => {
                tracing::info!("Quit hotkey received — exiting event loop");
                event_loop.exit();
            }
            UserMessage::HotkeyAction(action) => {
                let _delta = reduce(&mut self.state, action);
                if let Some(s) = self.surface.as_ref() {
                    s.window.request_redraw();
                }
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let Some(surface) = self.surface.as_ref() else {
            return;
        };
        if let Some(pos) = poll_cursor_logical(surface.dpi) {
            if pos != self.last_cursor {
                self.last_cursor = pos;
                surface.window.request_redraw();
            }
        }
    }
}

impl OverlayApp {
    fn redraw(&mut self) {
        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        if !self.state.visible {
            // Present an empty scene to clear any previous frame.
            let empty = OverlayFrame::empty();
            if let Err(e) = surface.present(&empty) {
                tracing::warn!(error = %e, "present(empty) failed");
            }
            return;
        }
        let frame = render(
            self.state.mode,
            self.last_cursor,
            surface.monitor,
            &self.state.config,
        );
        if let Err(e) = surface.present(&frame) {
            tracing::warn!(error = %e, "present(frame) failed");
        }
    }
}
