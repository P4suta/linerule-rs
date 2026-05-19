//! Perceptual opacity helpers.
//!
//! Linear alpha values are pretty for math but ugly for human perception:
//! doubling a linear alpha does not double perceived translucency. We map
//! linear values to perceptual ones with two curves:
//!
//! - [`smooth`]: simple gamma-2.2 approximation, fast and adequate for HUD fade.
//! - [`l_star`]: CIE L\* (piecewise cube-root + linear toe), used by
//!   [`crate::color::Opacity`] to convert a stored byte into its on-screen alpha.

/// `linear^(1/2.2)` clamped to `[0, 1]`. NaN and negatives map to `0`; values
/// `≥ 1` map to `1`. Branch-free in the common case.
#[must_use]
pub fn smooth(linear: f32) -> f32 {
    if !linear.is_finite() || linear <= 0.0 {
        return 0.0;
    }
    if linear >= 1.0 {
        return 1.0;
    }
    linear.powf(1.0 / 2.2)
}

/// CIE L\* curve: piecewise cube-root above the toe, linear segment below.
/// Returns a value in `[0, 1]`. NaN and negatives map to `0`.
#[must_use]
pub fn l_star(linear: f32) -> f32 {
    const TOE: f32 = 0.008_856;
    const LINEAR_SLOPE: f32 = 9.032_962;

    if !linear.is_finite() || linear <= 0.0 {
        return 0.0;
    }
    if linear >= 1.0 {
        return 1.0;
    }
    if linear <= TOE {
        return linear * LINEAR_SLOPE / 1.16;
    }
    1.16_f32.mul_add(linear.cbrt(), -0.16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smooth_endpoints() {
        assert!((smooth(0.0) - 0.0).abs() < 1e-6);
        assert!((smooth(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn smooth_monotone() {
        let a = smooth(0.25);
        let b = smooth(0.5);
        let c = smooth(0.75);
        assert!(a < b);
        assert!(b < c);
    }

    #[test]
    fn smooth_handles_nan_and_negatives() {
        assert!(smooth(f32::NAN).abs() < 1e-6);
        assert!(smooth(-1.0).abs() < 1e-6);
        assert!((smooth(2.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn l_star_endpoints() {
        assert!((l_star(0.0) - 0.0).abs() < 1e-6);
        assert!((l_star(1.0) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn l_star_monotone() {
        let xs: [f32; 6] = [0.0, 0.05, 0.2, 0.4, 0.7, 1.0];
        let ys: Vec<f32> = xs.iter().map(|x| l_star(*x)).collect();
        for w in ys.windows(2) {
            assert!(w[0] <= w[1], "l_star must be non-decreasing: {ys:?}");
        }
    }
}
