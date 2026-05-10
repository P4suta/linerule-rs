//! `cycle` is a 3-element permutation — verify cycle³ ≡ id.
//!
//! `Mode` is `#[non_exhaustive]` (`Off | Mask(Orientation)`) with no
//! public ordering, so we enumerate the three reachable variants
//! explicitly rather than ask bolero to generate them. If a fourth
//! variant is ever added, `match` exhaustiveness in `cycle` will
//! force this test to be revisited.

use std::collections::HashSet;

use linerule_core::{Mode, cycle};

const ALL_MODES: [Mode; 3] = [Mode::Off, Mode::HORIZONTAL_MASK, Mode::VERTICAL_MASK];

#[test]
fn property_cycle_is_period_three() {
    for &m in &ALL_MODES {
        assert_eq!(cycle(cycle(cycle(m))), m, "cycle³ != id at {m:?}");
    }
}

#[test]
fn property_cycle_is_a_permutation() {
    let images: HashSet<_> = ALL_MODES.iter().map(|&m| cycle(m)).collect();
    assert_eq!(
        images.len(),
        ALL_MODES.len(),
        "cycle is not injective — collisions detected",
    );
}

#[test]
fn cycle_canonical_order() {
    assert_eq!(cycle(Mode::Off), Mode::HORIZONTAL_MASK);
    assert_eq!(cycle(Mode::HORIZONTAL_MASK), Mode::VERTICAL_MASK);
    assert_eq!(cycle(Mode::VERTICAL_MASK), Mode::Off);
}

#[test]
fn mode_default_is_off() {
    assert_eq!(Mode::default(), Mode::Off);
}
