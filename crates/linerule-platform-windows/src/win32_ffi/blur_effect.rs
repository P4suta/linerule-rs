//! WinRT Composition 用の自作 D2D エフェクトグラフ記述。
//!
//! WinRT には `IGraphicsEffect` を実装した D2D エフェクトクラスが Win2D (UWP 専用)
//! にしか無いため、`IGraphicsEffectD2D1Interop` を汎用ノード [`D2dEffectNode`] として
//! 自前実装する。各ノードは CLSID・プロパティ列・0..N source を記述し、source に別ノードを
//! 差すことでエフェクトグラフを組める。
//!
//! backdrop blur 用には以下を構築し、root を `Compositor::CreateEffectFactory` に渡す:
//! ```text
//! BASE  = backdrop → GaussianBlur(σ) → Saturation → Contrast
//! NOISE = Turbulence → Saturation(0=グレー化) → Opacity(強度)
//! root  = Composite[SOURCE_OVER]( BASE, NOISE )
//! ```
//! 純粋なぼかしだけだと実機での見えが「のっぺり」するため、後段で彩度/明暗を張り
//! (vibrancy)、さらに procedural な Turbulence ノイズを薄く重ねて摺りガラスの粒状感を
//! 出す。leaf (blur) の source に `CompositionBackdropBrush` を差し込む。

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
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_BORDER_MODE_HARD, D2D1_COMPOSITE_MODE_SOURCE_OVER, D2D1_TURBULENCE_NOISE_FRACTAL_SUM,
};
use windows::Win32::Graphics::Direct2D::{
    CLSID_D2D1Composite, CLSID_D2D1Contrast, CLSID_D2D1GaussianBlur, CLSID_D2D1Opacity,
    CLSID_D2D1Saturation, CLSID_D2D1Turbulence, D2D1_GAUSSIANBLUR_OPTIMIZATION_BALANCED,
};
use windows::Win32::System::WinRT::Graphics::Direct2D::{
    GRAPHICS_EFFECT_PROPERTY_MAPPING, GRAPHICS_EFFECT_PROPERTY_MAPPING_DIRECT,
    IGraphicsEffectD2D1Interop, IGraphicsEffectD2D1Interop_Impl,
};
use windows::core::{Error, GUID, HSTRING, Interface, PCWSTR, Result as WinResult, implement};

use crate::error::{PlatformError, Result};

/// CompositionEffectFactory に渡す source パラメータ名。`SetSourceParameter` でも
/// 同名で backdrop を差し込む。グラフの leaf (GaussianBlur) のみが参照する。
const SOURCE_NAME: &str = "source";

/// 後段 `Saturation` の既定値。`D2D1_SATURATION_PROP_SATURATION` は `[0, 1]` で
/// 0.5 = 原画 (identity)、1.0 = 最大彩度。0.5 超で彩度が上がる。
const BLUR_SATURATION: f32 = 0.70;
/// 後段 `Contrast` の既定値。`D2D1_CONTRAST_PROP_CONTRAST` は `[-1, 1]` で 0 = identity、
/// 正で明暗の張り (コントラスト) が増す。
const BLUR_CONTRAST: f32 = 0.15;
/// ノイズ粒状感の既定強度 (`Opacity`、`[0, 1]`)。0 でノイズ枝を作らない (無効)。
const BLUR_NOISE: f32 = 0.05;
/// ノイズの既定 base frequency (`[0.05, 1.0]`、1/DIP)。高いほど細かい粒。
const BLUR_NOISE_FREQ: f32 = 0.70;

/// 汎用 D2D エフェクトノード。1 つの CLSID・プロパティ列 (登録スキーマ順に box 済み)・
/// 0..N source (子ノード or backdrop source param) を保持し、`IGraphicsEffectD2D1Interop`
/// として CreateEffectFactory に解釈させる。
///
/// `CreateEffectFactory` は各 CLSID の登録スキーマとプロパティ/source の数・型が一致する
/// ことを検証し、不一致だと `E_INVALIDARG` を返す。よってプロパティは「数・順序・型」を、
/// source は「数」を正しく渡す (例: GaussianBlur=プロパティ3/source1、Turbulence=
/// プロパティ7/source0、Composite=プロパティ1/source2)。
#[implement(IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectD2D1Interop)]
struct D2dEffectNode {
    name: RefCell<HSTRING>,
    clsid: GUID,
    properties: Vec<IPropertyValue>,
    sources: Vec<IGraphicsEffectSource>,
}

