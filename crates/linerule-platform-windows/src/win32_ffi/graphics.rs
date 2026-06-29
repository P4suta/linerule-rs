//! Safe wrapper over the D3D11 + DXGI + D2D device stack (Windows-only).
//!
//! Confines COM-creation `unsafe` here so composition / renderer code stays `#![forbid(unsafe_code)]`.

#![allow(
    unsafe_code,
    reason = "FFI boundary; D3D11/DXGI/D2D COM APIs are all unsafe in the windows crate."
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

/// The D3D11 + DXGI + D2D device set; shared stack the WinRT composition host
/// sits on.
pub struct D2dStack {
    /// D3D11 device (BGRA + hardware).
    pub d3d11: ID3D11Device,
    /// `IDXGIDevice` view.
    pub dxgi: IDXGIDevice,
    /// D2D1 factory (single-threaded).
    pub d2d_factory: ID2D1Factory1,
    /// D2D device.
    pub d2d_device: ID2D1Device,
    /// D2D device context.
    pub d2d_context: ID2D1DeviceContext,
}

/// Creates D3D11 → DXGI → D2D factory → D2D device → D2D context.
///
/// # Errors
/// When D3D11 / DXGI / D2D creation fails.
pub fn create_d2d_stack() -> Result<D2dStack> {
    let mut d3d11: Option<ID3D11Device> = None;
    // SAFETY: output is Option<> (null allowed); the flag combination is documented.
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
    // SAFETY: factory_options is zero-init OK; returns Result<ID2D1Factory1>.
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

    // SAFETY: dxgi is a valid IDXGIDevice.
    let d2d_device: ID2D1Device =
        unsafe { d2d_factory.CreateDevice(&dxgi) }.map_err(|e| PlatformError::BadHr {
            operation: "ID2D1Factory1::CreateDevice",
            hr: e.code().0,
        })?;

    // SAFETY: d2d_device is valid.
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
