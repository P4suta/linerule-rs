//! D3D11 + DXGI + D2D デバイススタックの薄い safe wrapper。
//!
//! WinRT composition host (`win32_ffi::composition`) がこのスタックの上に乗る。
//! COM オブジェクト生成の `unsafe` をこのファイルに吸収し、composition / renderer
//! 側は `#![forbid(unsafe_code)]` で safe な状態遷移だけ書けるようにする。
//!
//! Windows-only。Linux 上では `cfg(target_os = "windows")` でビルドされない。

#![allow(
    unsafe_code,
    reason = "FFI 境界。D3D11 / DXGI / D2D の各 COM API は windows crate でも全部 unsafe。"
)]

use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct2D::{
    D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_FACTORY_OPTIONS, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1CreateFactory, ID2D1Device, ID2D1DeviceContext, ID2D1Factory1,
};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_11_0,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::core::Interface;

use crate::error::{PlatformError, Result};

/// D3D11 + DXGI + D2D デバイス一式。WinRT composition host がこの上に乗る共有スタック。
pub struct D2dStack {
    /// D3D11 デバイス (BGRA + ハードウェア)。
    pub d3d11: ID3D11Device,
    /// `IDXGIDevice` view。
    pub dxgi: IDXGIDevice,
    /// D2D1 ファクトリ (single-threaded)。
    pub d2d_factory: ID2D1Factory1,
    /// D2D デバイス。
    pub d2d_device: ID2D1Device,
    /// D2D デバイスコンテキスト。
    pub d2d_context: ID2D1DeviceContext,
}

/// D3D11 → DXGI → D2D factory → D2D device → D2D context を生成する。
///
/// # Errors
/// D3D11 / DXGI / D2D のいずれかの生成に失敗したとき。
pub fn create_d2d_stack() -> Result<D2dStack> {
    let mut d3d11: Option<ID3D11Device> = None;
    // SAFETY: 出力は Option<>、null も許容。flags の組み合わせは MSDN documented。
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_1]),
            D3D11_SDK_VERSION,
            Some(&mut d3d11),
            None,
            None,
        )
    }
    .map_err(|e| PlatformError::BadHr {
        operation: "D3D11CreateDevice",
        hr: e.code().0,
    })?;
    let d3d11 = d3d11.ok_or(PlatformError::NullHandle {
        operation: "D3D11CreateDevice (out param null)",
    })?;

    let dxgi: IDXGIDevice = d3d11.cast().map_err(|e| PlatformError::BadHr {
        operation: "ID3D11Device::cast::<IDXGIDevice>",
        hr: e.code().0,
    })?;

    let factory_options = D2D1_FACTORY_OPTIONS::default();
    // SAFETY: factory_options は zero-init OK、戻り値は Result<ID2D1Factory1>
    let d2d_factory: ID2D1Factory1 = unsafe {
        D2D1CreateFactory::<ID2D1Factory1>(
            D2D1_FACTORY_TYPE_SINGLE_THREADED,
            Some(&factory_options),
        )
    }
    .map_err(|e| PlatformError::BadHr {
        operation: "D2D1CreateFactory",
        hr: e.code().0,
    })?;

    // SAFETY: dxgi は valid IDXGIDevice
    let d2d_device: ID2D1Device =
        unsafe { d2d_factory.CreateDevice(&dxgi) }.map_err(|e| PlatformError::BadHr {
            operation: "ID2D1Factory1::CreateDevice",
            hr: e.code().0,
        })?;

    // SAFETY: d2d_device は valid
    let d2d_context: ID2D1DeviceContext =
        unsafe { d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE) }.map_err(
            |e| PlatformError::BadHr {
                operation: "ID2D1Device::CreateDeviceContext",
                hr: e.code().0,
            },
        )?;

    Ok(D2dStack {
        d3d11,
        dxgi,
        d2d_factory,
        d2d_device,
        d2d_context,
    })
}
