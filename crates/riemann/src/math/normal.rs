//! The standard normal distribution: density, distribution function and inverse.
//!
//! Derived from QuantLib v1.42 `ql/math/errorfunction.cpp` and
//! `ql/math/distributions/normaldistribution.cpp` and `.hpp` (Copyright (C) 2000, 2001, 2002, 2003
//! RiskMap srl; (C) 2002, 2003 Ferdinando Ametrano; (C) 2008 StatPro Italia srl; (C) 2010 Kakhkhor
//! Abdijalilov); modified BSD license. See THIRD_PARTY_NOTICES.
//!
//! The error function is based on code from the GNU C library, originally written by Sun:
//!
//! Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
//!
//! Developed at SunPro, a Sun Microsystems, Inc. business.
//! Permission to use, copy, modify, and distribute this
//! software is freely granted, provided that this notice
//! is preserved.

#![allow(clippy::excessive_precision)]

/// The normal density.
pub fn normal_pdf(x: f64) -> f64 {
    (-0.5 * x * x).exp() / (2.0 * std::f64::consts::PI).sqrt()
}

const ERX: f64 = 8.45062911510467529297e-01;
const EFX: f64 = 1.28379167095512586316e-01;
const EFX8: f64 = 1.02703333676410069053e+00;
const PP: [f64; 5] = [
    1.28379167095512558561e-01,
    -3.25042107247001499370e-01,
    -2.84817495755985104766e-02,
    -5.77027029648944159157e-03,
    -2.37630166566501626084e-05,
];
const QQ: [f64; 5] = [
    3.97917223959155352819e-01,
    6.50222499887672944485e-02,
    5.08130628187576562776e-03,
    1.32494738004321644526e-04,
    -3.96022827877536812320e-06,
];
const PA: [f64; 7] = [
    -2.36211856075265944077e-03,
    4.14856118683748331666e-01,
    -3.72207876035701323847e-01,
    3.18346619901161753674e-01,
    -1.10894694282396677476e-01,
    3.54783043256182359371e-02,
    -2.16637559486879084300e-03,
];
const QA: [f64; 6] = [
    1.06420880400844228286e-01,
    5.40397917702171048937e-01,
    7.18286544141962662868e-02,
    1.26171219808761642112e-01,
    1.36370839120290507362e-02,
    1.19844998467991074170e-02,
];
const RA: [f64; 8] = [
    -9.86494403484714822705e-03,
    -6.93858572707181764372e-01,
    -1.05586262253232909814e+01,
    -6.23753324503260060396e+01,
    -1.62396669462573470355e+02,
    -1.84605092906711035994e+02,
    -8.12874355063065934246e+01,
    -9.81432934416914548592e+00,
];
const SA: [f64; 8] = [
    1.96512716674392571292e+01,
    1.37657754143519042600e+02,
    4.34565877475229228821e+02,
    6.45387271733267880336e+02,
    4.29008140027567833386e+02,
    1.08635005541779435134e+02,
    6.57024977031928170135e+00,
    -6.04244152148580987438e-02,
];
const RB: [f64; 7] = [
    -9.86494292470009928597e-03,
    -7.99283237680523006574e-01,
    -1.77579549177547519889e+01,
    -1.60636384855821916062e+02,
    -6.37566443368389627722e+02,
    -1.02509513161107724954e+03,
    -4.83519191608651397019e+02,
];
const SB: [f64; 7] = [
    3.03380607434824582924e+01,
    3.25792512996573918826e+02,
    1.53672958608443695994e+03,
    3.19985821950859553908e+03,
    2.55305040643316442583e+03,
    4.74528541206955367215e+02,
    -2.24409524465858183362e+01,
];

/// Horner's rule with a leading constant term.
fn poly(first: f64, x: f64, rest: &[f64]) -> f64 {
    first + x * rest.iter().rev().fold(0.0, |sum, &c| c + x * sum)
}

/// The error function (QuantLib `ErrorFunction`).
fn erf(x: f64) -> f64 {
    if !x.is_finite() {
        return if x.is_nan() {
            x
        } else if x > 0.0 {
            1.0
        } else {
            -1.0
        };
    }
    let ax = x.abs();
    if ax < 0.84375 {
        if ax < 3.7252902984e-09 {
            if ax < f64::MIN_POSITIVE * 16.0 {
                return 0.125 * (8.0 * x + EFX8 * x);
            }
            return x + EFX * x;
        }
        let z = x * x;
        let r = poly(PP[0], z, &PP[1..]);
        let s = poly(1.0, z, &QQ);
        return x + x * (r / s);
    }
    if ax < 1.25 {
        let s = ax - 1.0;
        let p = poly(PA[0], s, &PA[1..]);
        let q = poly(1.0, s, &QA);
        return if x >= 0.0 { ERX + p / q } else { -ERX - p / q };
    }
    if ax >= 6.0 {
        return if x >= 0.0 {
            1.0 - f64::EPSILON
        } else {
            f64::EPSILON - 1.0
        };
    }
    let s = 1.0 / (ax * ax);
    let (r, big_s) = if ax < 2.85714285714285 {
        (poly(RA[0], s, &RA[1..]), poly(1.0, s, &SA))
    } else {
        (poly(RB[0], s, &RB[1..]), poly(1.0, s, &SB))
    };
    let r = (-ax * ax - 0.5625 + r / big_s).exp();
    if x >= 0.0 {
        1.0 - r / ax
    } else {
        r / ax - 1.0
    }
}

