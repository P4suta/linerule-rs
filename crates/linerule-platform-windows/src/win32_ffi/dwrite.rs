//! `IDWriteFactory` / `IDWriteTextFormat` / `ID2D1DeviceContext::DrawText` の
//! 薄い safe wrapper。
//!
//! Phase G で `linerule-platform-windows/hud_renderer.rs` から呼ばれる。
//! `unsafe` の境界を `win32_ffi/dwrite.rs` 1 ファイルに集約する（ADR-0006）。

#![allow(
    unsafe_code,
    reason = "FFI 境界。DWrite / D2D の各 COM API は windows crate でも全部 unsafe。\
              ADR-0003 + ADR-0006 で集約。"
)]

use linerule_core::Rgba;
use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D1_COLOR_F};
use windows::Win32::Graphics::Direct2D::{D2D1_DRAW_TEXT_OPTIONS_NONE, ID2D1SolidColorBrush};
use windows::Win32::Graphics::DirectComposition::IDCompositionSurface;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_MEASURING_MODE_NATURAL,
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
};
use windows::core::HSTRING;
use windows_numerics::Matrix3x2;

use crate::error::{PlatformError, Result};

/// `IDWriteFactory` を新規作成する（`DWRITE_FACTORY_TYPE_SHARED`）。
///
/// プロセス内で 1 つだけ生成すれば十分。`HudRenderer::new` の初期化で 1 度呼ぶ。
///
/// # Errors
/// `DWriteCreateFactory` が失敗したとき (極めて稀)。
pub fn create_dwrite_factory() -> Result<IDWriteFactory> {
    // SAFETY: factory_type は windows-rs の enum、riid は &IDWriteFactory::IID を
    // out param 用に正しく渡す。
    let factory: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }
        .map_err(|e| PlatformError::BadHr {
            operation: "DWriteCreateFactory",
            hr: e.code().0,
        })?;
    Ok(factory)
}

/// `IDWriteFactory::CreateTextFormat` の薄い safe wrapper。
///
/// `weight = SemiBold` (title 用に少し太め) / style = Normal / stretch = Normal /
/// locale = "en-us" (HUD ラベルは英字のみのため CJK locale を意識しない)。
///
/// # Errors
/// font family が存在しない / 引数不正のとき。
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
    // SAFETY: family / locale は valid PCWSTR (HSTRING の借用)。size は plain f32。
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

/// 1 行分の描画指示（HUD レイアウトを `draw_hud_to_surface` に渡すための値型）。
///
/// 借用ベース（`&str` / `&IDWriteTextFormat`）にして HUD 1 frame 分の Vec を
/// caller 側で `let` で組み立ててから一括投入できるようにする。
pub struct HudDrawRow<'a> {
    /// surface-local の描画矩形（logical px / surface 原点起点）。
    pub rect: D2D_RECT_F,
    /// 描画するテキスト。
    pub text: &'a str,
    /// 適用する text format（HudFontKey + font_size から `create_text_format`
    /// で得たもの）。
    pub format: &'a IDWriteTextFormat,
    /// テキスト色（straight alpha）。
    pub color: Rgba,
}

/// `IDCompositionSurface` の中身を「背景クリア + 複数行テキスト描画」で更新する。
///
/// `BeginDraw (DComp) → BeginDraw (D2D) → Clear → DrawText× → EndDraw (D2D) →
/// EndDraw (DComp)` の標準シーケンスを 1 関数に閉じ込め、呼び出し側を
/// `#![forbid(unsafe_code)]` で書けるようにする。DComp surface tile を render
/// target に bind する責務は `begin_dcomp_draw_d2d` 側で完結するため、本関数では
/// 明示的な `SetTarget` 呼び出しを行わない (ADR-0006 + graphics::fill_surface 参照)。
///
/// `opacity` (0.0–1.0) は背景・各行色の alpha に乗算する形で適用される。dcomp の
/// visual 単位 opacity を使わない理由は `graphics.rs` のコメント参照。
///
/// # Errors
/// 各 COM 呼び出しが失敗したとき。
pub fn draw_hud_to_surface(
    surface: &IDCompositionSurface,
    background: Rgba,
    opacity: f32,
    rows: &[HudDrawRow<'_>],
) -> Result<()> {
    let opacity = opacity.clamp(0.0, 1.0);
    let (dc, offset) = crate::win32_ffi::graphics::begin_dcomp_draw_d2d(
        surface,
        "IDCompositionSurface::BeginDraw (HUD)",
    )?;

    let bg = color_to_premultiplied_f(scale_alpha(background, opacity));
    // SAFETY: dc / surface valid。Begin/End はペア。
    unsafe {
        dc.BeginDraw();
        #[allow(
            clippy::cast_precision_loss,
            reason = "DComp offset は通常 < 4096; f32 精度に余裕"
        )]
        dc.SetTransform(&Matrix3x2 {
            M11: 1.0,
            M12: 0.0,
            M21: 0.0,
            M22: 1.0,
            M31: offset.x as f32,
            M32: offset.y as f32,
        });
        dc.Clear(Some(&bg));

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

        dc.EndDraw(None, None).map_err(|e| PlatformError::BadHr {
            operation: "ID2D1DeviceContext::EndDraw (HUD)",
            hr: e.code().0,
        })?;
    }
    crate::win32_ffi::graphics::end_dcomp_draw(surface, "IDCompositionSurface::EndDraw (HUD)")
}

/// `[0, 255]` straight alpha の `Rgba` を D2D premultiplied float に変換する。
fn color_to_premultiplied_f(color: Rgba) -> D2D1_COLOR_F {
    let a = f32::from(color.a) / 255.0;
    let r = (f32::from(color.r) / 255.0) * a;
    let g = (f32::from(color.g) / 255.0) * a;
    let b = (f32::from(color.b) / 255.0) * a;
    D2D1_COLOR_F { r, g, b, a }
}

/// `Rgba::a` に `factor (0.0–1.0)` を乗算する。dcomp visual の opacity を使わず、
/// HUD frame の opacity を各色の alpha に bake するためのヘルパ。
fn scale_alpha(color: Rgba, factor: f32) -> Rgba {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "factor は 0..=1 clamp 済み、(u8 * f32) → u8 は明示的に floor"
    )]
    let a = (f32::from(color.a) * factor).clamp(0.0, 255.0) as u8;
    Rgba { a, ..color }
}
