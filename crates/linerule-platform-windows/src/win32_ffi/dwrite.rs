//! Safe wrappers over `IDWriteFactory` / `IDWriteTextFormat` /
//! `ID2D1DeviceContext::DrawText`.
//!
//! Called from `winrt_hud_renderer.rs`. Confines the `unsafe` boundary here.

#![allow(
    unsafe_code,
    reason = "FFI boundary; DWrite/D2D COM APIs are all unsafe in the windows crate."
)]

use linerule_core::{HudFrame, Rgba};
use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D1_COLOR_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ROUNDED_RECT, ID2D1DeviceContext, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_MEASURING_MODE_NATURAL,
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
};
use windows::core::HSTRING;
use windows_numerics::Matrix3x2;

use crate::error::{PlatformError, Result};

/// Creates an `IDWriteFactory` (`DWRITE_FACTORY_TYPE_SHARED`).
///
/// One per process suffices; called once in `HudRenderer::new`.
///
/// # Errors
/// When `DWriteCreateFactory` fails (very rare).
pub fn create_dwrite_factory() -> Result<IDWriteFactory> {
    // SAFETY: factory_type is a windows-rs enum; riid passes &IDWriteFactory::IID
    // correctly for the out param.
    let factory: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }
        .map_err(|e| PlatformError::BadHr {
            operation: "DWriteCreateFactory",
            hr: e.code().0,
        })?;
    Ok(factory)
}

/// Safe wrapper over `IDWriteFactory::CreateTextFormat`.
///
/// `bold` -> SemiBold; locale is "en-us" (HUD labels are ASCII-only).
///
/// # Errors
/// When the font family is missing or arguments are invalid.
pub fn create_text_format(
    factory: &IDWriteFactory,
    family_name: &str,
    font_size_dip: f32,
    bold: bool,
) -> Result<IDWriteTextFormat> {
    let family = HSTRING::from(family_name);
    let locale = HSTRING::from("en-us");
    let weight = if bold {
        DWRITE_FONT_WEIGHT_SEMI_BOLD
    } else {
        DWRITE_FONT_WEIGHT_NORMAL
    };
    // SAFETY: family / locale are valid PCWSTR (HSTRING borrows); size is plain f32.
    let format: IDWriteTextFormat = unsafe {
        factory.CreateTextFormat(
            &family,
            None,
            weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            font_size_dip,
            &locale,
        )
    }
    .map_err(|e| PlatformError::BadHr {
        operation: "IDWriteFactory::CreateTextFormat",
        hr: e.code().0,
    })?;
    Ok(format)
}

/// Draws transparent clear + rounded panel fill + rule fills + text rows on a
/// `dc` whose drawing session is already open (caller owns begin/end), applying
/// surface tile `offset` via `SetTransform`.
///
/// The caller-owned format and UTF-16 buffers are reused across frames, so a
/// steady-size HUD refresh does not allocate temporary vectors.
///
/// # Errors
/// When the format count does not match the rows or brush creation fails.
pub fn draw_hud_frame(
    dc: &ID2D1DeviceContext,
    offset: windows::Win32::Foundation::POINT,
    frame: &HudFrame,
    formats: &[IDWriteTextFormat],
    scratch_utf16: &mut Vec<u16>,
) -> Result<()> {
    if formats.len() != frame.rows.len() {
        return Err(PlatformError::Invariant {
            operation: "draw_hud_frame format/row length mismatch",
        });
    }
    let opacity = frame.opacity.clamp(0.0, 1.0);
    let transparent = D2D1_COLOR_F {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };
    let background = color_to_premultiplied_f(scale_alpha(frame.background, opacity));
    let panel = D2D_RECT_F {
        left: 0.0,
        top: 0.0,
        right: frame.panel_width,
        bottom: frame.panel_height,
    };
    // SAFETY: caller passes dc with its drawing session already open.
    unsafe {
        #[allow(
            clippy::cast_precision_loss,
            reason = "surface tile offset is usually < 4096; well within f32 precision"
        )]
        dc.SetTransform(&Matrix3x2 {
            M11: 1.0,
            M12: 0.0,
            M21: 0.0,
            M22: 1.0,
            M31: offset.x as f32,
            M32: offset.y as f32,
        });
        dc.Clear(Some(&transparent));

        let background_brush: ID2D1SolidColorBrush = dc
            .CreateSolidColorBrush(&background, None)
            .map_err(|e| PlatformError::BadHr {
                operation: "ID2D1DeviceContext::CreateSolidColorBrush (HUD bg)",
                hr: e.code().0,
            })?;
        dc.FillRoundedRectangle(
            &D2D1_ROUNDED_RECT {
                rect: panel,
                radiusX: frame.corner_radius,
                radiusY: frame.corner_radius,
            },
            &background_brush,
        );

        for rule in &frame.rules {
            let rule_color = color_to_premultiplied_f(scale_alpha(rule.color, opacity));
            let brush: ID2D1SolidColorBrush =
                dc.CreateSolidColorBrush(&rule_color, None)
                    .map_err(|e| PlatformError::BadHr {
                        operation: "ID2D1DeviceContext::CreateSolidColorBrush (HUD rule)",
                        hr: e.code().0,
                    })?;
            let left = rule.left - frame.panel_left;
            let top = rule.top - frame.panel_top;
            dc.FillRectangle(
                &D2D_RECT_F {
                    left,
                    top,
                    right: left + rule.width,
                    bottom: top + rule.height,
                },
                &brush,
            );
        }

        for (row, format) in frame.rows.iter().zip(formats) {
            let brush_color = color_to_premultiplied_f(scale_alpha(row.color, opacity));
            let brush: ID2D1SolidColorBrush = dc
                .CreateSolidColorBrush(&brush_color, None)
                .map_err(|e| PlatformError::BadHr {
                    operation: "ID2D1DeviceContext::CreateSolidColorBrush (HUD)",
                    hr: e.code().0,
                })?;
            scratch_utf16.clear();
            scratch_utf16.extend(row.text.encode_utf16());
            let left = row.origin_x - frame.panel_left;
            let top = row.origin_y - frame.panel_top;
            let rect = D2D_RECT_F {
                left,
                top,
                right: frame.panel_width,
                bottom: top + row.font_size * 1.5,
            };
            dc.DrawText(
                scratch_utf16,
                format,
                &rect,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }
    Ok(())
}

/// Converts a `[0, 255]` straight-alpha `Rgba` to a D2D premultiplied float color.
fn color_to_premultiplied_f(color: Rgba) -> D2D1_COLOR_F {
    let a = f32::from(color.a) / 255.0;
    let r = (f32::from(color.r) / 255.0) * a;
    let g = (f32::from(color.g) / 255.0) * a;
    let b = (f32::from(color.b) / 255.0) * a;
    D2D1_COLOR_F { r, g, b, a }
}

/// Multiplies `Rgba::a` by `factor` (0.0–1.0): bakes HUD opacity into alpha
/// rather than via a dcomp visual.
fn scale_alpha(color: Rgba, factor: f32) -> Rgba {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "factor is clamped 0..=1; (u8 * f32) -> u8 floors explicitly"
    )]
    let a = (f32::from(color.a) * factor).clamp(0.0, 255.0) as u8;
    Rgba { a, ..color }
}
