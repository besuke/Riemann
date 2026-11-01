//! Power-reverse dual-currency coupons: `clamp(leverage * S / fx_base - subtraction, floor, cap)`,
//! paid on the domestic notional. The definitions follow GaussQuant `prdc_coupon_GQL` and
//! LagrangeFinance II-9 and III-7.
//!
//! The model is theirs: an FX rate that is lognormal between coupon dates, with deterministic domestic
//! and foreign curves and one volatility. Rates are not stochastic, there is one source of randomness,
//! and there is no call, target redemption or knock-out. The many sensitivities come from the curves.
//!
//! Each coupon is a constant plus a call spread on the FX rate, so the value has a closed form, which
//! tests the Monte Carlo. Both are generic over [`Operand`].

// `!(x > 0.0)` is intended: it also rejects NaN
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::autodiff::{Operand, Recorder, Value, Var};
use crate::batch::map_parallel;
use crate::curves::{CurveInput, YieldCurve};
use crate::math::random::standard_normal;
use crate::options::{forward_value, standard_error_of, ForwardInputs, ForwardModel, OptionType};
use crate::Error;
use rayon::prelude::*;

fn invalid<T>(reason: String) -> Result<T, Error> {
    Err(Error::InvalidInput { reason })
}

/// The terms of a note. A coupon date with a fixed rate pays that rate instead of the formula.
#[derive(Debug, Clone, PartialEq)]
pub struct PrdcTerms {
    pub notional: f64,
    pub redemption: f64,
    /// Coupon dates in years from the curves' reference date, increasing.
    pub coupon_times: Vec<f64>,
    pub accrual_fractions: Vec<f64>,
    pub fixed_rates: Vec<Option<f64>>,
    pub fx_base: f64,
    pub leverage: f64,
    pub subtraction: f64,
    pub floor: f64,
    /// Infinity for no cap.
    pub cap: f64,
}

impl PrdcTerms {
    fn check(&self) -> Result<(), Error> {
        let n = self.coupon_times.len();
        if n == 0 || self.accrual_fractions.len() != n || self.fixed_rates.len() != n {
            return invalid("a note needs coupon times, accrual fractions and fixed rates of the same length".into());
        }
        if self.coupon_times[0] <= 0.0 || self.coupon_times.windows(2).any(|w| w[1] <= w[0]) {
            return invalid("coupon times must be positive and increasing".into());
        }
        if !(self.fx_base > 0.0) || !(self.leverage > 0.0) {
            return invalid("the FX base and leverage must be positive".into());
        }
        if !(self.cap >= self.floor) {
            return invalid(format!("the cap ({}) is below the floor ({})", self.cap, self.floor));
        }
        Ok(())
    }
}

/// The market of a note. `spot` is domestic per foreign.
#[derive(Debug, Clone, Copy)]
pub struct PrdcMarket<'c, T = f64> {
    pub spot: T,
    pub volatility: T,
    pub domestic: &'c YieldCurve<T>,
    pub foreign: &'c YieldCurve<T>,
}

impl<T: Operand> PrdcMarket<'_, T> {
    fn check(&self) -> Result<(), Error> {
        if !(self.spot.primal() > 0.0) || !(self.volatility.primal() > 0.0) {
            return invalid("the spot and volatility must be positive".into());
        }
        Ok(())
    }

    fn domestic_discount(&self, t: f64) -> Result<T, Error> {
        self.domestic.discount_at(t, true)
    }

    fn forward(&self, t: f64) -> Result<T, Error> {
        Ok(self.spot * self.foreign.discount_at(t, true)? / self.domestic_discount(t)?)
    }
}

