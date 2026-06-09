//! WinRT Composition 用の自作 Gaussian blur エフェクト記述。
//!
//! WinRT には `IGraphicsEffect` を実装した Gaussian blur クラスが Win2D (UWP 専用)
//! にしか無いため、`IGraphicsEffectD2D1Interop` を自前実装し、`CLSID_D2D1GaussianBlur`
//! と StandardDeviation プロパティ・single source を記述する。これを
//! `Compositor::CreateEffectFactory` に渡し、`CompositionBackdropBrush` を source に
//! 差し込むことで「窓の背後をぼかす」effect brush を得る。

#![allow(
    unsafe_code,
    reason = "FFI 境界。`#[implement]` の COM vtable と PropertyValue boxing を含む。"
)]
#![allow(
    trivial_casts,
    reason = "windows-rs `#[implement]` マクロ展開が生成する vtable cast 由来。"
)]

use std::cell::RefCell;

use windows::Foundation::{IPropertyValue, PropertyValue};
use windows::Graphics::Effects::{
    IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectSource_Impl,
};
use windows::UI::Composition::{CompositionBrush, CompositionEffectSourceParameter, Compositor};
use windows::Win32::Graphics::Direct2D::Common::D2D1_BORDER_MODE_HARD;
use windows::Win32::Graphics::Direct2D::{
    CLSID_D2D1GaussianBlur, D2D1_GAUSSIANBLUR_OPTIMIZATION_BALANCED,
};
use windows::Win32::System::WinRT::Graphics::Direct2D::{
    GRAPHICS_EFFECT_PROPERTY_MAPPING, GRAPHICS_EFFECT_PROPERTY_MAPPING_DIRECT,
    IGraphicsEffectD2D1Interop, IGraphicsEffectD2D1Interop_Impl,
};
use windows::core::{Error, GUID, HSTRING, Interface, PCWSTR, Result as WinResult, implement};

use crate::error::{PlatformError, Result};

/// CompositionEffectFactory に渡す source パラメータ名。`SetSourceParameter` でも
/// 同名で backdrop を差し込む。
const SOURCE_NAME: &str = "source";

/// `CompositionBackdropBrush` を 1 source に取り、Gaussian blur をかける effect 記述。
///
/// `CreateEffectFactory` は D2D1GaussianBlur の登録スキーマ (プロパティ 3 個: index 0
/// StandardDeviation / 1 Optimization / 2 BorderMode) と数・型が一致することを検証し、
/// 不一致だと `E_INVALIDARG` を返す。よって 3 プロパティすべてを正しい型で公開する
/// (1 個しか宣言しないと CreateEffectFactory が E_INVALIDARG で失敗する)。
#[implement(IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectD2D1Interop)]
struct GaussianBlurEffect {
    name: RefCell<HSTRING>,
    standard_deviation: f32,
    source: IGraphicsEffectSource,
}

// IGraphicsEffect は IGraphicsEffectSource を継承するため marker も実装する。
impl IGraphicsEffectSource_Impl for GaussianBlurEffect_Impl {}

impl windows::Graphics::Effects::IGraphicsEffect_Impl for GaussianBlurEffect_Impl {
    fn Name(&self) -> WinResult<HSTRING> {
        Ok(self.name.borrow().clone())
    }

    fn SetName(&self, name: &HSTRING) -> WinResult<()> {
        *self.name.borrow_mut() = name.clone();
        Ok(())
    }
}

impl IGraphicsEffectD2D1Interop_Impl for GaussianBlurEffect_Impl {
    fn GetEffectId(&self) -> WinResult<GUID> {
        Ok(CLSID_D2D1GaussianBlur)
    }

    fn GetNamedPropertyMapping(
        &self,
        _name: &PCWSTR,
        _index: *mut u32,
        _mapping: *mut GRAPHICS_EFFECT_PROPERTY_MAPPING,
    ) -> WinResult<()> {
        // 名前引きは未実装。single-arg CreateEffectFactory は animatable property を
        // 宣言しないので呼ばれない。canonical 実装に合わせ E_NOTIMPL を返す。
        Err(Error::from(windows::Win32::Foundation::E_NOTIMPL))
    }

    fn GetPropertyCount(&self) -> WinResult<u32> {
        // D2D1GaussianBlur のプロパティ数 (StandardDeviation / Optimization /
        // BorderMode)。CreateEffectFactory がこの数を D2D 登録スキーマと照合する。
        Ok(3)
    }

