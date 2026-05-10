//! `cycle` is a 5-element permutation — verify cycle⁵ ≡ id.
//!
//! `Mode` is `#[non_exhaustive]` with no public ordering, so we enumerate
//! the five variants explicitly rather than ask bolero to generate them.
//! If a sixth variant is ever added, `match` exhaustiveness in `cycle`
//! will force this test to be revisited.

use std::collections::HashSet;

use linerule_core::{Mode, cycle};

const ALL_MODES: [Mode; 5] = [
    Mode::Off,
    Mode::Bar,
    Mode::Mask,
    Mode::Vertical,
    Mode::VerticalMask,
];

#[test]
fn property_cycle_is_period_five() {
    for &m in &ALL_MODES {
        assert_eq!(
            cycle(cycle(cycle(cycle(cycle(m))))),
            m,
            "cycle⁵ != id at {m:?}",
        );
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
    assert_eq!(cycle(Mode::Off), Mode::Bar);
    assert_eq!(cycle(Mode::Bar), Mode::Mask);
    assert_eq!(cycle(Mode::Mask), Mode::Vertical);
    assert_eq!(cycle(Mode::Vertical), Mode::VerticalMask);
    assert_eq!(cycle(Mode::VerticalMask), Mode::Off);
}

#[test]
fn mode_default_is_off() {
    assert_eq!(Mode::default(), Mode::Off);
}
