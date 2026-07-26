//! Overlay composition host on WinRT `Windows.UI.Composition`.
//!
//! Chosen over Win32 DComp (`graphics.rs`) because only WinRT can do backdrop
//! blur. Absorbs the unsafe WinRT interop / `CreateDispatcherQueueController`;
//! per-frame visual work (safe) lives in the `winrt_*_renderer` modules.

#![allow(
    unsafe_code,
    reason = "FFI boundary; WinRT interop and DispatcherQueue creation are unsafe in the windows crate."
)]

use windows::Foundation::Size;
use windows::Graphics::DirectX::{DirectXAlphaMode, DirectXPixelFormat};
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::{
    CompositionDrawingSurface, CompositionGraphicsDevice, CompositionTarget, Compositor,
    ContainerVisual,
};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Direct2D::ID2D1DeviceContext;
use windows::Win32::System::WinRT::Composition::{
    ICompositionDrawingSurfaceInterop, ICompositorDesktopInterop, ICompositorInterop,
};
use windows::Win32::System::WinRT::{
    CreateDispatcherQueueController, DQTAT_COM_STA, DQTYPE_THREAD_CURRENT, DispatcherQueueOptions,
};
use windows::core::Interface;
use windows_numerics::Vector2;

use std::cell::OnceCell;

use crate::error::{PlatformError, Result};
use crate::win32_ffi::graphics::{self, D2dStack, GraphicsBackend};

thread_local! {
    /// UI-thread controller driving WinRT Composition commits. Only one per
    /// thread (recreation errors), so set once and reused across rebuilds.
    static DISPATCHER_QUEUE: OnceCell<windows::System::DispatcherQueueController> =
        const { OnceCell::new() };
}

/// Establishes the DispatcherQueue on this thread once; no-op if already set.
///
/// `DQTAT_COM_STA`: this thread has no COM apartment, so the controller
/// establishes an STA apartment (required to create a WinRT Compositor).
fn ensure_dispatcher_queue() -> Result<()> {
    DISPATCHER_QUEUE.with(|cell| {
        if cell.get().is_some() {
            return Ok(());
        }
        let options = DispatcherQueueOptions {
            dwSize: u32::try_from(core::mem::size_of::<DispatcherQueueOptions>()).unwrap_or(0),
            threadType: DQTYPE_THREAD_CURRENT,
            apartmentType: DQTAT_COM_STA,
        };
        // SAFETY: options hold documented zero/enum values; creates a queue on the current thread.
        let controller = unsafe { CreateDispatcherQueueController(options) }.map_err(|e| {
            PlatformError::BadHr {
                operation: "CreateDispatcherQueueController",
                hr: e.code().0,
            }
        })?;
        cell.set(controller).map_err(|_| PlatformError::Invariant {
            operation: "dispatcher queue initialized twice",
        })
    })
}

/// WinRT composition host holding the `Compositor`, desktop target, shared D2D
/// stack, and HUD graphics device. Drop releases the WinRT objects.
pub struct WinrtPipeline {
    /// WinRT compositor; creates visuals / brushes / surfaces.
    pub compositor: Compositor,
    /// Desktop window target bound to the overlay HWND. Detached on Drop.
    _target: DesktopWindowTarget,
    /// Root container visual; direct children are only overlay_root and hud_root.
    _root: ContainerVisual,
    /// Intermediate visual for the overlay slit (dim + indicator).
    pub overlay_root: ContainerVisual,
    /// Intermediate visual for the HUD panel; in front of overlay_root.
    pub hud_root: ContainerVisual,
    /// Graphics device that creates the HUD's D2D drawing surface.
    pub graphics_device: CompositionGraphicsDevice,
    /// Shared D2D stack; kept alive because `graphics_device` etc. reference it.
    _stack: D2dStack,
}

