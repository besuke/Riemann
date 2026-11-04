//! Closed-form approximations of American options under Black-Scholes-Merton: Barone-Adesi and
//! Whaley (1987) and Bjerksund and Stensland (1993).
//!
//! Reproduces QuantLib v1.42 (modified BSD license; see THIRD_PARTY_NOTICES):
//! `ql/pricingengines/vanilla/baroneadesiwhaleyengine.cpp` (Copyright (C) 2003, 2006 Ferdinando
//! Ametrano; (C) 2007 StatPro Italia srl) and `ql/pricingengines/vanilla/bjerksundstenslandengine.cpp`
//! (Copyright (C) 2003 Ferdinando Ametrano; (C) 2007 StatPro Italia srl; (C) 2023 Klaus Spanderen).
//! Only the values are ported; the Greeks come from automatic differentiation of the values, through
//! the Newton iteration of the Barone-Adesi-Whaley critical price as well.
//!
//! Differences from QuantLib: flat continuous rates, one volatility and a time in years, as in
//! [`crate::options`]; QuantLib gives no Greeks for the Barone-Adesi-Whaley approximation when early
//! exercise can be optimal.

// `!(x > 0.0)` is intended: it also rejects NaN
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::autodiff::Operand;
use crate::math::solver1d::is_close_with;
use crate::options::{greeks_of, Greeks, OptionInputs, OptionType};
use crate::Error;

/// Which approximation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AmericanApproximation {
    /// Barone-Adesi and Whaley (QuantLib `BaroneAdesiWhaleyApproximationEngine`).
    BaroneAdesiWhaley,
    /// Bjerksund and Stensland, 1993 (QuantLib `BjerksundStenslandApproximationEngine`).
    BjerksundStensland,
}

fn invalid<T>(reason: String) -> Result<T, Error> {
    Err(Error::InvalidInput { reason })
}

/// Black's formula times the discount factor (QuantLib `blackFormula` and `BlackCalculator::value`).
fn black<T: Operand>(kind: OptionType, strike: T, forward: T, std_dev: T, discount: T) -> T {
    let d1 = (forward / strike).ln() / std_dev + std_dev * 0.5;
    let d2 = d1 - std_dev;
    let phi = kind.sign();
    ((forward * (d1 * phi).normal_cdf()) - (strike * (d2 * phi).normal_cdf())) * phi * discount
}

fn pow<T: Operand>(base: T, exponent: T) -> T {
    (exponent * base.ln()).exp()
}

/// The value of an American option by an approximation.
pub fn compute_american_approximation_value<T: Operand>(
    method: AmericanApproximation,
    kind: OptionType,
    inputs: &OptionInputs<T>,
) -> Result<T, Error> {
    inputs.check()?;
    let OptionInputs { spot, strike, rate, dividend_yield, volatility, time } = *inputs;
    let variance = volatility * volatility * time;
    let risk_free_discount = (-rate * time).exp();
    let dividend_discount = (-dividend_yield * time).exp();
    match method {
        AmericanApproximation::BaroneAdesiWhaley => {
            barone_adesi_whaley(kind, spot, strike, risk_free_discount, dividend_discount, variance)
        }
        AmericanApproximation::BjerksundStensland => {
            bjerksund_stensland(kind, spot, strike, risk_free_discount, dividend_discount, variance)
        }
    }
}

/// The value and Greeks of an American option by an approximation, by automatic differentiation.
pub fn compute_american_approximation_greeks(
    method: AmericanApproximation,
    kind: OptionType,
    inputs: &OptionInputs,
) -> Result<Greeks, Error> {
    greeks_of(inputs, |i| compute_american_approximation_value(method, kind, i))
}

// ---- Barone-Adesi and Whaley ----------------------------------------------------------------------

const BAW_TOLERANCE: f64 = 1e-6;
const MAX_NEWTON_STEPS: usize = 1000;

/// `K` of the quadratic approximation: `2 r / (sigma^2 (1 - e^{-rT}))` per unit time.
fn baw_k<T: Operand>(risk_free_discount: T, variance: T) -> T {
    if !is_close_with(risk_free_discount.primal(), 1.0, 1000) {
        -(risk_free_discount.ln() * 2.0) / (variance * (-risk_free_discount + 1.0))
    } else {
        variance.like(2.0) / variance
    }
}

