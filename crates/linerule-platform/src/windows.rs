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
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene, wgpu};
use windows::Win32::{
    Foundation::{HWND, POINT},
    UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetCursorPos, GetWindowLongPtrW, LWA_ALPHA, SetLayeredWindowAttributes,
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
// Chord parser — "Ctrl+Alt+R" → global_hotkey::HotKey
// ===========================================================================

fn parse_chord(chord: &str) -> Result<HotKey, RunError> {
    let mut mods = Modifiers::empty();
    let mut code: Option<Code> = None;

    for part in chord.split('+') {
        let trimmed = part.trim();
        match trimmed.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "alt" | "option" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            "super" | "meta" | "win" | "cmd" => mods |= Modifiers::META,
            _ => {
                code = Some(parse_code(trimmed).ok_or_else(|| {
                    RunError::Hotkey(format!("unknown key {trimmed:?} in chord {chord:?}"))
                })?);
            }
        }
    }

    let code = code.ok_or_else(|| {
        RunError::Hotkey(format!("chord {chord:?} has no main key (only modifiers)"))
    })?;
    Ok(HotKey::new(Some(mods), code))
}

fn parse_code(key: &str) -> Option<Code> {
    let upper = key.to_ascii_uppercase();
    match upper.as_str() {
        // Letters
        "A" => Some(Code::KeyA),
        "B" => Some(Code::KeyB),
        "C" => Some(Code::KeyC),
        "D" => Some(Code::KeyD),
        "E" => Some(Code::KeyE),
        "F" => Some(Code::KeyF),
        "G" => Some(Code::KeyG),
        "H" => Some(Code::KeyH),
        "I" => Some(Code::KeyI),
        "J" => Some(Code::KeyJ),
        "K" => Some(Code::KeyK),
        "L" => Some(Code::KeyL),
        "M" => Some(Code::KeyM),
        "N" => Some(Code::KeyN),
        "O" => Some(Code::KeyO),
        "P" => Some(Code::KeyP),
        "Q" => Some(Code::KeyQ),
        "R" => Some(Code::KeyR),
        "S" => Some(Code::KeyS),
        "T" => Some(Code::KeyT),
        "U" => Some(Code::KeyU),
        "V" => Some(Code::KeyV),
        "W" => Some(Code::KeyW),
        "X" => Some(Code::KeyX),
        "Y" => Some(Code::KeyY),
        "Z" => Some(Code::KeyZ),
        // Punctuation widely used for our defaults
        "[" => Some(Code::BracketLeft),
        "]" => Some(Code::BracketRight),
        "-" | "MINUS" => Some(Code::Minus),
        "=" | "EQUAL" => Some(Code::Equal),
        "ARROWUP" | "UP" => Some(Code::ArrowUp),
        "ARROWDOWN" | "DOWN" => Some(Code::ArrowDown),
        "ARROWLEFT" | "LEFT" => Some(Code::ArrowLeft),
        "ARROWRIGHT" | "RIGHT" => Some(Code::ArrowRight),
        _ => None,
    }
}

// ===========================================================================
// Win32 click-through + cursor poll
// ===========================================================================

#[expect(
    unsafe_code,
    reason = "Win32 FFI: SetWindowLongPtrW / SetLayeredWindowAttributes"
)]
fn apply_click_through(window: &Window) -> Result<(), RunError> {
    let hwnd = win32_hwnd(window)?;
    // SAFETY: `hwnd` came from `winit::Window::window_handle()` and is a
    // valid HWND for the lifetime of the window. `GWL_EXSTYLE` is the
    // standard window-long index for extended styles. `SetWindowLongPtrW`
    // returns the previous value (0 on first call); we OR our flags into
    // it. Both APIs require running on the window's owner thread, which
    // is the event-loop thread invoking this.
    let prev_exstyle = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    let new_exstyle = prev_exstyle | (WS_EX_LAYERED.0 as isize) | (WS_EX_TRANSPARENT.0 as isize);
    // SAFETY: see GetWindowLongPtrW above.
    let prev = unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_exstyle) };
    if prev == 0 {
        // Per MS docs, 0 may indicate either "previous value was 0" or an
        // error. We disambiguate via a follow-up read.
        // SAFETY: same window / index validity argument as above.
        let confirmed = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
        if confirmed == 0 {
            return Err(RunError::ClickThrough(
                "SetWindowLongPtrW(GWL_EXSTYLE) returned 0 and read-back is 0".into(),
            ));
        }
    }
    // SAFETY: `hwnd` valid; `LWA_ALPHA` is a stable semantic constant;
    // alpha = 255 means "let per-pixel alpha from vello drive blending".
    unsafe {
        SetLayeredWindowAttributes(
            hwnd,
            windows::Win32::Foundation::COLORREF(0),
            255,
            LWA_ALPHA,
        )
        .map_err(|e| RunError::ClickThrough(format!("SetLayeredWindowAttributes: {e}")))?;
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
// Vello / wgpu rendering
// ===========================================================================

struct Surface {
    window: Arc<Window>,
    // wgpu::Instance is intentionally NOT held — `create_surface`
    // already binds the surface to its own backend; we don't need to
    // keep the instance alive afterwards.
    wgpu_surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    scene: Scene,
    monitor: ScreenRect<Logical>,
    dpi: f32,
}

impl Surface {
    fn new(window: Arc<Window>) -> Result<Self, RunError> {
        let size = window.inner_size();
        let scale = window.scale_factor() as f32;

        let monitor = ScreenRect::<Logical>::new(
            Point::<Logical>::new(0, 0),
            (size.width as f32 / scale).round() as u32,
            (size.height as f32 / scale).round() as u32,
        );

        let instance = wgpu::Instance::default();
        let wgpu_surface = instance
            .create_surface(window.clone())
            .map_err(|e| RunError::Renderer(format!("create_surface: {e}")))?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&wgpu_surface),
            force_fallback_adapter: false,
        }))
        .map_err(|e| RunError::Renderer(format!("request_adapter: {e}")))?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("linerule-overlay-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::default(),
        }))
        .map_err(|e| RunError::Renderer(format!("request_device: {e}")))?;

        let surface_format = wgpu_surface
            .get_capabilities(&adapter)
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::PreMultiplied,
            view_formats: Vec::new(),
            desired_maximum_frame_latency: 2,
        };
        wgpu_surface.configure(&device, &config);

        let renderer = Renderer::new(
            &device,
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

        Ok(Self {
            window,
            wgpu_surface,
            device,
            queue,
            config,
            renderer,
            scene: Scene::new(),
            monitor,
            dpi: scale,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.wgpu_surface.configure(&self.device, &self.config);
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

        let surface_texture = self
            .wgpu_surface
            .get_current_texture()
            .map_err(|e| RunError::Renderer(format!("get_current_texture: {e}")))?;

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                &self.scene,
                &view,
                &RenderParams {
                    base_color: Color::TRANSPARENT,
                    width: self.config.width,
                    height: self.config.height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .map_err(|e| RunError::Renderer(format!("render_to_texture: {e}")))?;

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

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserMessage) {
        match event {
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
