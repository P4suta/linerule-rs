//! WinRT `Windows.UI.Composition` をホストする overlay コンポジション基盤。
//!
//! Win32 DirectComposition (`graphics.rs`) の代替経路。`Compositor` +
//! `CreateDesktopWindowTarget` で overlay HWND を target にし、`SpriteVisual` +
//! `CompositionColorBrush` で dim/indicator を、`CompositionDrawingSurface` で
//! HUD を描く。`CreateBackdropBrush` + Gaussian blur effect で周囲ブラーを行う
//! (Win32 DComp 単体では backdrop blur ができないため)。
//!
//! このファイルは WinRT interop (`ICompositorDesktopInterop` 等) と
//! `CreateDispatcherQueueController` の unsafe を吸収する。WinRT の object メソッド
//! 呼び出し自体は windows crate では safe なので、毎フレームの visual 操作は
//! `winrt_composition_renderer` / `winrt_hud_renderer` (`#![forbid(unsafe_code)]`)
//! 側に置く。

#![allow(
    unsafe_code,
    reason = "FFI 境界。WinRT interop (ICompositorDesktopInterop / ICompositorInterop / \
              ICompositionDrawingSurfaceInterop) と DispatcherQueue 生成は windows crate でも unsafe。"
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
use crate::win32_ffi::graphics::{self, D2dStack};

thread_local! {
    /// UI thread の DispatcherQueueController。WinRT Composition の commit を駆動する。
    /// thread につき 1 つだけ生成でき、再生成はエラーになるので thread_local に 1 度
    /// だけ確立して以降の pipeline 再構築 (device-lost rebuild) でも使い回す。thread
    /// 終了時に Drop される。
    static DISPATCHER_QUEUE: OnceCell<windows::System::DispatcherQueueController> =
        const { OnceCell::new() };
}

/// この thread に DispatcherQueue を 1 度だけ確立する。既にあれば no-op。
///
/// `DQTAT_COM_STA`: この thread は COM apartment 未初期化なので、controller に STA
/// apartment を確立させる (WinRT Compositor 生成に必須)。
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
        // SAFETY: options は documented zero/enum 値。current thread に queue を作る。
        let controller = unsafe { CreateDispatcherQueueController(options) }.map_err(|e| {
            PlatformError::BadHr {
                operation: "CreateDispatcherQueueController",
                hr: e.code().0,
            }
        })?;
        let _ = cell.set(controller);
        Ok(())
    })
}

/// WinRT composition host。`Compositor` と desktop target、共有 D2D スタック、
/// HUD 用 graphics device を保持する。Drop で WinRT object が Release される。
pub struct WinrtPipeline {
    /// WinRT compositor。visual / brush / surface の生成元。
    pub compositor: Compositor,
    /// overlay HWND に紐づく desktop window target。Drop で切り離される。
    #[allow(dead_code, reason = "target の寿命を pipeline と同期させる owner")]
    target: DesktopWindowTarget,
    /// root container visual。直接子は overlay_root と hud_root のみ。
    #[allow(dead_code, reason = "root の寿命を pipeline と同期させる owner")]
    root: ContainerVisual,
    /// overlay slit (dim + indicator) 用の中間 visual。
    pub overlay_root: ContainerVisual,
    /// HUD パネル用の中間 visual。overlay_root より前面。
    pub hud_root: ContainerVisual,
    /// HUD の D2D 描画面を作る graphics device。
    pub graphics_device: CompositionGraphicsDevice,
    /// 共有 D2D スタック。`graphics_device` 等が内部参照するので alive に保つ。
    #[allow(dead_code, reason = "D2D デバイス一式を pipeline 寿命中 alive に保つ")]
    stack: D2dStack,
}

