//! Backward-looking RFR rates in the generalized forward market model (FMM) of Lyashenko and
//! Mercurio (LagrangeFinance II-7.5): the compounded rate of a period, its forward before and during
//! the period, and the caplet on it.
//!
//! The rate `R_j` of the period `[T_s, T_e]` is the overnight rate compounded over the period, known
//! only at `T_e`. Its forward at `t`, the expectation under the `T_e`-forward measure, is
//! `(A(t) P(t, max(t, T_s)) / P(t, T_e) - 1) / tau`, where `A(t)` is the compounding already realized
//! (1 before the period starts). Under that measure it is a martingale with
//! `dR = sigma g(t) R dW` (lognormal) or `dR = sigma g(t) dW` (normal), where `g = 1` up to `T_s` and
//! falls linearly to 0 at `T_e`: the rate keeps moving through the period, less and less as the
//! fixings accumulate. The caplet paying `tau (R - K)^+` at `T_e` is Black's or Bachelier's formula
//! with the variance `sigma^2 v`, where
//!
//! - before the period: `v = (T_s - t) + (T_e - T_s) / 3`;
//! - during the period: `v = (T_e - t)^3 / (3 (T_e - T_s)^2)`.
//!
//! With a period of length 0 this is the classical caplet on a forward-looking rate fixed at `T_s`
//! ([`crate::lmm::compute_caplet_value`]). This module has no joint paths of several rates; those
//! belong to the market model (`docs/spec_rates_fx_extension.md`).

// `!(x > 0.0)` is intended: it also rejects NaN
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::curves::YieldCurve;
use crate::lmm::{compute_caplet_value, CapletTerms};
use crate::ois::OvernightIndex;
use crate::ois_swap::{compute_overnight_compounding, create_overnight_coupon, Fixings, OvernightCompounding};
use crate::time::Date;
use crate::options::ForwardModel;
use crate::Error;

fn invalid<T>(reason: String) -> Result<T, Error> {
    Err(Error::InvalidInput { reason })
}

/// The forward of the compounded rate of a period, at `t`: `(realized * discount_start /
/// discount_end - 1) / accrual`. `realized` is the compounding factor of the fixings already
/// known, 1 before the period starts; `discount_start` is the discount factor to the later of `t`
/// and the period start, and `discount_end` to the period end.
pub fn compute_backward_forward_rate(realized: f64, discount_start: f64, discount_end: f64, accrual: f64) -> Result<f64, Error> {
    if !(realized > 0.0) || !(discount_start > 0.0) || !(discount_end > 0.0) || !(accrual > 0.0) {
        return invalid("the realized factor, the discount factors and the accrual must be positive".into());
    }
    Ok((realized * discount_start / discount_end - 1.0) / accrual)
}

/// The state of a backward-looking period at the evaluation date, from its daily fixings: the
/// compounding factor of the fixings already known, and the forward of the whole period, ready for
/// [`crate::lmm_market::LmmSpec`] (`discount_start` and `forwards[0]` of a period under way) and for
/// [`compute_backward_caplet_value`]. The days are the index's business days from `start` to `end`;
/// every fixing before the evaluation date is required and today's is used when given, as for an OIS
/// coupon ([`crate::ois_swap::compute_overnight_compounding`]).
pub fn compute_backward_period_state(
    index: &OvernightIndex,
    start: Date,
    end: Date,
    evaluation_date: Date,
    fixings: &Fixings,
    forecast: &YieldCurve,
) -> Result<OvernightCompounding, Error> {
    let coupon = create_overnight_coupon(index, start, end)?;
    compute_overnight_compounding(&coupon, forecast, evaluation_date, fixings)
}

/// A caplet on the compounded rate of one period, paid at the period end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackwardCapletTerms {
    /// The forward of the compounded rate, from [`compute_backward_forward_rate`].
    pub forward: f64,
    pub strike: f64,
    /// Annual volatility before the period: lognormal for Black, in rate units for Bachelier.
    pub volatility: f64,
    /// Years from the evaluation to the period start; negative during the period.
    pub time_to_start: f64,
    /// Years from the evaluation to the period end; positive.
    pub time_to_end: f64,
    /// Year fraction of the period.
    pub accrual: f64,
    /// Discount factor to the payment at the period end.
    pub discount_factor: f64,
    pub notional: f64,
}

/// The time `v` such that the rate's variance to the period end is `sigma^2 v`, for the linear
/// decay of the volatility through the period.
pub fn compute_backward_variance_time(time_to_start: f64, time_to_end: f64) -> Result<f64, Error> {
    if !time_to_start.is_finite() || !(time_to_end > 0.0) || !(time_to_end >= time_to_start) {
        return invalid(format!(
            "the period must end after the evaluation and not before it starts (to start {time_to_start}, to end {time_to_end})"
        ));
    }
    let length = time_to_end - time_to_start;
    Ok(if time_to_start >= 0.0 {
        time_to_start + length / 3.0
    } else {
        time_to_end.powi(3) / (3.0 * length * length)
    })
}