/// Attaches the WinRT composition tree to the overlay HWND.
///
/// # Errors
/// When DispatcherQueue / Compositor / interop / D2D stack creation fails.
pub fn create_winrt_pipeline(
    hwnd: HWND,
    graphics_backend: GraphicsBackend,
) -> Result<WinrtPipeline> {
    // DispatcherQueue, established once on the UI thread (drained by GetMessage).
    ensure_dispatcher_queue()?;

    let compositor = Compositor::new().map_err(|e| PlatformError::BadHr {
        operation: "Compositor::new",
        hr: e.code().0,
    })?;

    // Desktop window target (interop).
    let desktop_interop: ICompositorDesktopInterop =
        compositor.cast().map_err(|e| PlatformError::BadHr {
            operation: "Compositor::cast<ICompositorDesktopInterop>",
            hr: e.code().0,
        })?;
    // SAFETY: hwnd is valid (from OverlayWindow). istopmost=true composes on top.
    let target: DesktopWindowTarget = unsafe {
        desktop_interop.CreateDesktopWindowTarget(hwnd, true)
    }
    .map_err(|e| PlatformError::BadHr {
        operation: "ICompositorDesktopInterop::CreateDesktopWindowTarget",
        hr: e.code().0,
    })?;

    // Root container visual set on the target (covers the whole window).
    let root = create_container(&compositor)?;
    let composition_target: CompositionTarget =
        target.cast().map_err(|e| PlatformError::BadHr {
            operation: "DesktopWindowTarget::cast<CompositionTarget>",
            hr: e.code().0,
        })?;
    composition_target
        .SetRoot(&root)
        .map_err(|e| PlatformError::BadHr {
            operation: "CompositionTarget::SetRoot",
            hr: e.code().0,
        })?;

    // Insert children in fixed order overlay_root then hud_root (last is front).
    let overlay_root = create_container(&compositor)?;
    let hud_root = create_container(&compositor)?;
    let children = root.Children().map_err(|e| PlatformError::BadHr {
        operation: "ContainerVisual::Children",
        hr: e.code().0,
    })?;
    children
        .InsertAtTop(&overlay_root)
        .map_err(|e| PlatformError::BadHr {
            operation: "VisualCollection::InsertAtTop(overlay_root)",
            hr: e.code().0,
        })?;
    children
        .InsertAtTop(&hud_root)
        .map_err(|e| PlatformError::BadHr {
            operation: "VisualCollection::InsertAtTop(hud_root)",
            hr: e.code().0,
        })?;

    // HUD graphics device (bridges the D2D device to WinRT).
    let stack = graphics::create_d2d_stack(graphics_backend)?;
    let comp_interop: ICompositorInterop = compositor.cast().map_err(|e| PlatformError::BadHr {
        operation: "Compositor::cast<ICompositorInterop>",
        hr: e.code().0,
    })?;
    // SAFETY: d2d_device is a valid rendering device.
    let graphics_device =
        unsafe { comp_interop.CreateGraphicsDevice(&stack.d2d_device) }.map_err(|e| {
            PlatformError::BadHr {
                operation: "ICompositorInterop::CreateGraphicsDevice",
                hr: e.code().0,
            }
        })?;

    Ok(WinrtPipeline {
        compositor,
        _target: target,
        _root: root,
        overlay_root,
        hud_root,
        graphics_device,
        _stack: stack,
    })
}

/// Creates a `ContainerVisual` covering the whole window
/// (RelativeSizeAdjustment = 1.0).
fn create_container(compositor: &Compositor) -> Result<ContainerVisual> {
    let visual = compositor
        .CreateContainerVisual()
        .map_err(|e| PlatformError::BadHr {
            operation: "Compositor::CreateContainerVisual",
            hr: e.code().0,
        })?;
    visual
        .SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })
        .map_err(|e| PlatformError::BadHr {
            operation: "Visual::SetRelativeSizeAdjustment",
            hr: e.code().0,
        })?;
    Ok(visual)
}

/// Creates the HUD's D2D drawing surface; always BGRA premultiplied.
///
/// # Errors
/// When `CreateDrawingSurface` fails.
pub fn create_drawing_surface(
    device: &CompositionGraphicsDevice,
    width: f32,
    height: f32,
) -> Result<CompositionDrawingSurface> {
    device
        .CreateDrawingSurface(
            Size {
                Width: width,
                Height: height,
            },
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            DirectXAlphaMode::Premultiplied,
        )
        .map_err(|e| PlatformError::BadHr {
            operation: "CompositionGraphicsDevice::CreateDrawingSurface",
            hr: e.code().0,
        })
}

/// Begins D2D drawing; pair with `end_surface_draw`. Returned `offset` is the
/// top-left within the surface tile (apply to the `SetTransform` translation).
///
/// # Errors
/// When `BeginDraw` fails.
pub fn begin_surface_draw(
    surface: &CompositionDrawingSurface,
) -> Result<(ID2D1DeviceContext, POINT)> {
    let interop: ICompositionDrawingSurfaceInterop =
        surface.cast().map_err(|e| PlatformError::BadHr {
            operation: "CompositionDrawingSurface::cast<ICompositionDrawingSurfaceInterop>",
            hr: e.code().0,
        })?;
    let mut offset = POINT::default();
    let update_rect: Option<*const RECT> = None;
    // SAFETY: interop is valid; iid is ID2D1DeviceContext, offset is an out param.
    let dc: ID2D1DeviceContext =
        unsafe { interop.BeginDraw(update_rect, &mut offset) }.map_err(|e| {
            PlatformError::BadHr {
                operation: "ICompositionDrawingSurfaceInterop::BeginDraw",
                hr: e.code().0,
            }
        })?;
    Ok((dc, offset))
}

/// `EndDraw`, paired with `begin_surface_draw`.
///
/// # Errors
/// When `EndDraw` fails.
pub fn end_surface_draw(surface: &CompositionDrawingSurface) -> Result<()> {
    let interop: ICompositionDrawingSurfaceInterop =
        surface.cast().map_err(|e| PlatformError::BadHr {
            operation: "CompositionDrawingSurface::cast<ICompositionDrawingSurfaceInterop>",
            hr: e.code().0,
        })?;
    // SAFETY: paired with begin_surface_draw.
    unsafe { interop.EndDraw() }.map_err(|e| PlatformError::BadHr {
        operation: "ICompositionDrawingSurfaceInterop::EndDraw",
        hr: e.code().0,
    })
}