/// The normal distribution function (QuantLib `CumulativeNormalDistribution`).
pub fn normal_cdf(z: f64) -> f64 {
    let result = 0.5 * (1.0 + erf(z * std::f64::consts::FRAC_1_SQRT_2));
    if result > 1e-8 {
        return result;
    }
    // asymptotic expansion for very negative z, following (26.2.12) in Abramowitz and Stegun
    let (mut sum, zsqr, mut i, mut g) = (1.0, z * z, 1.0, 1.0);
    let mut a = f64::MAX;
    loop {
        let last = a;
        let x = (4.0 * i - 3.0) / zsqr;
        let y = x * ((4.0 * i - 1.0) / zsqr);
        a = g * (x - y);
        sum -= a;
        g *= y;
        i += 1.0;
        a = a.abs();
        if !(last > a && a >= (sum * f64::EPSILON).abs()) {
            break;
        }
    }
    -normal_pdf(z) / z * sum
}

const A: [f64; 6] = [
    -3.969683028665376e+01,
    2.209460984245205e+02,
    -2.759285104469687e+02,
    1.383577518672690e+02,
    -3.066479806614716e+01,
    2.506628277459239e+00,
];
const B: [f64; 5] = [
    -5.447609879822406e+01,
    1.615858368580409e+02,
    -1.556989798598866e+02,
    6.680131188771972e+01,
    -1.328068155288572e+01,
];
const C: [f64; 6] = [
    -7.784894002430293e-03,
    -3.223964580411365e-01,
    -2.400758277161838e+00,
    -2.549732539343734e+00,
    4.374664141464968e+00,
    2.938163982698783e+00,
];
const D: [f64; 4] = [
    7.784695709041462e-03,
    3.224671290700398e-01,
    2.445134137142996e+00,
    3.754408661907416e+00,
];
const X_LOW: f64 = 0.02425;

/// The inverse of the normal distribution function for 0 < x < 1 (QuantLib
/// `InverseCumulativeNormal`, without refinement: relative error below 1.15e-9).
pub fn inverse_normal_cdf(x: f64) -> f64 {
    let tail = |z: f64| {
        (((((C[0] * z + C[1]) * z + C[2]) * z + C[3]) * z + C[4]) * z + C[5])
            / ((((D[0] * z + D[1]) * z + D[2]) * z + D[3]) * z + 1.0)
    };
    if x < X_LOW {
        tail((-2.0 * x.ln()).sqrt())
    } else if x > 1.0 - X_LOW {
        -tail((-2.0 * (1.0 - x).ln()).sqrt())
    } else {
        let z = x - 0.5;
        let r = z * z;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * z
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values_of_the_distribution_function() {
        // values of 0.5 erfc(-z / sqrt 2) to 16 digits
        for (z, expected) in [
            (0.0, 0.5),
            (1.0, 0.841_344_746_068_542_9),
            (-1.0, 0.158_655_253_931_457_05),
            (1.96, 0.975_002_104_851_779_5),
            (-3.0, 0.001_349_898_031_630_094_5),
            (3.5, 0.999_767_370_920_964_2),
            (-6.0, 9.865_876_450_376_946e-10),
        ] {
            let got = normal_cdf(z);
            assert!((got - expected).abs() < 1e-15, "{z}: {got}");
        }
    }

    #[test]
    fn the_inverse_undoes_the_distribution_function() {
        for x in [1e-8, 1e-4, 0.01, 0.02425, 0.1, 0.5, 0.9, 0.97575, 0.999, 1.0 - 1e-8] {
            let z = inverse_normal_cdf(x);
            assert!((normal_cdf(z) - x).abs() < 3e-9, "{x}: {z}");
        }
    }

    #[test]
    fn symmetry() {
        for z in [0.1, 0.5, 0.9, 1.2, 2.0, 3.0, 5.0] {
            assert!((normal_cdf(z) + normal_cdf(-z) - 1.0).abs() < 1e-15, "{z}");
        }
    }
}