/// The value of the caplet (Lyashenko and Mercurio, "Looking forward to backward-looking rates").
pub fn compute_backward_caplet_value(model: ForwardModel, terms: &BackwardCapletTerms) -> Result<f64, Error> {
    let v = compute_backward_variance_time(terms.time_to_start, terms.time_to_end)?;
    compute_caplet_value(
        model,
        &CapletTerms {
            forward: terms.forward,
            strike: terms.strike,
            volatility: terms.volatility,
            expiry: v,
            accrual: terms.accrual,
            discount_factor: terms.discount_factor,
            notional: terms.notional,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::random::generate_standard_normal;

    fn terms(time_to_start: f64, time_to_end: f64) -> BackwardCapletTerms {
        BackwardCapletTerms {
            forward: 0.02,
            strike: 0.021,
            volatility: 0.3,
            time_to_start,
            time_to_end,
            accrual: 0.25,
            discount_factor: 0.98,
            notional: 1.0,
        }
    }

    /// Euler steps (100 paths) of `dR = sigma g(u) R dW` (or `sigma g(u) dW`) from 0 to `time_to_end`.
    fn simulate(model: ForwardModel, t: &BackwardCapletTerms, paths: u64, steps: usize) -> (f64, f64) {
        let (start, end) = (t.time_to_start, t.time_to_end);
        let g = |u: f64| if u <= start { 1.0 } else { (end - u) / (end - start) };
        let dt = end / steps as f64;
        let (mut sum, mut sum_squares) = (0.0, 0.0);
        for p in 0..paths {
            let mut r = t.forward;
            for k in 0..steps {
                let u = (k as f64 + 0.5) * dt;
                let z = generate_standard_normal(7, p, k as u64);
                let shock = t.volatility * g(u) * dt.sqrt() * z;
                r = match model {
                    // exact in each step for the lognormal rate
                    ForwardModel::Black => r * (shock - 0.5 * shock * shock).exp(),
                    ForwardModel::Bachelier => r + shock,
                };
            }
            let payoff = t.notional * t.accrual * t.discount_factor * (r - t.strike).max(0.0);
            sum += payoff;
            sum_squares += payoff * payoff;
        }
        let n = paths as f64;
        let mean = sum / n;
        (mean, ((sum_squares / n - mean * mean) / n).sqrt())
    }

    #[test]
    fn the_formula_agrees_with_euler_paths_before_and_during_the_period() {
        for (model, vol) in [(ForwardModel::Black, 0.3), (ForwardModel::Bachelier, 0.006)] {
            for (s, e) in [(0.75, 1.0), (-0.1, 0.15)] {
                let t = BackwardCapletTerms { volatility: vol, ..terms(s, e) };
                let analytic = compute_backward_caplet_value(model, &t).unwrap();
                let (mc, se) = simulate(model, &t, 100, 200);
                assert!((mc - analytic).abs() < 4.0 * se, "{model:?} {s} {e}: {analytic} against {mc} ± {se}");
            }
        }
    }

    #[test]
    fn limits_and_the_classical_caplet() {
        // a period of length 0 is the forward-looking caplet fixed at its start
        let classical = compute_caplet_value(
            ForwardModel::Black,
            &CapletTerms { forward: 0.02, strike: 0.021, volatility: 0.3, expiry: 0.75, accrual: 0.25, discount_factor: 0.98, notional: 1.0 },
        )
        .unwrap();
        let backward = compute_backward_caplet_value(ForwardModel::Black, &terms(0.75, 0.75 + 1e-12)).unwrap();
        assert!((backward - classical).abs() < 1e-12);
        // the decay through the period adds a third of its length to the variance time
        assert!((compute_backward_variance_time(0.75, 1.0).unwrap() - (0.75 + 0.25 / 3.0)).abs() < 1e-15);
        // so the backward-looking caplet is worth more than the forward-looking one
        assert!(compute_backward_caplet_value(ForwardModel::Black, &terms(0.75, 1.0)).unwrap() > classical);
        // the variance is continuous at the period start and vanishes at its end
        let at_start = compute_backward_variance_time(0.0, 0.25).unwrap();
        let just_after = compute_backward_variance_time(-1e-12, 0.25 - 1e-12).unwrap();
        assert!((at_start - just_after).abs() < 1e-10);
        let near_end = compute_backward_caplet_value(ForwardModel::Black, &BackwardCapletTerms { forward: 0.03, ..terms(-0.25 + 1e-9, 1e-9) }).unwrap();
        assert!((near_end - 0.25 * 0.98 * (0.03 - 0.021)).abs() < 1e-12);
        assert!(compute_backward_variance_time(0.5, 0.25).is_err());
        assert!(compute_backward_variance_time(-0.25, 0.0).is_err());
    }

    #[test]
    fn daily_fixings_give_the_realized_factor_and_the_forward_of_the_period() {
        use crate::interest_rate::{Compounding, InterestRate};
        use crate::ois::create_tonar_index;
        use crate::ois_swap::{compute_overnight_coupon_amount, create_ois_swap, OisSwapSpec, SwapSide};
        use crate::time::{Actual365Kind, DayCount};
        let d = |y: i32, m: u32, day: u32| Date::from_ymd(y, m, day).unwrap();
        let index = create_tonar_index();
        let (start, end) = (d(2026, 11, 4), d(2027, 2, 4));
        let today = d(2026, 12, 7);
        let curve = YieldCurve::flat(today, InterestRate::new(0.005, DayCount::Actual365Fixed(Actual365Kind::Standard), Compounding::Continuous, 0).unwrap());
        // before the period starts: no fixings, the curve's forward
        let early_curve = YieldCurve::flat(d(2026, 11, 2), InterestRate::new(0.005, DayCount::Actual365Fixed(Actual365Kind::Standard), Compounding::Continuous, 0).unwrap());
        let before = compute_backward_period_state(&index, start, end, d(2026, 11, 2), &Fixings::new(), &early_curve).unwrap();
        assert_eq!((before.realized, before.fixed_days), (1.0, 0));
        let expected = (early_curve.discount(start).unwrap() / early_curve.discount(end).unwrap() - 1.0) / before.accrual;
        assert!((before.rate - expected).abs() < 1e-15);
        // a month in: the fixings of the business days before today
        let coupon = create_overnight_coupon(&index, start, end).unwrap();
        let mut fixings = Fixings::new();
        let mut product = 1.0;
        for (k, (date, fraction)) in coupon.value_dates.iter().zip(&coupon.day_fractions).enumerate() {
            if *date >= today {
                break;
            }
            let rate = 0.0048 + 0.00001 * k as f64;
            fixings.insert(*date, rate);
            product *= 1.0 + rate * fraction;
        }
        let state = compute_backward_period_state(&index, start, end, today, &fixings, &curve).unwrap();
        assert!((state.realized - product).abs() < 1e-15);
        assert_eq!(state.fixed_days, fixings.len());
        assert!(state.fixed_days > 15 && state.fixed_days < state.days);
        // the same forward as the stage 3 formula from the realized factor and the curve
        let first_unfixed = coupon.value_dates[state.fixed_days];
        let formula = compute_backward_forward_rate(state.realized, curve.discount(first_unfixed).unwrap(), curve.discount(end).unwrap(), state.accrual).unwrap();
        assert!((state.rate - formula).abs() < 1e-15);
        // and the same rate as the overnight coupon of a one-period OIS swap (checked against QuantLib in P11-2)
        let swap = create_ois_swap(OisSwapSpec {
            side: SwapSide::Payer,
            nominal: 1.0,
            effective_date: start,
            termination_date: end,
            index: index.clone(),
            fixed_rate: 0.005,
            fixed_day_count: None,
            spread: 0.0,
            fixed_frequency: 1,
            overnight_frequency: 1,
            payment_lag: 0,
        })
        .unwrap();
        let amount = compute_overnight_coupon_amount(&swap, &swap.overnight_leg[0], &curve, today, &fixings).unwrap();
        assert!((amount / swap.overnight_leg[0].accrual - state.rate).abs() < 1e-15);
        // a missing fixing is an error
        let mut gap = fixings.clone();
        gap.remove(&coupon.value_dates[3]);
        assert!(compute_backward_period_state(&index, start, end, today, &gap, &curve).is_err());
    }

    #[test]
    fn the_forward_joins_realized_and_expected_compounding() {
        // before the period: the forward rate of the curve
        let before = compute_backward_forward_rate(1.0, 0.99, 0.985, 0.25).unwrap();
        assert!((before - (0.99 / 0.985 - 1.0) / 0.25).abs() < 1e-15);
        // during the period: the realized part times the expected rest
        let during = compute_backward_forward_rate(1.002, 1.0, 0.996, 0.25).unwrap();
        assert!((during - (1.002 / 0.996 - 1.0) / 0.25).abs() < 1e-15);
        assert!(compute_backward_forward_rate(0.0, 1.0, 0.99, 0.25).is_err());
    }
}