/// overlay HWND に WinRT composition tree を attach する。
///
/// # Errors
/// DispatcherQueue / Compositor / interop / D2D stack のいずれかの生成に失敗したとき。
pub fn create_winrt_pipeline(hwnd: HWND) -> Result<WinrtPipeline> {
    // 1. UI thread に DispatcherQueue を 1 度だけ立てる (既存 GetMessage ループで drain)。
    ensure_dispatcher_queue()?;

    // 2. Compositor。
    let compositor = Compositor::new().map_err(|e| PlatformError::BadHr {
        operation: "Compositor::new",
        hr: e.code().0,
    })?;

    // 3. desktop window target (interop)。
    let desktop_interop: ICompositorDesktopInterop =
        compositor.cast().map_err(|e| PlatformError::BadHr {
            operation: "Compositor::cast<ICompositorDesktopInterop>",
            hr: e.code().0,
        })?;
    // SAFETY: hwnd は valid (OverlayWindow 由来)。istopmost=true で最前面合成。
    let target: DesktopWindowTarget = unsafe {
        desktop_interop.CreateDesktopWindowTarget(hwnd, true)
    }
    .map_err(|e| PlatformError::BadHr {
        operation: "ICompositorDesktopInterop::CreateDesktopWindowTarget",
        hr: e.code().0,
    })?;

    // 4. root container visual を target に設定 (window 全体を覆う)。
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

    // 5. overlay_root → hud_root の固定順で子を入れる (後入れが前面)。
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

    // 6. HUD 用 graphics device (D2D デバイスを WinRT に橋渡し)。
    let stack = graphics::create_d2d_stack()?;
    let comp_interop: ICompositorInterop = compositor.cast().map_err(|e| PlatformError::BadHr {
        operation: "Compositor::cast<ICompositorInterop>",
        hr: e.code().0,
    })?;
    // SAFETY: d2d_device は valid な rendering device。
    let graphics_device =
        unsafe { comp_interop.CreateGraphicsDevice(&stack.d2d_device) }.map_err(|e| {
            PlatformError::BadHr {
                operation: "ICompositorInterop::CreateGraphicsDevice",
                hr: e.code().0,
            }
        })?;

    Ok(WinrtPipeline {
        compositor,
        target,
        root,
        overlay_root,
        hud_root,
        graphics_device,
        stack,
    })
}

/// window 全体を覆う `ContainerVisual` を作る (RelativeSizeAdjustment = 1.0)。
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

/// HUD 用の D2D 描画面を作る。BGRA premultiplied 固定。
///
/// # Errors
/// `CreateDrawingSurface` が失敗したとき。
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

/// `CompositionDrawingSurface` に D2D 描画を開始する。返る `ID2D1DeviceContext`
/// に直接描画コマンドを発行し、最後に [`end_surface_draw`] を呼ぶ。`offset` は
/// surface tile 内の左上座標 (`SetTransform` の translation に反映すること)。
///
/// # Errors
/// `BeginDraw` が失敗したとき。
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
    // SAFETY: interop は valid。iid に ID2D1DeviceContext を渡し、offset は out param。
    let dc: ID2D1DeviceContext =
        unsafe { interop.BeginDraw(update_rect, &mut offset) }.map_err(|e| {
            PlatformError::BadHr {
                operation: "ICompositionDrawingSurfaceInterop::BeginDraw",
                hr: e.code().0,
            }
        })?;
    Ok((dc, offset))
}

/// [`begin_surface_draw`] と pair で呼ぶ `EndDraw`。
///
/// # Errors
/// `EndDraw` が失敗したとき。
pub fn end_surface_draw(surface: &CompositionDrawingSurface) -> Result<()> {
    let interop: ICompositionDrawingSurfaceInterop =
        surface.cast().map_err(|e| PlatformError::BadHr {
            operation: "CompositionDrawingSurface::cast<ICompositionDrawingSurfaceInterop>",
            hr: e.code().0,
        })?;
    // SAFETY: begin_surface_draw と pair。
    unsafe { interop.EndDraw() }.map_err(|e| PlatformError::BadHr {
        operation: "ICompositionDrawingSurfaceInterop::EndDraw",
        hr: e.code().0,
    })
}