// IGraphicsEffect は IGraphicsEffectSource を継承するため marker も実装する。
impl IGraphicsEffectSource_Impl for D2dEffectNode_Impl {}

impl windows::Graphics::Effects::IGraphicsEffect_Impl for D2dEffectNode_Impl {
    fn Name(&self) -> WinResult<HSTRING> {
        Ok(self.name.borrow().clone())
    }

    fn SetName(&self, name: &HSTRING) -> WinResult<()> {
        *self.name.borrow_mut() = name.clone();
        Ok(())
    }
}

impl IGraphicsEffectD2D1Interop_Impl for D2dEffectNode_Impl {
    fn GetEffectId(&self) -> WinResult<GUID> {
        Ok(self.clsid)
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
        // 登録スキーマと照合される。box した数をそのまま返す。
        Ok(u32::try_from(self.properties.len()).unwrap_or(u32::MAX))
    }

    fn GetProperty(&self, index: u32) -> WinResult<IPropertyValue> {
        self.properties
            .get(index as usize)
            .cloned()
            .ok_or_else(|| Error::from(windows::Win32::Foundation::E_INVALIDARG))
    }

    fn GetSource(&self, index: u32) -> WinResult<IGraphicsEffectSource> {
        self.sources
            .get(index as usize)
            .cloned()
            .ok_or_else(|| Error::from(windows::Win32::Foundation::E_INVALIDARG))
    }

    fn GetSourceCount(&self) -> WinResult<u32> {
        Ok(u32::try_from(self.sources.len()).unwrap_or(u32::MAX))
    }
}

/// ノードを構築し、`IGraphicsEffect` として返す (次段の source に差すには
/// [`as_source`] で `IGraphicsEffectSource` に cast する)。
fn node(
    name: &str,
    clsid: GUID,
    properties: Vec<IPropertyValue>,
    sources: Vec<IGraphicsEffectSource>,
) -> IGraphicsEffect {
    D2dEffectNode {
        name: RefCell::new(HSTRING::from(name)),
        clsid,
        properties,
        sources,
    }
    .into()
}

/// `IGraphicsEffect` を次段ノードの source として使えるよう `IGraphicsEffectSource` に
/// cast する。
fn as_source(effect: &IGraphicsEffect) -> Result<IGraphicsEffectSource> {
    effect
        .cast()
        .map_err(map_hr("IGraphicsEffect::cast<IGraphicsEffectSource>"))
}

/// FLOAT プロパティを box する。
fn single(v: f32) -> Result<IPropertyValue> {
    PropertyValue::CreateSingle(v)
        .map_err(map_hr("PropertyValue::CreateSingle"))?
        .cast()
        .map_err(map_hr("PropertyValue::cast (single)"))
}

/// UINT32 (enum) プロパティを box する。
fn uint(v: u32) -> Result<IPropertyValue> {
    PropertyValue::CreateUInt32(v)
        .map_err(map_hr("PropertyValue::CreateUInt32"))?
        .cast()
        .map_err(map_hr("PropertyValue::cast (uint)"))
}

/// BOOL プロパティを box する (D2D の `D2D1_PROPERTY_TYPE_BOOL`; Win2D 同様 Boolean)。
fn boolean(v: bool) -> Result<IPropertyValue> {
    PropertyValue::CreateBoolean(v)
        .map_err(map_hr("PropertyValue::CreateBoolean"))?
        .cast()
        .map_err(map_hr("PropertyValue::cast (boolean)"))
}