/// The closed-form value of the note, with the coupon as a floor plus a call spread on the FX rate.
pub fn prdc_analytic_value<T: Operand>(terms: &PrdcTerms, market: &PrdcMarket<T>) -> Result<T, Error> {
    terms.check()?;
    market.check()?;
    let scale = terms.leverage / terms.fx_base;
    let zero = market.spot.like(0.0);
    let one = market.spot.like(1.0);
    let call = |forward: T, strike: f64, t: f64| -> Result<T, Error> {
        if strike <= 0.0 {
            // always in the money
            return Ok(forward - strike);
        }
        forward_value(
            ForwardModel::Black,
            OptionType::Call,
            &ForwardInputs { forward, strike: zero.like(strike), volatility: market.volatility, time: zero.like(t), discount: one },
        )
    };
    let mut value = zero;
    let mut last_discount = one;
    for i in 0..terms.coupon_times.len() {
        let t = terms.coupon_times[i];
        let discount = market.domestic_discount(t)?;
        last_discount = discount;
        let rate = match terms.fixed_rates[i] {
            Some(fixed) => zero.like(fixed),
            None => {
                let forward = market.forward(t)?;
                let above_floor = call(forward, (terms.floor + terms.subtraction) / scale, t)?;
                let above_cap = if terms.cap.is_finite() {
                    call(forward, (terms.cap + terms.subtraction) / scale, t)?
                } else {
                    zero
                };
                (above_floor - above_cap) * scale + terms.floor
            }
        };
        value = value + rate * discount * (terms.notional * terms.accrual_fractions[i]);
    }
    Ok(value + last_discount * terms.redemption)
}

/// One input of the note and its derivative.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrdcCurveSensitivity {
    pub domestic: bool,
    pub input: CurveInput,
    pub d_npv: f64,
}

/// The value and first derivatives of a note.
#[derive(Debug, Clone, PartialEq)]
pub struct PrdcSensitivity {
    pub npv: f64,
    /// Zero for the closed form.
    pub standard_error: f64,
    pub fx_delta: f64,
    pub fx_vega: f64,
    pub curves: Vec<PrdcCurveSensitivity>,
}

/// The closed-form value and its derivatives, by automatic differentiation.
pub fn prdc_analytic(
    terms: &PrdcTerms,
    spot: f64,
    volatility: f64,
    domestic: &YieldCurve,
    foreign: &YieldCurve,
) -> Result<PrdcSensitivity, Error> {
    let recorder = Recorder::new();
    let (s, v) = (recorder.input(spot), recorder.input(volatility));
    let d = domestic.lift(&recorder)?;
    let f = foreign.lift(&recorder)?;
    let market = PrdcMarket { spot: s, volatility: v, domestic: &d.curve, foreign: &f.curve };
    let npv = prdc_analytic_value(terms, &market)?;
    sensitivity(&recorder, npv, 0.0, s, v, &d.inputs, &f.inputs)
}