/// The critical price above (call) or below (put) which exercise is optimal, by QuantLib's Newton
/// iteration from the Barone-Adesi-Whaley seed, to a relative tolerance of 1e-6 on the strike.
fn critical_price<T: Operand>(kind: OptionType, strike: T, risk_free_discount: T, dividend_discount: T, variance: T) -> Result<T, Error> {
    if risk_free_discount.primal() > 1.0 {
        return invalid(format!(
            "the Barone-Adesi-Whaley approximation is not applicable with negative interest rates (risk-free discount factor: {})",
            risk_free_discount.primal()
        ));
    }
    let n = (dividend_discount / risk_free_discount).ln() * 2.0 / variance;
    let m = -(risk_free_discount.ln() * 2.0) / variance;
    let b_t = (dividend_discount / risk_free_discount).ln();
    let std_dev = variance.sqrt();
    let one = variance.like(1.0);
    let root = ((n - 1.0) * (n - 1.0) + m * 4.0).sqrt();
    let mut si = match kind {
        OptionType::Call => {
            let qu = (-(n - 1.0) + root) / 2.0;
            let su = strike / (one - qu.recip());
            let h = -(b_t + std_dev * 2.0) * strike / (su - strike);
            strike + (su - strike) * (one - h.exp())
        }
        OptionType::Put => {
            let qu = (-(n - 1.0) - root) / 2.0;
            let su = strike / (one - qu.recip());
            let h = (b_t - std_dev * 2.0) * strike / (strike - su);
            su + (strike - su) * h.exp()
        }
    };
    let k = baw_k(risk_free_discount, variance);
    let q_root = ((n - 1.0) * (n - 1.0) + k * 4.0).sqrt();
    let q = match kind {
        OptionType::Call => (-(n - 1.0) + q_root) / 2.0,
        OptionType::Put => (-(n - 1.0) - q_root) / 2.0,
    };
    // the right-hand side, the left-hand side and the slope b_i at a trial price
    let terms = |si: T| -> Result<(T, T, T), Error> {
        if !(si.primal() > 0.0) {
            // QuantLib fails here in blackFormula: "forward + displacement must be positive"
            return invalid(format!(
                "the Barone-Adesi-Whaley critical price reached a non-positive price ({})",
                si.primal()
            ));
        }
        let forward = si * dividend_discount / risk_free_discount;
        let d1 = ((forward / strike).ln() + variance * 0.5) / std_dev;
        let value = black(kind, strike, forward, std_dev, risk_free_discount);
        match kind {
            OptionType::Call => {
                let lhs = si - strike;
                let rhs = value + (one - dividend_discount * d1.normal_cdf()) * si / q;
                let bi = dividend_discount * d1.normal_cdf() * (one - q.recip())
                    + (one - dividend_discount * d1.normal_pdf() / std_dev) / q;
                Ok((lhs, rhs, bi))
            }
            OptionType::Put => {
                let lhs = strike - si;
                let rhs = value - (one - dividend_discount * (-d1).normal_cdf()) * si / q;
                let bi = -(dividend_discount * (-d1).normal_cdf() * (one - q.recip()))
                    - (one + dividend_discount * (-d1).normal_pdf() / std_dev) / q;
                Ok((lhs, rhs, bi))
            }
        }
    };
    let (mut lhs, mut rhs, mut bi) = terms(si)?;
    let mut steps = 0;
    while ((lhs - rhs).primal() / strike.primal()).abs() > BAW_TOLERANCE {
        steps += 1;
        if steps > MAX_NEWTON_STEPS {
            return Err(Error::NoConvergence { reason: "Barone-Adesi-Whaley critical price".into() });
        }
        si = match kind {
            OptionType::Call => (strike + rhs - bi * si) / (one - bi),
            OptionType::Put => (strike - rhs + bi * si) / (one + bi),
        };
        (lhs, rhs, bi) = terms(si)?;
    }
    Ok(si)
}

