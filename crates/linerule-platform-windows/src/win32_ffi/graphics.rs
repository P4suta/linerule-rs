//! D3D11 + DXGI + D2D + DirectComposition の薄い safe wrapper。
//!
//! Phase D で `linerule-platform-windows/composition_renderer.rs` から呼ばれる。
//! ここで COM オブジェクト型 (windows crate の `IDCompositionDesktopDevice` 等)
//! を保持・操作する unsafe を全部吸収し、composition_renderer は
//! `#![forbid(unsafe_code)]` で safe な状態遷移だけ書く。
//!
//! Windows-only。Linux 上では `cfg(target_os = "windows")` でビルドされない。

#![allow(
    unsafe_code,
    reason = "FFI 境界。D3D11 / DXGI / D2D / DComposition の各 COM API は\
              windows crate でも全部 unsafe。ADR-0003 で集約。"
)]

use linerule_core::Rgba;
use windows::Foundation::Numerics::Matrix3x2;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_POINT_2F, D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1,
    D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_FACTORY_OPTIONS, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1CreateFactory, ID2D1Device, ID2D1DeviceContext, ID2D1Factory1,
};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_11_0,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice2, IDCompositionDesktopDevice, IDCompositionDevice,
    IDCompositionSurface, IDCompositionTarget, IDCompositionVisual2,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED as DXGI_ALPHA_MODE_PREMULTIPLIED_BC, DXGI_FORMAT_B8G8R8A8_UNORM,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::core::Interface;

use crate::error::{Result, Win32Error};

/// D3D11 + DXGI + D2D + DComp の生 COM ハンドル束。 `CompositionRenderer` から
/// 所有される。 各フィールドは `windows` crate の COM type で、`Drop` で
/// 自動 Release される。
pub struct DcompPipeline {
    pub d3d11: ID3D11Device,
    pub dxgi: IDXGIDevice,
    pub d2d_factory: ID2D1Factory1,
    pub d2d_device: ID2D1Device,
    pub d2d_context: ID2D1DeviceContext,
    pub dcomp: IDCompositionDesktopDevice,
    pub target: IDCompositionTarget,
    pub root: IDCompositionVisual2,
}

/// Overlay HWND に dcomp visual tree を attach するパイプライン生成。
///
/// 流れ:
/// 1. `D3D11CreateDevice(HARDWARE, BGRA_SUPPORT)` で BGRA 対応の D3D11 デバイスを得る
/// 2. それを `IDXGIDevice` にキャスト
/// 3. `D2D1CreateFactory::<ID2D1Factory1>()` でファクトリ
/// 4. `factory.CreateDevice(&dxgi)` で D2D デバイス
/// 5. `d2d_device.CreateDeviceContext(NONE)` で D2D コンテキスト
/// 6. `DCompositionCreateDevice2::<IDCompositionDevice2>(d2d_device)` でコンポジションデバイス、
///    `cast::<IDCompositionDesktopDevice>()` でデスクトップ機能を取得
/// 7. `CreateTargetForHwnd(hwnd, topmost=true)` でターゲット
/// 8. `CreateVisual()` でルートビジュアル
/// 9. `target.SetRoot(root)` で連結
pub fn create_dcomp_pipeline(hwnd: HWND) -> Result<DcompPipeline> {
    // 1. D3D11
    let mut d3d11: Option<ID3D11Device> = None;
    // SAFETY: 出力は Option<>、null も許容。flags の組み合わせは MSDN documented。
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            None,
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_1]),
            D3D11_SDK_VERSION,
            Some(&mut d3d11),
            None,
            None,
        )
    }
    .map_err(|e| Win32Error::BadHr {
        operation: "D3D11CreateDevice",
        hr: e.code().0,
    })?;
    let d3d11 = d3d11.ok_or(Win32Error::NullHandle {
        operation: "D3D11CreateDevice (out param null)",
    })?;

    // 2. DXGI
    let dxgi: IDXGIDevice = d3d11.cast().map_err(|e| Win32Error::BadHr {
        operation: "ID3D11Device::cast::<IDXGIDevice>",
        hr: e.code().0,
    })?;

    // 3. D2D factory
    let factory_options = D2D1_FACTORY_OPTIONS::default();
    // SAFETY: factory_options は zero-init OK、戻り値は Result<ID2D1Factory1>
    let d2d_factory: ID2D1Factory1 = unsafe {
        D2D1CreateFactory::<ID2D1Factory1>(
            D2D1_FACTORY_TYPE_SINGLE_THREADED,
            Some(&factory_options),
        )
    }
    .map_err(|e| Win32Error::BadHr {
        operation: "D2D1CreateFactory",
        hr: e.code().0,
    })?;

    // 4. D2D device
    // SAFETY: dxgi は valid IDXGIDevice
    let d2d_device: ID2D1Device =
        unsafe { d2d_factory.CreateDevice(&dxgi) }.map_err(|e| Win32Error::BadHr {
            operation: "ID2D1Factory1::CreateDevice",
            hr: e.code().0,
        })?;

    // 5. D2D context
    // SAFETY: d2d_device は valid
    let d2d_context: ID2D1DeviceContext =
        unsafe { d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE) }.map_err(
            |e| Win32Error::BadHr {
                operation: "ID2D1Device::CreateDeviceContext",
                hr: e.code().0,
            },
        )?;

    // 6. DComp device → DesktopDevice
    // SAFETY: d2d_device を rendering device として渡す
    let dcomp_dev: IDCompositionDevice = unsafe { DCompositionCreateDevice2(&d2d_device) }
        .map_err(|e| Win32Error::BadHr {
            operation: "DCompositionCreateDevice2",
            hr: e.code().0,
        })?;
    let dcomp: IDCompositionDesktopDevice = dcomp_dev.cast().map_err(|e| Win32Error::BadHr {
        operation: "IDCompositionDevice::cast::<IDCompositionDesktopDevice>",
        hr: e.code().0,
    })?;

    // 7. Target for HWND
    // SAFETY: hwnd は valid (OverlayWindow::new で取得済み)
    let target: IDCompositionTarget =
        unsafe { dcomp.CreateTargetForHwnd(hwnd, true) }.map_err(|e| Win32Error::BadHr {
            operation: "IDCompositionDesktopDevice::CreateTargetForHwnd",
            hr: e.code().0,
        })?;

    // 8. Root visual
    let root = create_visual(&dcomp)?;

    // 9. Link target → root
    // SAFETY: target / root は同じ device 由来で valid
    unsafe { target.SetRoot(&root) }.map_err(|e| Win32Error::BadHr {
        operation: "IDCompositionTarget::SetRoot",
        hr: e.code().0,
    })?;

    Ok(DcompPipeline {
        d3d11,
        dxgi,
        d2d_factory,
        d2d_device,
        d2d_context,
        dcomp,
        target,
        root,
    })
}