    fn GetProperty(&self, index: u32) -> WinResult<IPropertyValue> {
        match index {
            // 0 = D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION (FLOAT)
            0 => PropertyValue::CreateSingle(self.standard_deviation)?.cast(),
            // 1 = D2D1_GAUSSIANBLUR_PROP_OPTIMIZATION (enum → UINT32)
            1 => PropertyValue::CreateUInt32(D2D1_GAUSSIANBLUR_OPTIMIZATION_BALANCED.0 as u32)?
                .cast(),
            // 2 = D2D1_GAUSSIANBLUR_PROP_BORDER_MODE (enum → UINT32)
            2 => PropertyValue::CreateUInt32(D2D1_BORDER_MODE_HARD.0 as u32)?.cast(),
            _ => Err(Error::from(windows::Win32::Foundation::E_INVALIDARG)),
        }
    }

    fn GetSource(&self, index: u32) -> WinResult<IGraphicsEffectSource> {
        match index {
            0 => Ok(self.source.clone()),
            _ => Err(Error::from(windows::Win32::Foundation::E_INVALIDARG)),
        }
    }

    fn GetSourceCount(&self) -> WinResult<u32> {
        Ok(1)
    }
}

/// 背後 (backdrop) を Gaussian blur する `CompositionBrush` を作る。
/// `standard_deviation` は logical px 基準 (DPI スケールは呼び出し側で補正)。
///
/// # Errors
/// effect factory / brush 生成が失敗したとき。
pub fn create_backdrop_blur_brush(
    compositor: &Compositor,
    standard_deviation: f32,
) -> Result<CompositionBrush> {
    let source_param = CompositionEffectSourceParameter::Create(&HSTRING::from(SOURCE_NAME))
        .map_err(map_hr("CompositionEffectSourceParameter::Create"))?;
    let effect: IGraphicsEffect = GaussianBlurEffect {
        name: RefCell::new(HSTRING::from("LineruleBlur")),
        standard_deviation,
        source: source_param.cast().map_err(map_hr(
            "CompositionEffectSourceParameter::cast<IGraphicsEffectSource>",
        ))?,
    }
    .into();

    let factory = compositor
        .CreateEffectFactory(&effect)
        .map_err(map_hr("Compositor::CreateEffectFactory"))?;
    let brush = factory
        .CreateBrush()
        .map_err(map_hr("CompositionEffectFactory::CreateBrush"))?;

    // backdrop の選択 (実機検証ポイント)。本 overlay は WS_EX_NOREDIRECTIONBITMAP の
    // 透明窓なので、`CreateBackdropBrush` が「窓の背後」(他アプリ/デスクトップ) を
    // サンプルできる (redirection surface が無いため compositor が透過してサンプルする;
    // self-feedback も無い)。これが調査 (yvt.jp の Win32 backdrop blur 解説 + MS Q&A)
    // で確認した既定。一般の MS Learn doc は `CreateBackdropBrush` を「同一窓内の背後」と
    // 説明しており、その読みでは `CreateHostBackdropBrush` が必要に見えるが、Win32 透明窓では
    // `CreateHostBackdropBrush` は黒を返す既知の問題がある。実機で「ぼけず tint だけ」に
    // 見えたら `LINERULE_BLUR_HOST=1` で host backdrop に切り替えて比較できる。
    let backdrop = if std::env::var("LINERULE_BLUR_HOST").is_ok() {
        compositor
            .CreateHostBackdropBrush()
            .map_err(map_hr("Compositor::CreateHostBackdropBrush"))?
    } else {
        compositor
            .CreateBackdropBrush()
            .map_err(map_hr("Compositor::CreateBackdropBrush"))?
    };
    brush
        .SetSourceParameter(&HSTRING::from(SOURCE_NAME), &backdrop)
        .map_err(map_hr("CompositionEffectBrush::SetSourceParameter"))?;
    brush.cast().map_err(map_hr("CompositionEffectBrush::cast"))
}

const _: GRAPHICS_EFFECT_PROPERTY_MAPPING = GRAPHICS_EFFECT_PROPERTY_MAPPING_DIRECT;

fn map_hr(operation: &'static str) -> impl Fn(Error) -> PlatformError {
    move |e: Error| PlatformError::BadHr {
        operation,
        hr: e.code().0,
    }
}