/// VECTOR2 (`D2D1_VECTOR_2F`) プロパティを box する (FLOAT 配列で表現)。
fn vector2(x: f32, y: f32) -> Result<IPropertyValue> {
    PropertyValue::CreateSingleArray(&[x, y])
        .map_err(map_hr("PropertyValue::CreateSingleArray"))?
        .cast()
        .map_err(map_hr("PropertyValue::cast (vector2)"))
}

/// env から f32 を読む (実機での見え方チューニング用)。無効値は `default`。
fn env_f32(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.trim().parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(default)
}

/// 背後 (backdrop) を Gaussian blur し、後段で彩度/コントラストを持ち上げ、さらに
/// procedural ノイズで粒状感を足した `CompositionBrush` を作る。`standard_deviation` は
/// logical px 基準 (DPI スケールは呼び出し側で補正)。
///
/// 強さは既定 [`BLUR_SATURATION`] / [`BLUR_CONTRAST`] / [`BLUR_NOISE`] /
/// [`BLUR_NOISE_FREQ`]。実機調整用に env で上書きできる (再ビルド不要):
/// `LINERULE_BLUR_SATURATION` (`[0,1]`) / `LINERULE_BLUR_CONTRAST` (`[-1,1]`) /
/// `LINERULE_BLUR_NOISE` (`[0,1]`、0 でノイズ無効) / `LINERULE_BLUR_NOISE_FREQ`
/// (`[0.05,1]`)。
///
/// # Errors
/// effect factory / brush 生成が失敗したとき。
pub fn create_backdrop_blur_brush(
    compositor: &Compositor,
    standard_deviation: f32,
) -> Result<CompositionBrush> {
    let saturation = env_f32("LINERULE_BLUR_SATURATION", BLUR_SATURATION).clamp(0.0, 1.0);
    let contrast = env_f32("LINERULE_BLUR_CONTRAST", BLUR_CONTRAST).clamp(-1.0, 1.0);
    let noise = env_f32("LINERULE_BLUR_NOISE", BLUR_NOISE).clamp(0.0, 1.0);
    let noise_freq = env_f32("LINERULE_BLUR_NOISE_FREQ", BLUR_NOISE_FREQ).clamp(0.05, 1.0);

    let source_param = CompositionEffectSourceParameter::Create(&HSTRING::from(SOURCE_NAME))
        .map_err(map_hr("CompositionEffectSourceParameter::Create"))?;
    let source_param = source_param.cast().map_err(map_hr(
        "CompositionEffectSourceParameter::cast<IGraphicsEffectSource>",
    ))?;

    // BASE: backdrop → GaussianBlur → Saturation → Contrast。
    // GaussianBlur: 0 StandardDeviation(FLOAT) / 1 Optimization(enum) / 2 BorderMode(enum)。
    let blur = node(
        "LineruleBlur",
        CLSID_D2D1GaussianBlur,
        vec![
            single(standard_deviation)?,
            uint(D2D1_GAUSSIANBLUR_OPTIMIZATION_BALANCED.0 as u32)?,
            uint(D2D1_BORDER_MODE_HARD.0 as u32)?,
        ],
        vec![source_param],
    );
    let saturation_node = node(
        "LineruleSaturation",
        CLSID_D2D1Saturation,
        vec![single(saturation)?],
        vec![as_source(&blur)?],
    );
    let contrast_node = node(
        "LineruleContrast",
        CLSID_D2D1Contrast,
        vec![single(contrast)?, boolean(false)?],
        vec![as_source(&saturation_node)?],
    );

    // --- 一時診断: ノイズ各ノードを単体で factory にかけ、E_INVALIDARG の出所を特定する ---
    // (このブロックは原因特定後に削除する。失敗時 smoke が events.jsonl tail を dump するので
    //  probe の WARN がそこに出る。)
    if noise > 0.0 {
        probe(
            compositor,
            &build_turbulence(noise_freq)?,
            "turbulence_only",
        );
        let op = node(
            "ProbeOpacity",
            CLSID_D2D1Opacity,
            vec![single(noise)?],
            vec![as_source(&build_turbulence(noise_freq)?)?],
        );
        probe(compositor, &op, "opacity_over_turbulence");
        let comp = node(
            "ProbeComposite",
            CLSID_D2D1Composite,
            vec![uint(D2D1_COMPOSITE_MODE_SOURCE_OVER.0 as u32)?],
            vec![
                as_source(&build_turbulence(noise_freq)?)?,
                as_source(&build_turbulence(noise_freq)?)?,
            ],
        );
        probe(compositor, &comp, "composite_two_turbulence");
    }

    // root: ノイズ無効なら BASE(=Contrast)、有効なら NOISE を SOURCE_OVER で重ねる。
    let root = if noise > 0.0 {
        build_noise_composite(&contrast_node, noise, noise_freq)?
    } else {
        contrast_node
    };

    let factory = compositor
        .CreateEffectFactory(&root)
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

/// Turbulence ジェネレータノード (入力 0)。プロパティ: 0 Offset(V2) / 1 Size(V2) /
/// 2 BaseFrequency(V2) / 3 NumOctaves(UINT32) / 4 Seed(UINT32) / 5 Noise(enum) /
/// 6 Stitchable(BOOL)。Offset/Size は仮想デスクトップの負側も覆うよう大きく取り空出力を避ける。
fn build_turbulence(frequency: f32) -> Result<IGraphicsEffect> {
    Ok(node(
        "LineruleTurbulence",
        CLSID_D2D1Turbulence,
        vec![
            vector2(-8192.0, -8192.0)?,
            vector2(32768.0, 32768.0)?,
            vector2(frequency, frequency)?,
            uint(1)?,
            uint(0)?,
            uint(D2D1_TURBULENCE_NOISE_FRACTAL_SUM.0 as u32)?,
            boolean(false)?,
        ],
        vec![],
    ))
}

/// `base` (背景) の上に procedural ノイズ粒 (前景) を `Composite[SOURCE_OVER]` で重ねた
/// root ノードを返す。ノイズ枝 = `Turbulence → Saturation(0=グレー化) → Opacity(strength)`。
fn build_noise_composite(
    base: &IGraphicsEffect,
    strength: f32,
    frequency: f32,
) -> Result<IGraphicsEffect> {
    let turbulence = build_turbulence(frequency)?;
    // Saturation(0) で RGB ノイズ → 輝度グレー粒に。
    let grey = node(
        "LineruleNoiseGrey",
        CLSID_D2D1Saturation,
        vec![single(0.0)?],
        vec![as_source(&turbulence)?],
    );
    // Opacity で粒の強度を下げる。
    let faded = node(
        "LineruleNoiseOpacity",
        CLSID_D2D1Opacity,
        vec![single(strength)?],
        vec![as_source(&grey)?],
    );
    // Composite: index0=destination(背景=base), index1=source(前景=noise)。
    Ok(node(
        "LineruleComposite",
        CLSID_D2D1Composite,
        vec![uint(D2D1_COMPOSITE_MODE_SOURCE_OVER.0 as u32)?],
        vec![as_source(base)?, as_source(&faded)?],
    ))
}

/// 一時診断ヘルパ: 単一エフェクトを `CreateEffectFactory` にかけ、結果 (HRESULT) を WARN で
/// 残す。どのノードが `E_INVALIDARG` を出すか特定するためのもの (原因特定後に削除)。
fn probe(compositor: &Compositor, effect: &IGraphicsEffect, label: &str) {
    match compositor.CreateEffectFactory(effect) {
        Ok(_) => tracing::warn!(probe = label, "blur effect factory probe: OK"),
        Err(e) => {
            let hr = e.code().0 as u32;
            tracing::warn!(
                probe = label,
                hr = format!("{hr:#010x}"),
                "blur effect factory probe: FAIL"
            );
        },
    }
}

const _: GRAPHICS_EFFECT_PROPERTY_MAPPING = GRAPHICS_EFFECT_PROPERTY_MAPPING_DIRECT;

fn map_hr(operation: &'static str) -> impl Fn(Error) -> PlatformError {
    move |e: Error| PlatformError::BadHr {
        operation,
        hr: e.code().0,
    }
}