/// 1 つのレイヤを表す `IDCompositionVisual2` を作る。
pub fn create_visual(device: &IDCompositionDesktopDevice) -> Result<IDCompositionVisual2> {
    // SAFETY: device は valid
    unsafe { device.CreateVisual() }.map_err(|e| Win32Error::BadHr {
        operation: "IDCompositionDesktopDevice::CreateVisual",
        hr: e.code().0,
    })
}

/// レイヤの平行移動を設定する。
pub fn visual_set_offset(visual: &IDCompositionVisual2, x: f32, y: f32) -> Result<()> {
    // SAFETY: visual は valid、引数は plain f32
    unsafe {
        visual.SetOffsetX2(x).map_err(|e| Win32Error::BadHr {
            operation: "IDCompositionVisual2::SetOffsetX",
            hr: e.code().0,
        })?;
        visual.SetOffsetY2(y).map_err(|e| Win32Error::BadHr {
            operation: "IDCompositionVisual2::SetOffsetY",
            hr: e.code().0,
        })?;
    }
    Ok(())
}

/// レイヤに描画内容（`IDCompositionSurface`）を接続する。
pub fn visual_set_content(
    visual: &IDCompositionVisual2,
    surface: Option<&IDCompositionSurface>,
) -> Result<()> {
    // SAFETY: visual は valid、surface は valid な COM ハンドル or None
    unsafe { visual.SetContent(surface) }.map_err(|e| Win32Error::BadHr {
        operation: "IDCompositionVisual2::SetContent",
        hr: e.code().0,
    })
}

/// レイヤを root のチャイルドリストに追加する。
pub fn root_add_visual(root: &IDCompositionVisual2, child: &IDCompositionVisual2) -> Result<()> {
    // SAFETY: 両者とも同じ device 由来の有効ハンドル
    unsafe { root.AddVisual(child, false, None) }.map_err(|e| Win32Error::BadHr {
        operation: "IDCompositionVisual2::AddVisual",
        hr: e.code().0,
    })
}

/// レイヤを root から削除する。
pub fn root_remove_visual(root: &IDCompositionVisual2, child: &IDCompositionVisual2) -> Result<()> {
    // SAFETY: 両者とも有効ハンドル
    unsafe { root.RemoveVisual(child) }.map_err(|e| Win32Error::BadHr {
        operation: "IDCompositionVisual2::RemoveVisual",
        hr: e.code().0,
    })
}