fn barone_adesi_whaley<T: Operand>(
    kind: OptionType,
    spot: T,
    strike: T,
    risk_free_discount: T,
    dividend_discount: T,
    variance: T,
) -> Result<T, Error> {
    let std_dev = variance.sqrt();
    let forward = spot * dividend_discount / risk_free_discount;
    let european = black(kind, strike, forward, std_dev, risk_free_discount);
    if dividend_discount.primal() >= 1.0 && kind == OptionType::Call {
        // early exercise never optimal
        return Ok(european);
    }
    let sk = critical_price(kind, strike, risk_free_discount, dividend_discount, variance)?;
    let forward_sk = sk * dividend_discount / risk_free_discount;
    let d1 = ((forward_sk / strike).ln() + variance * 0.5) / std_dev;
    let n = (dividend_discount / risk_free_discount).ln() * 2.0 / variance;
    let k = baw_k(risk_free_discount, variance);
    let q_root = ((n - 1.0) * (n - 1.0) + k * 4.0).sqrt();
    let one = variance.like(1.0);
    Ok(match kind {
        OptionType::Call => {
            let q = (-(n - 1.0) + q_root) / 2.0;
            let a = sk / q * (one - dividend_discount * d1.normal_cdf());
            if spot.primal() < sk.primal() {
                european + a * pow(spot / sk, q)
            } else {
                spot - strike
            }
        }
        OptionType::Put => {
            let q = (-(n - 1.0) - q_root) / 2.0;
            let a = -(sk / q) * (one - dividend_discount * (-d1).normal_cdf());
            if spot.primal() > sk.primal() {
                european + a * pow(spot / sk, q)
            } else {
                strike - spot
            }
        }
    })
}

// ---- Bjerksund and Stensland ----------------------------------------------------------------------

/// QuantLib's `phi` of the 1993 approximation.
fn phi<T: Operand>(s: T, gamma: T, h: T, i: T, r_t: T, b_t: T, variance: T) -> T {
    let lambda = -r_t + gamma * b_t + gamma * (gamma - 1.0) * variance * 0.5;
    let std_dev = variance.sqrt();
    let d = -((s / h).ln() + (b_t + (gamma - 0.5) * variance)) / std_dev;
    let kappa = b_t * 2.0 / variance + (gamma * 2.0 - 1.0);
    lambda.exp() * (d.normal_cdf() - pow(i / s, kappa) * (d - (i / s).ln() * 2.0 / std_dev).normal_cdf())
}

