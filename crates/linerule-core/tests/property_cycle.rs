//! `cycle` is a 4-element permutation — verify cycle⁴ ≡ id.
//!
//! `Mode` is `#[non_exhaustive]` with no public ordering, so we enumerate
//! the four variants explicitly rather than ask bolero to generate them.
//! If a fifth variant is ever added, `match` exhaustiveness in `cycle`
//! will force this test to be revisited.

use std::collections::HashSet;

use linerule_core::{Mode, cycle};

const ALL_MODES: [Mode; 4] = [Mode::Off, Mode::Bar, Mode::Mask, Mode::Vertical];

#[test]
fn property_cycle_is_period_four() {
    for &m in &ALL_MODES {
        assert_eq!(cycle(cycle(cycle(cycle(m)))), m, "cycle⁴ != id at {m:?}");
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
    assert_eq!(cycle(Mode::Vertical), Mode::Off);
}

#[test]
fn mode_default_is_off() {
    assert_eq!(Mode::default(), Mode::Off);
}