/// `IDCompositionSurface` を新規作成する。`BGRA_UNORM` + premultiplied alpha 固定。
pub fn create_surface(
    device: &IDCompositionDesktopDevice,
    width: u32,
    height: u32,
) -> Result<IDCompositionSurface> {
    // SAFETY: device valid、width/height u32
    unsafe {
        device.CreateSurface(
            width,
            height,
            DXGI_FORMAT_B8G8R8A8_UNORM,
            DXGI_ALPHA_MODE_PREMULTIPLIED_BC,
        )
    }
    .map_err(|e| Win32Error::BadHr {
        operation: "IDCompositionDesktopDevice::CreateSurface",
        hr: e.code().0,
    })
}

/// 既存 surface を指定色で塗りつぶす。`BeginDraw` → `Clear` → `EndDraw` の三段。
pub fn fill_surface(
    surface: &IDCompositionSurface,
    d2d_context: &ID2D1DeviceContext,
    color: Rgba,
) -> Result<()> {
    // SAFETY: surface / context は valid
    let mut offset = windows::Win32::Foundation::POINT::default();
    let bitmap: windows::Win32::Graphics::Direct2D::ID2D1Bitmap1 =
        unsafe { surface.BeginDraw(None, &mut offset) }.map_err(|e| Win32Error::BadHr {
            operation: "IDCompositionSurface::BeginDraw",
            hr: e.code().0,
        })?;

    // Premultiplied alpha:
    let f = color_to_premultiplied_f(color);
    unsafe {
        d2d_context.SetTarget(&bitmap);
        // BeginDraw on context to bind the bitmap as the render target
        d2d_context.BeginDraw();
        // Translate so the surface offset is honoured
        d2d_context.SetTransform(&Matrix3x2 {
            M11: 1.0,
            M12: 0.0,
            M21: 0.0,
            M22: 1.0,
            M31: offset.x as f32,
            M32: offset.y as f32,
        });
        d2d_context.Clear(Some(&f));
        // EndDraw returns Result<()> on errors
        d2d_context
            .EndDraw(None, None)
            .map_err(|e| Win32Error::BadHr {
                operation: "ID2D1DeviceContext::EndDraw",
                hr: e.code().0,
            })?;
        // Detach target
        d2d_context.SetTarget(None);
        surface.EndDraw().map_err(|e| Win32Error::BadHr {
            operation: "IDCompositionSurface::EndDraw",
            hr: e.code().0,
        })?;
    }
    Ok(())
}

/// `IDCompositionDesktopDevice::Commit` で visual tree をディスプレイに反映する。
pub fn commit(device: &IDCompositionDesktopDevice) -> Result<()> {
    // SAFETY: device valid
    unsafe { device.Commit() }.map_err(|e| Win32Error::BadHr {
        operation: "IDCompositionDesktopDevice::Commit",
        hr: e.code().0,
    })
}

/// `[0, 255]` straight alpha の `Rgba` を D2D の premultiplied float に変換する。
fn color_to_premultiplied_f(color: Rgba) -> D2D1_COLOR_F {
    let a = f32::from(color.a) / 255.0;
    let r = (f32::from(color.r) / 255.0) * a;
    let g = (f32::from(color.g) / 255.0) * a;
    let b = (f32::from(color.b) / 255.0) * a;
    D2D1_COLOR_F { r, g, b, a }
}

// 警告抑止: D2D 型の一部はまだ使っていない（Phase D の発展用）
const _: D2D1_PIXEL_FORMAT = D2D1_PIXEL_FORMAT {
    format: DXGI_FORMAT_B8G8R8A8_UNORM,
    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
};
const _: D2D1_BITMAP_PROPERTIES1 = D2D1_BITMAP_PROPERTIES1 {
    pixelFormat: D2D1_PIXEL_FORMAT {
        format: DXGI_FORMAT_B8G8R8A8_UNORM,
        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
    },
    dpiX: 96.0,
    dpiY: 96.0,
    bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET,
    colorContext: core::mem::ManuallyDrop::new(None),
};
const _: D2D_RECT_F = D2D_RECT_F {
    left: 0.0,
    top: 0.0,
    right: 0.0,
    bottom: 0.0,
};
const _: D2D_POINT_2F = D2D_POINT_2F { x: 0.0, y: 0.0 };
const _: D2D1_BITMAP_OPTIONS_CANNOT_DRAW = D2D1_BITMAP_OPTIONS_CANNOT_DRAW;
