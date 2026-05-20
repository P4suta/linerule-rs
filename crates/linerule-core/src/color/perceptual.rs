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

    /// `smooth(0.5)` の具体値を pin する。`1.0 / 2.2` を `1.0 % 2.2` (= `1.0`)
    /// や `1.0 * 2.2` (= `2.2`) に mutate された場合と区別するため、 公式値
    /// `0.5^(1/2.2) ≈ 0.7297` を tolerance 0.005 で挟む (Phase ε mutation
    /// baseline)。
    #[test]
    fn smooth_midpoint_value_is_pinned() {
        // 0.5^(1/2.2) ≈ 0.7297400
        let v = smooth(0.5);
        assert!(
            (v - 0.729_740).abs() < 0.005,
            "smooth(0.5): expected ≈ 0.7297, got {v}"
        );
        // 0.25 の場合も別 spot で確認 (0.25^(1/2.2) ≈ 0.5325)
        let v2 = smooth(0.25);
        assert!(
            (v2 - 0.532_5).abs() < 0.005,
            "smooth(0.25): expected ≈ 0.5325, got {v2}"
        );
    }

    /// `l_star` の toe 以下 (linear 部分, L38) と toe 以上 (cube-root 部分, L40)
    /// の両方を pin する。L38 `*` / `/` の mutation を spot で catch する。
    #[test]
    fn l_star_segment_values_are_pinned() {
        // toe 以下: linear * 9.032962 / 1.16
        // linear = 0.005 → 0.005 * 9.032962 / 1.16 ≈ 0.038935
        let v_toe = l_star(0.005);
        assert!(
            (v_toe - 0.038_935).abs() < 0.001,
            "l_star(0.005) toe segment: expected ≈ 0.03894, got {v_toe}"
        );
        // toe 以上: 1.16 * linear.cbrt() - 0.16
        // linear = 0.5 → 1.16 * 0.5^(1/3) - 0.16 ≈ 1.16 * 0.7937 - 0.16 ≈ 0.7607
        let v_cbrt = l_star(0.5);
        assert!(
            (v_cbrt - 0.760_67).abs() < 0.005,
            "l_star(0.5) cube-root segment: expected ≈ 0.7607, got {v_cbrt}"
        );
    }

    /// `l_star` が NaN / 負値で 0.0 を返すことを pin する。L31 `||` を `&&`
    /// に mutate すると NaN は `is_finite=false` だが `<= 0.0=false` なので
    /// `&&` の方は false で 0.0 ガードを抜けてしまう。
    #[test]
    fn l_star_handles_nan_and_negatives() {
        assert!(l_star(f32::NAN).abs() < 1e-6, "NaN must map to 0.0");
        assert!(l_star(-0.5).abs() < 1e-6, "negative must map to 0.0");
        assert!(
            l_star(f32::INFINITY).abs() < 1e-6,
            "infinity must map to 0.0"
        );
        // > 1 は 1.0 にクランプ
        assert!((l_star(2.0) - 1.0).abs() < 1e-6);
    }
}