/// The call value of the 1993 approximation; puts by put-call symmetry.
fn bjerksund_stensland<T: Operand>(
    kind: OptionType,
    spot: T,
    strike: T,
    risk_free_discount: T,
    dividend_discount: T,
    variance: T,
) -> Result<T, Error> {
    // put-call symmetry: a put is a call with spot and strike, and the two rates, exchanged
    let (s, x, rf_d, d_d) = match kind {
        OptionType::Call => (spot, strike, risk_free_discount, dividend_discount),
        OptionType::Put => (strike, spot, dividend_discount, risk_free_discount),
    };
    if d_d.primal() > 1.0 && rf_d.primal() > d_d.primal() {
        return invalid("double-boundary case r<q<0 for a call given".into());
    }
    let std_dev = variance.sqrt();
    let european = black(OptionType::Call, x, s * d_d / rf_d, std_dev, rf_d);
    let mut value = if d_d.primal() >= 1.0 && d_d.primal() >= rf_d.primal() {
        european
    } else {
        let b_t = (d_d / rf_d).ln();
        let r_t = rf_d.recip().ln();
        let one = variance.like(1.0);
        let beta = (b_t / variance * -1.0 + 0.5) + ((b_t / variance - 0.5) * (b_t / variance - 0.5) + r_t * 2.0 / variance).sqrt();
        let b_infinity = beta / (beta - 1.0) * x;
        let b_0 = if b_t.primal() == r_t.primal() {
            x
        } else {
            let candidate = r_t / (r_t - b_t) * x;
            if x.primal() >= candidate.primal() { x } else { candidate }
        };
        let ht = -(b_t + std_dev * 2.0) * b_0 / (b_infinity - b_0);
        let i = b_0 + (b_infinity - b_0) * (one - ht.exp());
        let forward = s * d_d / rf_d;
        let q = (i / forward).ln() / std_dev;
        let approximation = if s.primal() >= i.primal() {
            (s - x).max(s.like(0.0))
        } else if q.primal() > 12.5 {
            // a run-away exercise boundary: QuantLib uses the European values
            european
        } else {
            let zero = s.like(0.0);
            (i - x) * pow(s / i, beta) * (one - phi(s, beta, i, i, r_t, b_t, variance)) + s * phi(s, one, i, i, r_t, b_t, variance)
                - s * phi(s, one, x, i, r_t, b_t, variance)
                - x * phi(s, zero, i, i, r_t, b_t, variance)
                + x * phi(s, zero, x, i, r_t, b_t, variance)
        };
        // never below the European value (the end of QuantLib's americanCallApproximation)
        if approximation.primal() < european.primal() { european } else { approximation }
    };
    if value.primal().is_nan() {
        // QuantLib returns NaN, for example for a call with r < 0 and q = 0, where beta has no real value
        return invalid("the Bjerksund-Stensland approximation is undefined for these inputs (QuantLib returns NaN)".into());
    }
    // immediate exercise when it is worth more
    if value.primal() < (s - x).primal() * (1.0 + 10.0 * f64::EPSILON) {
        value = (s - x).max(s.like(0.0));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{compute_binomial_value, compute_european_value, Exercise};

    fn inputs(spot: f64, strike: f64, rate: f64, q: f64, vol: f64, time: f64) -> OptionInputs {
        OptionInputs { spot, strike, rate, dividend_yield: q, volatility: vol, time }
    }

    #[test]
    fn approximations_lie_between_european_and_intrinsic_and_near_a_fine_tree() {
        use AmericanApproximation::*;
        for kind in [OptionType::Call, OptionType::Put] {
            for (spot, q) in [(90.0, 0.1), (100.0, 0.1), (110.0, 0.0), (100.0, 0.05)] {
                let x = inputs(spot, 100.0, 0.08, q, 0.2, 0.5);
                let european = compute_european_value(kind, &x).unwrap();
                let tree = compute_binomial_value(kind, Exercise::American, &x, 2000).unwrap();
                for method in [BaroneAdesiWhaley, BjerksundStensland] {
                    let v = compute_american_approximation_value(method, kind, &x).unwrap();
                    assert!(v >= european - 1e-12, "{method:?} {kind:?} {spot} {q}: {v} < {european}");
                    assert!((v - tree).abs() < 0.1, "{method:?} {kind:?} {spot} {q}: {v} vs tree {tree}");
                }
            }
        }
    }

    #[test]
    fn greeks_agree_with_finite_differences() {
        use AmericanApproximation::*;
        let base = inputs(95.0, 100.0, 0.06, 0.03, 0.25, 0.75);
        for method in [BaroneAdesiWhaley, BjerksundStensland] {
            for kind in [OptionType::Call, OptionType::Put] {
                let g = compute_american_approximation_greeks(method, kind, &base).unwrap();
                let value = |x: OptionInputs| compute_american_approximation_value(method, kind, &x).unwrap();
                let h = 1e-5;
                let central = |up: OptionInputs, down: OptionInputs| (value(up) - value(down)) / (2.0 * h);
                let delta = central(OptionInputs { spot: base.spot + h, ..base }, OptionInputs { spot: base.spot - h, ..base });
                let vega = central(OptionInputs { volatility: base.volatility + h, ..base }, OptionInputs { volatility: base.volatility - h, ..base });
                let rho = central(OptionInputs { rate: base.rate + h, ..base }, OptionInputs { rate: base.rate - h, ..base });
                // the Newton iteration of Barone-Adesi-Whaley stops at 1e-6 of the strike, so its
                // derivatives carry that error; finite differences see a fixed number of steps too
                for (name, ad, fd) in [("delta", g.delta, delta), ("vega", g.vega, vega), ("rho", g.rho, rho)] {
                    assert!((ad - fd).abs() < 1e-4 * fd.abs().max(1.0), "{method:?} {kind:?} {name}: {ad} against {fd}");
                }
            }
        }
    }

    #[test]
    fn a_call_without_dividends_is_european_and_negative_rates_fail_for_baw() {
        let x = inputs(100.0, 100.0, 0.05, 0.0, 0.3, 1.0);
        let european = compute_european_value(OptionType::Call, &x).unwrap();
        for method in [AmericanApproximation::BaroneAdesiWhaley, AmericanApproximation::BjerksundStensland] {
            let v = compute_american_approximation_value(method, OptionType::Call, &x).unwrap();
            assert!((v - european).abs() < 1e-12);
        }
        let negative = inputs(100.0, 100.0, -0.01, 0.0, 0.3, 1.0);
        assert!(compute_american_approximation_value(AmericanApproximation::BaroneAdesiWhaley, OptionType::Put, &negative).is_err());
    }
}
