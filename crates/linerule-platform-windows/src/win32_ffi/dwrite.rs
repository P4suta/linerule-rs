//! Safe wrappers over `IDWriteFactory` / `IDWriteTextFormat` /
//! `ID2D1DeviceContext::DrawText`.
//!
//! Called from `winrt_hud_renderer.rs`. Confines the `unsafe` boundary here.

#![allow(
    unsafe_code,
    reason = "FFI boundary; DWrite/D2D COM APIs are all unsafe in the windows crate."
)]

use linerule_core::Rgba;
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
/// `weight = SemiBold` (slightly bold, for titles) / style = Normal / stretch =
/// Normal / locale = "en-us" (HUD labels are ASCII-only).
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

/// One row's draw instruction (value type passing HUD layout to `draw_hud_rows`).
///
/// Borrow-based (`&str` / `&IDWriteTextFormat`) so the caller can build a Vec for
/// one HUD frame and submit it in one call.
pub struct HudDrawRow<'a> {
    /// Surface-local draw rect (logical px, from the surface origin).
    pub rect: D2D_RECT_F,
    /// Text to draw.
    pub text: &'a str,
    /// Text format to apply (from `create_text_format` for HudFontKey + font_size).
    pub format: &'a IDWriteTextFormat,
    /// Text color (straight alpha).
    pub color: Rgba,
}

/// One non-text fill rect (divider etc.) draw instruction.
pub struct HudDrawRule {
    /// Surface-local fill rect (logical px, from the surface origin).
    pub rect: D2D_RECT_F,
    /// Fill color (straight alpha).
    pub color: Rgba,
}

/// Issues "transparent clear + rounded panel fill + rule fills + text rows" on
/// an `ID2D1DeviceContext` whose drawing session is already open, applying the
/// surface tile `offset` via `SetTransform`. begin/end are the caller's
/// responsibility.
///
/// The background is painted as a rounded fill of the `panel` rect (not a
/// full-surface `Clear`): the area outside the corners stays transparent, so
/// the overlay shows through (Fluent style).
///
/// `opacity` (0.0–1.0) multiplies into the alpha of the background, rules, and
/// every row color.
///
/// # Errors
/// When brush creation fails.
#[allow(
    clippy::too_many_arguments,
    reason = "one draw call per HUD frame; the args mirror the HudFrame fields \
              and a grouping struct would just restate them"
)]
pub fn draw_hud_rows(
    dc: &ID2D1DeviceContext,
    offset: windows::Win32::Foundation::POINT,
    background: Rgba,
    panel: D2D_RECT_F,
    corner_radius: f32,
    opacity: f32,
    rules: &[HudDrawRule],
    rows: &[HudDrawRow<'_>],
) -> Result<()> {
    let opacity = opacity.clamp(0.0, 1.0);
    let transparent = D2D1_COLOR_F {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };
    let bg = color_to_premultiplied_f(scale_alpha(background, opacity));
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

        let bg_brush: ID2D1SolidColorBrush =
            dc.CreateSolidColorBrush(&bg, None)
                .map_err(|e| PlatformError::BadHr {
                    operation: "ID2D1DeviceContext::CreateSolidColorBrush (HUD bg)",
                    hr: e.code().0,
                })?;
        dc.FillRoundedRectangle(
            &D2D1_ROUNDED_RECT {
                rect: panel,
                radiusX: corner_radius,
                radiusY: corner_radius,
            },
            &bg_brush,
        );

        for rule in rules {
            let rule_color = color_to_premultiplied_f(scale_alpha(rule.color, opacity));
            let brush: ID2D1SolidColorBrush =
                dc.CreateSolidColorBrush(&rule_color, None)
                    .map_err(|e| PlatformError::BadHr {
                        operation: "ID2D1DeviceContext::CreateSolidColorBrush (HUD rule)",
                        hr: e.code().0,
                    })?;
            dc.FillRectangle(&rule.rect, &brush);
        }

        for row in rows {
            let brush_color = color_to_premultiplied_f(scale_alpha(row.color, opacity));
            let brush: ID2D1SolidColorBrush = dc
                .CreateSolidColorBrush(&brush_color, None)
                .map_err(|e| PlatformError::BadHr {
                    operation: "ID2D1DeviceContext::CreateSolidColorBrush (HUD)",
                    hr: e.code().0,
                })?;
            let wide: Vec<u16> = row.text.encode_utf16().collect();
            dc.DrawText(
                &wide,
                row.format,
                &row.rect,
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

/// Multiplies `Rgba::a` by `factor` (0.0–1.0). Bakes HUD frame opacity into each
/// color's alpha instead of using a dcomp visual's opacity.
fn scale_alpha(color: Rgba, factor: f32) -> Rgba {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "factor is clamped 0..=1; (u8 * f32) -> u8 floors explicitly"
    )]
    let a = (f32::from(color.a) * factor).clamp(0.0, 255.0) as u8;
    Rgba { a, ..color }
}
