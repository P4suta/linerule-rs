//! Newtype validators — boundary tests for `Thickness`.
//!
//! Encodes the type-level invariants stated in `lib.rs`:
//! - `Thickness` rejects 0 and >512, accepts 1..=512.

use linerule_core::{CoreError, Rgba, Thickness};

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

#[test]
fn thickness_try_from_is_constructor_alias() {
    assert_eq!(Thickness::try_from(0u16), Err(CoreError::Thickness(0)));
    assert_eq!(Thickness::try_from(28u16).map(Thickness::get), Ok(28));
}

#[test]
fn thickness_round_trips_through_u16() {
    let t = Thickness::new(42).expect("42 is valid");
    let back: u16 = t.into();
    assert_eq!(back, 42);
}

// ---- Rgba -----------------------------------------------------------------

#[test]
fn rgba_constructor_preserves_channels() {
    let c = Rgba::new(0x12, 0x34, 0x56, 0x78);
    assert_eq!((c.r, c.g, c.b, c.a), (0x12, 0x34, 0x56, 0x78));
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
