//! Newtype validators — boundary tests for `Opacity` / `Thickness`.
//!
//! Encodes the type-level invariants stated in `lib.rs`:
//! - `Opacity` rejects 0, accepts 1..=255.
//! - `Thickness` rejects 0 and >512, accepts 1..=512.

use linerule_core::{CoreError, Opacity, Rgba, Thickness};

// ---- Opacity --------------------------------------------------------------

#[test]
fn opacity_rejects_zero() {
    assert_eq!(Opacity::new(0), Err(CoreError::Opacity(0)));
}

#[test]
fn opacity_accepts_one() {
    let o = Opacity::new(1).expect("1 is valid opacity");
    assert_eq!(o.get(), 1);
}

#[test]
fn opacity_accepts_max() {
    let o = Opacity::new(255).expect("255 is valid opacity");
    assert_eq!(o.get(), 255);
}

#[test]
fn opacity_round_trip_for_every_valid_value() {
    for v in 1u8..=255 {
        let o = Opacity::new(v).expect("non-zero value should validate");
        let back: u8 = o.into();
        assert_eq!(back, v, "round-trip failed for {v}");
    }
}

#[test]
fn opacity_default_is_aa() {
    assert_eq!(Opacity::DEFAULT.get(), 0xaa);
}

#[test]
fn opacity_try_from_is_constructor_alias() {
    assert_eq!(Opacity::try_from(0u8), Err(CoreError::Opacity(0)));
    assert_eq!(Opacity::try_from(128u8).map(Opacity::get), Ok(128));
}

// ---- Thickness ------------------------------------------------------------

#[test]
fn thickness_rejects_zero() {
    assert_eq!(Thickness::new(0), Err(CoreError::Thickness(0)));
}

#[test]
fn thickness_rejects_overflow() {
    let too_big = Thickness::MAX_PX + 1;
    assert_eq!(
        Thickness::new(too_big),
        Err(CoreError::Thickness(u32::from(too_big))),
    );
}

#[test]
fn thickness_accepts_one() {
    assert_eq!(Thickness::new(1).map(Thickness::get), Ok(1));
}

#[test]
fn thickness_accepts_max() {
    assert_eq!(
        Thickness::new(Thickness::MAX_PX).map(Thickness::get),
        Ok(Thickness::MAX_PX),
    );
}

#[test]
fn thickness_default_is_28() {
    assert_eq!(Thickness::DEFAULT.get(), 28);
}

// ---- Rgba -----------------------------------------------------------------

#[test]
fn rgba_constructor_preserves_channels() {
    let c = Rgba::new(0x12, 0x34, 0x56, 0x78);
    assert_eq!((c.r, c.g, c.b, c.a), (0x12, 0x34, 0x56, 0x78));
}

#[test]
fn rgba_default_bar_is_warm_yellow() {
    let bar = Rgba::DEFAULT_BAR;
    assert_eq!(bar.r, 0xff);
    assert_eq!(bar.g, 0xeb);
    assert_eq!(bar.b, 0x3b);
    assert_eq!(bar.a, 0xaa);
}

#[test]
fn rgba_default_mask_is_near_black_not_pure_black() {
    // Pure black is the Windows colour-key sentinel
    // (`linerule_platform::windows::COLORKEY_TRANSPARENT`). The mask
    // colour MUST avoid `(0, 0, 0)` or its dim regions become
    // transparent slits in the Win32 layered window.
    let m = Rgba::DEFAULT_MASK;
    assert!(
        (m.r, m.g, m.b) != (0, 0, 0),
        "DEFAULT_MASK must NOT be pure black (would collide with the LWA_COLORKEY sentinel); got ({}, {}, {})",
        m.r,
        m.g,
        m.b,
    );
    assert!(
        m.r < 64 && m.g < 64 && m.b < 64,
        "DEFAULT_MASK should still be a dark shade for the typoscope effect; got ({}, {}, {})",
        m.r,
        m.g,
        m.b,
    );
}