fn sensitivity<'t>(
    recorder: &'t Recorder,
    npv: Var<'t>,
    standard_error: f64,
    spot: Var<'t>,
    volatility: Var<'t>,
    domestic: &[(CurveInput, Var<'t>)],
    foreign: &[(CurveInput, Var<'t>)],
) -> Result<PrdcSensitivity, Error> {
    let mut wrt = vec![spot, volatility];
    wrt.extend(domestic.iter().map(|(_, x)| *x));
    wrt.extend(foreign.iter().map(|(_, x)| *x));
    let g = recorder.gradient(npv, &wrt)?;
    let mut curves = Vec::with_capacity(wrt.len() - 2);
    for (k, (input, _)) in domestic.iter().enumerate() {
        curves.push(PrdcCurveSensitivity { domestic: true, input: *input, d_npv: g[2 + k].mean() });
    }
    for (k, (input, _)) in foreign.iter().enumerate() {
        curves.push(PrdcCurveSensitivity { domestic: false, input: *input, d_npv: g[2 + domestic.len() + k].mean() });
    }
    Ok(PrdcSensitivity { npv: npv.primal(), standard_error, fx_delta: g[0].mean(), fx_vega: g[1].mean(), curves })
}

/// Settings of a Monte Carlo evaluation of a note.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrdcMonteCarlo {
    pub paths: u64,
    pub seed: u64,
    /// Each normal number is also used with the opposite sign; `paths` must be even.
    pub antithetic: bool,
    /// Worker threads; the results do not depend on them. 0 means one per logical processor.
    pub threads: usize,
}

/// Monte Carlo on the coupon dates, with pathwise derivatives on path vectors. The kinks at the floor
/// and cap are differentiated as the slopes on each side, which is sound for these continuous coupons.
pub fn prdc_monte_carlo(
    terms: &PrdcTerms,
    spot: f64,
    volatility: f64,
    domestic: &YieldCurve,
    foreign: &YieldCurve,
    settings: &PrdcMonteCarlo,
) -> Result<PrdcSensitivity, Error> {
    terms.check()?;
    if settings.paths < 2 || (settings.antithetic && settings.paths % 2 != 0) {
        return invalid(format!("paths must be at least 2, and even with antithetic numbers (not {})", settings.paths));
    }
    map_parallel(&[()], settings.threads, |_| on_workers(terms, spot, volatility, domestic, foreign, settings))?
        .pop()
        .expect("one unit")
}

fn on_workers(
    terms: &PrdcTerms,
    spot: f64,
    volatility: f64,
    domestic: &YieldCurve,
    foreign: &YieldCurve,
    settings: &PrdcMonteCarlo,
) -> Result<PrdcSensitivity, Error> {
    let PrdcMonteCarlo { paths, seed, antithetic, .. } = *settings;
    let draws = if antithetic { paths / 2 } else { paths };
    let recorder = Recorder::new();
    let (s, v) = (recorder.input(spot), recorder.input(volatility));
    let d = domestic.lift(&recorder)?;
    let f = foreign.lift(&recorder)?;
    let market = PrdcMarket { spot: s, volatility: v, domestic: &d.curve, foreign: &f.curve };
    market.check()?;

    let scale = terms.leverage / terms.fx_base;
    let mut log_fx = s.ln();
    let mut previous_time = 0.0;
    let mut previous_forward = s;
    let mut total = recorder.constant(0.0);
    let mut last_discount = recorder.constant(1.0);
    for i in 0..terms.coupon_times.len() {
        let t = terms.coupon_times[i];
        let mut normals: Vec<f64> = (0..draws)
            .into_par_iter()
            .map(|p| standard_normal(seed, p, i as u64))
            .collect();
        if antithetic {
            let opposite: Vec<f64> = normals.iter().map(|z| -z).collect();
            normals.extend(opposite);
        }
        let z = recorder.input_paths(normals);
        let dt = t - previous_time;
        let forward = market.forward(t)?;
        log_fx = log_fx + (forward / previous_forward).ln() - v * v * 0.5 * dt + v * dt.sqrt() * z;
        previous_time = t;
        previous_forward = forward;
        let discount = market.domestic_discount(t)?;
        last_discount = discount;
        let rate = match terms.fixed_rates[i] {
            Some(fixed) => recorder.constant(fixed),
            None => {
                let raw = log_fx.exp() * scale - terms.subtraction;
                let floored = raw.max(recorder.constant(terms.floor));
                if terms.cap.is_finite() {
                    floored.min(recorder.constant(terms.cap))
                } else {
                    floored
                }
            }
        };
        total = total + rate * discount * (terms.notional * terms.accrual_fractions[i]);
    }
    let per_path = total + last_discount * terms.redemption;
    let npv = per_path.average();
    let per_path_value: Value = per_path.value();
    let standard_error = standard_error_of(&per_path_value, antithetic.then_some(draws));
    sensitivity(&recorder, npv, standard_error, s, v, &d.inputs, &f.inputs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curves::LogDiscountInterpolation;
    use crate::time::{Actual365Kind, Date, DayCount};

    fn dc() -> DayCount {
        DayCount::Actual365Fixed(Actual365Kind::Standard)
    }

    /// Discount factors from forward rates over each year, as in LagrangeFinance III-7.
    fn curve(rates: &[f64]) -> YieldCurve {
        let today = Date::from_ymd(2026, 11, 2).unwrap();
        let mut dates = vec![today];
        let mut discounts = vec![1.0];
        let mut sum = 0.0;
        for (i, r) in rates.iter().enumerate() {
            sum += r;
            dates.push(Date::from_serial(today.serial() as i64 + 365 * (i as i64 + 1)).unwrap());
            discounts.push((-sum).exp());
        }
        YieldCurve::interpolated_discounts(dates, discounts, dc(), LogDiscountInterpolation::LogLinear).unwrap()
    }

    fn jpy() -> YieldCurve {
        curve(&[0.01, 0.012, 0.014, 0.016, 0.018])
    }

    fn foreign() -> YieldCurve {
        curve(&[0.04, 0.038, 0.036, 0.034, 0.032])
    }

    /// The note of LagrangeFinance III-7: `fixed + leverage * (S / reference - 1)`, floored at 0.
    fn terms() -> PrdcTerms {
        PrdcTerms {
            notional: 1e8,
            redemption: 1e8,
            coupon_times: vec![1.0, 2.0, 3.0, 4.0, 5.0],
            accrual_fractions: vec![1.0; 5],
            fixed_rates: vec![None; 5],
            fx_base: 150.0,
            leverage: 0.8,
            subtraction: 0.8 - 0.02,
            floor: 0.0,
            cap: 0.08,
        }
    }

    fn settings(threads: usize) -> PrdcMonteCarlo {
        PrdcMonteCarlo { paths: 400_000, seed: 20261101, antithetic: true, threads }
    }

    #[test]
    fn monte_carlo_agrees_with_the_closed_form() {
        let (d, f) = (jpy(), foreign());
        let a = prdc_analytic(&terms(), 150.0, 0.12, &d, &f).unwrap();
        let m = prdc_monte_carlo(&terms(), 150.0, 0.12, &d, &f, &settings(4)).unwrap();
        assert!((m.npv - a.npv).abs() < 5.0 * m.standard_error, "{} {} {}", m.npv, a.npv, m.standard_error);
        assert!((m.fx_delta - a.fx_delta).abs() < 0.02 * a.fx_delta.abs(), "{} {}", m.fx_delta, a.fx_delta);
        assert!((m.fx_vega - a.fx_vega).abs() < 0.03 * a.fx_vega.abs(), "{} {}", m.fx_vega, a.fx_vega);
        for (x, y) in m.curves.iter().zip(&a.curves) {
            assert_eq!(x.input, y.input);
            assert!((x.d_npv - y.d_npv).abs() < 0.03 * y.d_npv.abs().max(1e6), "{x:?} {y:?}");
        }
    }

    #[test]
    fn the_closed_form_derivatives_agree_with_numeric_differences() {
        let (d, f) = (jpy(), foreign());
        let a = prdc_analytic(&terms(), 150.0, 0.12, &d, &f).unwrap();
        let value = |s: f64, v: f64| {
            prdc_analytic_value(&terms(), &PrdcMarket { spot: s, volatility: v, domestic: &d, foreign: &f }).unwrap()
        };
        let delta = (value(150.0 + 1e-3, 0.12) - value(150.0 - 1e-3, 0.12)) / 2e-3;
        let vega = (value(150.0, 0.12 + 1e-5) - value(150.0, 0.12 - 1e-5)) / 2e-5;
        assert!((a.fx_delta - delta).abs() < 1e-5 * delta.abs(), "{} {delta}", a.fx_delta);
        assert!((a.fx_vega - vega).abs() < 1e-5 * vega.abs(), "{} {vega}", a.fx_vega);
        // a foreign node, against rebuilding the curve
        let node = 3usize;
        let rebuilt = |h: f64| {
            let today = Date::from_ymd(2026, 11, 2).unwrap();
            let rates = [0.04, 0.038, 0.036, 0.034, 0.032];
            let mut dates = vec![today];
            let mut discounts = vec![1.0];
            let mut sum = 0.0;
            for (i, r) in rates.iter().enumerate() {
                sum += r;
                dates.push(Date::from_serial(today.serial() as i64 + 365 * (i as i64 + 1)).unwrap());
                discounts.push((-sum + if i == node { h } else { 0.0 }).exp());
            }
            let bumped = YieldCurve::interpolated_discounts(dates, discounts, dc(), LogDiscountInterpolation::LogLinear).unwrap();
            prdc_analytic_value(&terms(), &PrdcMarket { spot: 150.0, volatility: 0.12, domestic: &d, foreign: &bumped }).unwrap()
        };
        let numeric = (rebuilt(1e-6) - rebuilt(-1e-6)) / 2e-6;
        let got = a.curves.iter().filter(|c| !c.domestic).nth(node).unwrap().d_npv;
        assert!((got - numeric).abs() < 1e-4 * numeric.abs().max(1.0), "{got} {numeric}");
    }

    #[test]
    fn pathwise_derivatives_are_the_derivatives_of_the_same_paths() {
        let (d, f) = (jpy(), foreign());
        let s = PrdcMonteCarlo { paths: 40_000, ..settings(2) };
        let m = prdc_monte_carlo(&terms(), 150.0, 0.12, &d, &f, &s).unwrap();
        let value = |x: f64, v: f64| prdc_monte_carlo(&terms(), x, v, &d, &f, &s).unwrap().npv;
        let delta = (value(150.0 + 1e-3, 0.12) - value(150.0 - 1e-3, 0.12)) / 2e-3;
        let vega = (value(150.0, 0.12 + 1e-5) - value(150.0, 0.12 - 1e-5)) / 2e-5;
        assert!((m.fx_delta - delta).abs() < 1e-3 * delta.abs(), "{} {delta}", m.fx_delta);
        assert!((m.fx_vega - vega).abs() < 1e-3 * vega.abs(), "{} {vega}", m.fx_vega);
    }

    #[test]
    fn monte_carlo_does_not_depend_on_the_number_of_threads() {
        let (d, f) = (jpy(), foreign());
        let s = |t| PrdcMonteCarlo { paths: 40_000, ..settings(t) };
        let one = prdc_monte_carlo(&terms(), 150.0, 0.12, &d, &f, &s(1)).unwrap();
        assert_eq!(one, prdc_monte_carlo(&terms(), 150.0, 0.12, &d, &f, &s(4)).unwrap());
    }

    #[test]
    fn fixed_coupons_and_no_cap_behave() {
        let (d, f) = (jpy(), foreign());
        let mut fixed = terms();
        fixed.fixed_rates = vec![Some(0.03); 5];
        let v = prdc_analytic(&fixed, 150.0, 0.12, &d, &f).unwrap();
        // a note with fixed coupons has no FX risk
        assert_eq!(v.fx_delta, 0.0);
        assert_eq!(v.fx_vega, 0.0);
        let exact: f64 = (1..=5).map(|i| 0.03 * 1e8 * d.discount_at(i as f64, true).unwrap()).sum::<f64>()
            + 1e8 * d.discount_at(5.0, true).unwrap();
        assert!((v.npv - exact).abs() < 1e-6);
        let mut uncapped = terms();
        uncapped.cap = f64::INFINITY;
        let capped = prdc_analytic(&terms(), 150.0, 0.12, &d, &f).unwrap();
        assert!(prdc_analytic(&uncapped, 150.0, 0.12, &d, &f).unwrap().npv > capped.npv);
    }

    #[test]
    fn invalid_terms_are_errors() {
        let (d, f) = (jpy(), foreign());
        let mut t = terms();
        t.cap = -1.0;
        assert!(prdc_analytic(&t, 150.0, 0.12, &d, &f).is_err());
        let mut t = terms();
        t.coupon_times = vec![1.0, 1.0, 3.0, 4.0, 5.0];
        assert!(prdc_analytic(&t, 150.0, 0.12, &d, &f).is_err());
        assert!(prdc_analytic(&terms(), -1.0, 0.12, &d, &f).is_err());
    }
}
