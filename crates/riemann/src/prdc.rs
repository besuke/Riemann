//! Power-reverse dual-currency coupons: `clamp(leverage * S / fx_base - subtraction, floor, cap)`,
//! paid on the domestic notional. The definitions follow GaussQuant `prdc_coupon_GQL` and
//! LagrangeFinance II-9 and III-7.
//!
//! The first model is theirs: an FX rate that is lognormal between coupon dates, with deterministic domestic
//! and foreign curves and one volatility. Rates are not stochastic, there is one source of randomness,
//! and there is no call, target redemption or knock-out. The many sensitivities come from the curves.
//!
//! Each coupon is a constant plus a call spread on the FX rate, so the value has a closed form, which
//! tests the Monte Carlo. Both are generic over [`Operand`].
//!
//! A second model has stochastic rates ([`PrdcThreeFactor`]): Hull-White domestic and foreign rates and
//! a lognormal FX rate under the domestic measure, with a constant or piecewise-constant FX volatility
//! ([`FxVolatilityTerm`]), a closed form per coupon (the FX forward is lognormal under each coupon's
//! forward measure) and a Monte Carlo of the three factors, with calls, target redemptions and
//! knock-outs (`docs/spec_prdc.md` 5〜13 章).

// `!(x > 0.0)` is intended: it also rejects NaN
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::autodiff::{Operand, Recorder, Value, Var};
use crate::batch::map_parallel;
use crate::curves::{CurveInput, YieldCurve};
use crate::math::random::generate_standard_normal;
use crate::options::{compute_forward_value, standard_error_of, ForwardInputs, ForwardModel, OptionType};
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
pub fn compute_prdc_analytic_value<T: Operand>(terms: &PrdcTerms, market: &PrdcMarket<T>) -> Result<T, Error> {
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
        compute_forward_value(
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
pub fn compute_prdc_analytic(
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
    let npv = compute_prdc_analytic_value(terms, &market)?;
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
pub fn compute_prdc_monte_carlo(
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
            .map(|p| generate_standard_normal(seed, p, i as u64))
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

// ---- the three-factor model: Hull-White domestic and foreign rates and a lognormal FX rate ----

/// A Hull-White short rate `r = x + phi(t)`, `dx = -a x dt + sigma dW`, with `phi` fitting the curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HullWhiteFactor {
    pub mean_reversion: f64,
    pub volatility: f64,
}

/// The three-factor market of a note, under the domestic risk-neutral measure:
/// `dr_d = (theta_d - a_d r_d) dt + sigma_d dW_d`,
/// `dr_f = (theta_f - a_f r_f - rho_fX sigma_f sigma_X) dt + sigma_f dW_f` (the quanto drift),
/// `dS / S = (r_d - r_f) dt + sigma_X(t) dW_X`, with `sigma_X` constant (`fx_volatility`) or
/// piecewise constant (`fx_volatility_term`).
#[derive(Debug, Clone, Copy)]
pub struct PrdcThreeFactor<'c> {
    /// Domestic per foreign.
    pub spot: f64,
    /// The constant FX volatility; not used with `fx_volatility_term`.
    pub fx_volatility: f64,
    /// A piecewise-constant FX volatility in place of `fx_volatility`.
    pub fx_volatility_term: Option<FxVolatilityTerm<'c>>,
    pub domestic: &'c YieldCurve,
    pub foreign: &'c YieldCurve,
    pub domestic_rate: HullWhiteFactor,
    pub foreign_rate: HullWhiteFactor,
    /// `rho(W_d, W_f)`, `rho(W_d, W_X)`, `rho(W_f, W_X)`.
    pub correlation: [f64; 3],
}

/// `B(tau) = (1 - exp(-a tau)) / a`, `tau` when `a` is 0.
fn hull_white_b(a: f64, tau: f64) -> f64 {
    if (a * tau).abs() < 1e-10 {
        tau * (1.0 - 0.5 * a * tau)
    } else {
        (1.0 - (-a * tau).exp()) / a
    }
}

/// `Var(int_0^T x du)` for `dx = -a x dt + sigma dW`, `x(0) = 0`.
fn integrated_variance(f: &HullWhiteFactor, t: f64) -> f64 {
    let a = f.mean_reversion;
    if (a * t).abs() < 1e-6 {
        return f.volatility * f.volatility * t * t * t / 3.0;
    }
    f.volatility * f.volatility / (a * a) * (t - 2.0 * hull_white_b(a, t) + hull_white_b(2.0 * a, t))
}

/// A piecewise-constant FX volatility: `volatilities[k]` from `times[k - 1]` (0 for the first) to
/// `times[k]`, and the last one after the last time. From ATM volatilities by maturity,
/// [`calibrate_prdc_fx_volatility_term`] gives the pieces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FxVolatilityTerm<'c> {
    /// The ends of the pieces, in years, positive and increasing.
    pub times: &'c [f64],
    pub volatilities: &'c [f64],
}

impl FxVolatilityTerm<'_> {
    fn check(&self) -> Result<(), Error> {
        if self.times.is_empty() || self.times.len() != self.volatilities.len() {
            return invalid("give one FX volatility per piece's end time".into());
        }
        if !(self.times[0] > 0.0) || self.times.windows(2).any(|w| !(w[1] > w[0])) || !self.times.iter().all(|t| t.is_finite()) {
            return invalid("the FX volatility's times must be positive, finite and increasing".into());
        }
        if !self.volatilities.iter().all(|v| *v > 0.0 && v.is_finite()) {
            return invalid("the FX volatilities must be positive and finite".into());
        }
        Ok(())
    }

    /// `int_0^t sigma_X(u)^2 du`.
    pub fn total_variance(&self, t: f64) -> f64 {
        split_pieces(self.times, 0.0, t).iter().map(|&(a, b, k)| self.volatilities[k] * self.volatilities[k] * (b - a)).sum()
    }
}

/// The pieces of a piecewise-constant function (piece `k` ends at `ends[k]`, the last piece goes on, and
/// no ends is one piece) that cover `[t0, t1]`: `(start, end, k)` in order.
fn split_pieces(ends: &[f64], t0: f64, t1: f64) -> Vec<(f64, f64, usize)> {
    let last = ends.len().saturating_sub(1);
    let mut k = ends.iter().position(|&e| e > t0).unwrap_or(last);
    let mut start = t0;
    let mut out = Vec::new();
    while k < last && ends[k] < t1 {
        out.push((start, ends[k], k));
        start = ends[k];
        k += 1;
    }
    out.push((start, t1, k));
    out
}

impl PrdcThreeFactor<'_> {
    /// The FX volatility's pieces over `[t0, t1]`: `(start, end, sigma_X)`.
    fn fx_pieces(&self, t0: f64, t1: f64) -> Vec<(f64, f64, f64)> {
        match self.fx_volatility_term {
            None => vec![(t0, t1, self.fx_volatility)],
            Some(term) => split_pieces(term.times, t0, t1).into_iter().map(|(a, b, k)| (a, b, term.volatilities[k])).collect(),
        }
    }

    /// `int_0^t sigma_X(u)^2 du`.
    pub fn fx_total_variance(&self, t: f64) -> f64 {
        match self.fx_volatility_term {
            None => self.fx_volatility * self.fx_volatility * t,
            Some(term) => term.total_variance(t),
        }
    }

    fn check(&self) -> Result<Vec<Vec<f64>>, Error> {
        if !(self.spot > 0.0) || !self.spot.is_finite() {
            return invalid("the spot must be positive and finite".into());
        }
        match self.fx_volatility_term {
            Some(term) => term.check()?,
            None if !(self.fx_volatility > 0.0) || !self.fx_volatility.is_finite() => {
                return invalid("the FX volatility must be positive and finite".into());
            }
            None => {}
        }
        for f in [self.domestic_rate, self.foreign_rate] {
            if !(f.mean_reversion >= 0.0) || !(f.volatility >= 0.0) || !f.mean_reversion.is_finite() || !f.volatility.is_finite() {
                return invalid("the rates' mean reversions and volatilities must be finite and non-negative".into());
            }
        }
        let [df, dx, fx] = self.correlation;
        let rho = [1.0, df, dx, df, 1.0, fx, dx, fx, 1.0];
        crate::lmm::check_correlation_entries(&rho, 3)?;
        // positive semi-definite: the determinant and the minors
        let det = 1.0 + 2.0 * df * dx * fx - df * df - dx * dx - fx * fx;
        if det < -1e-12 {
            return invalid("the correlations are not a correlation matrix (negative determinant)".into());
        }
        Ok(vec![vec![1.0, df, dx], vec![df, 1.0, fx], vec![dx, fx, 1.0]])
    }

}

/// The parameters of the three-factor model as operands, for one definition of the variance and the
/// closed form in `f64` and under automatic differentiation.
#[derive(Debug, Clone)]
struct ThreeFactorParameters<T> {
    /// The FX volatility of each piece, ending at `fx_ends` (none for a constant volatility).
    fx_volatility: Vec<T>,
    fx_ends: Vec<f64>,
    domestic: (T, T),
    foreign: (T, T),
    correlation: [T; 3],
}

/// The nodes and weights of Gauss-Legendre quadrature on `[-1, 1]` with 32 points, from the
/// eigenvalues of the Jacobi matrix (Golub and Welsch).
fn gauss_legendre() -> &'static (Vec<f64>, Vec<f64>) {
    static RULE: std::sync::OnceLock<(Vec<f64>, Vec<f64>)> = std::sync::OnceLock::new();
    RULE.get_or_init(|| {
        let n = 32;
        let mut jacobi = vec![vec![0.0; n]; n];
        for k in 1..n {
            let b = k as f64 / ((4 * k * k - 1) as f64).sqrt();
            jacobi[k][k - 1] = b;
            jacobi[k - 1][k] = b;
        }
        let (values, vectors) = crate::math::linear::compute_symmetric_eigen(&jacobi).expect("a symmetric matrix");
        let weights = (0..n).map(|k| 2.0 * vectors[0][k] * vectors[0][k]).collect();
        (values, weights)
    })
}

/// `B(tau) = (1 - exp(-a tau)) / a` for an operand `a`, by its series when `a tau` is small.
fn hull_white_b_of<T: Operand>(a: T, tau: f64) -> T {
    let x = a * tau;
    if x.primal().abs() < 1e-5 {
        // tau (1 - x / 2 + x^2 / 6), the error below x^3 / 24
        (x * x / 6.0 - x * 0.5 + 1.0) * tau
    } else {
        (-((-x).exp()) + 1.0) / a
    }
}

/// `V(T)` of [`PrdcThreeFactor::fx_forward_variance`]: the integrand is smooth on each piece of the FX
/// volatility, and 32-point Gauss-Legendre quadrature on each piece integrates it to rounding for the
/// mean reversions of practice.
fn fx_forward_variance_of<T: Operand>(p: &ThreeFactorParameters<T>, t: f64) -> T {
    let (nodes, weights) = gauss_legendre();
    let [df, dx, fx] = p.correlation;
    let mut sum = p.fx_volatility[0].like(0.0);
    for (u0, u1, k) in split_pieces(&p.fx_ends, 0.0, t) {
        let sx = p.fx_volatility[k];
        // tau = T - u over the piece
        let (lo, hi) = (t - u1, t - u0);
        for (node, weight) in nodes.iter().zip(weights) {
            let tau = lo + 0.5 * (hi - lo) * (node + 1.0);
            let bd = p.domestic.1 * hull_white_b_of(p.domestic.0, tau);
            let bf = p.foreign.1 * hull_white_b_of(p.foreign.0, tau);
            let integrand = sx * sx + bf * bf + bd * bd - fx * sx * bf * 2.0 + dx * sx * bd * 2.0 - df * bd * bf * 2.0;
            sum = sum + integrand * (0.5 * (hi - lo) * weight);
        }
    }
    sum
}

/// The closed form for operands: each coupon is the call spread of [`compute_prdc_analytic_value`] with
/// the volatility `sqrt(V(T) / T)`.
fn three_factor_analytic_of<T: Operand>(
    terms: &PrdcTerms,
    spot: T,
    domestic: &YieldCurve<T>,
    foreign: &YieldCurve<T>,
    p: &ThreeFactorParameters<T>,
) -> Result<T, Error> {
    let mut value = spot.like(0.0);
    let mut last_discount = spot.like(1.0);
    for i in 0..terms.coupon_times.len() {
        let t = terms.coupon_times[i];
        let one = PrdcTerms {
            coupon_times: vec![t],
            accrual_fractions: vec![terms.accrual_fractions[i]],
            fixed_rates: vec![terms.fixed_rates[i]],
            redemption: 0.0,
            ..terms.clone()
        };
        let volatility = (fx_forward_variance_of(p, t) / t).sqrt();
        let market = PrdcMarket { spot, volatility, domestic, foreign };
        value = value + compute_prdc_analytic_value(&one, &market)?;
        last_discount = domestic.discount_at(t, true)?;
    }
    Ok(value + last_discount * terms.redemption)
}

impl PrdcThreeFactor<'_> {
    fn parameters(&self) -> ThreeFactorParameters<f64> {
        let (fx_volatility, fx_ends) = match self.fx_volatility_term {
            None => (vec![self.fx_volatility], Vec::new()),
            Some(term) => (term.volatilities.to_vec(), term.times.to_vec()),
        };
        ThreeFactorParameters {
            fx_volatility,
            fx_ends,
            domestic: (self.domestic_rate.mean_reversion, self.domestic_rate.volatility),
            foreign: (self.foreign_rate.mean_reversion, self.foreign_rate.volatility),
            correlation: self.correlation,
        }
    }

    /// The variance of `ln F(T, T)` with `F(t, T) = S(t) P_f(t, T) / P_d(t, T)`, a martingale under the
    /// domestic `T`-forward measure: `int_0^T |sigma_X dW_X - sigma_f B_f dW_f + sigma_d B_d dW_d|^2`, by
    /// 32-point Gauss-Legendre quadrature (the integrand is smooth).
    pub fn fx_forward_variance(&self, t: f64) -> f64 {
        fx_forward_variance_of(&self.parameters(), t)
    }
}

/// The closed-form value of the note in the three-factor model: each coupon is the call spread of
/// [`compute_prdc_analytic_value`] on the forward `S P_f(0, T) / P_d(0, T)`, with the volatility
/// `sqrt(V(T) / T)` of [`PrdcThreeFactor::fx_forward_variance`], discounted by `P_d(0, T)`. Exact,
/// since the coupon depends only on the FX rate on its own date.
pub fn compute_prdc_three_factor_analytic(terms: &PrdcTerms, model: &PrdcThreeFactor) -> Result<f64, Error> {
    terms.check()?;
    model.check()?;
    three_factor_analytic_of(terms, model.spot, model.domestic, model.foreign, &model.parameters())
}

/// The first derivatives of the three-factor closed form.
#[derive(Debug, Clone, PartialEq)]
pub struct PrdcThreeFactorSensitivity {
    pub npv: f64,
    pub fx_delta: f64,
    /// With respect to a parallel shift of the FX volatility (every piece by the same amount).
    pub fx_vega: f64,
    /// With respect to each piece of the FX volatility (one entry for a constant one).
    pub fx_vega_term: Vec<f64>,
    /// With respect to the domestic rate's mean reversion and volatility.
    pub domestic_mean_reversion: f64,
    pub domestic_volatility: f64,
    pub foreign_mean_reversion: f64,
    pub foreign_volatility: f64,
    /// With respect to `rho_df`, `rho_dX`, `rho_fX`.
    pub correlation: [f64; 3],
    /// With respect to the curves' inputs, as [`compute_prdc_analytic`].
    pub curves: Vec<PrdcCurveSensitivity>,
}

/// The closed-form value of the note in the three-factor model and its derivatives with respect to
/// every input, by automatic differentiation in one pass.
pub fn compute_prdc_three_factor_sensitivity(terms: &PrdcTerms, model: &PrdcThreeFactor) -> Result<PrdcThreeFactorSensitivity, Error> {
    terms.check()?;
    model.check()?;
    let recorder = Recorder::new();
    let spot = recorder.input(model.spot);
    let plain = model.parameters();
    let fx_volatility: Vec<Var> = plain.fx_volatility.iter().map(|&v| recorder.input(v)).collect();
    let pieces = fx_volatility.len();
    let domestic = (recorder.input(model.domestic_rate.mean_reversion), recorder.input(model.domestic_rate.volatility));
    let foreign = (recorder.input(model.foreign_rate.mean_reversion), recorder.input(model.foreign_rate.volatility));
    let correlation = model.correlation.map(|r| recorder.input(r));
    let d = model.domestic.lift(&recorder)?;
    let f = model.foreign.lift(&recorder)?;
    let p = ThreeFactorParameters { fx_volatility: fx_volatility.clone(), fx_ends: plain.fx_ends, domestic, foreign, correlation };
    let npv = three_factor_analytic_of(terms, spot, &d.curve, &f.curve, &p)?;
    let mut wrt = vec![spot, domestic.0, domestic.1, foreign.0, foreign.1, correlation[0], correlation[1], correlation[2]];
    wrt.extend(fx_volatility.iter().copied());
    let first_curve = wrt.len();
    wrt.extend(d.inputs.iter().map(|(_, x)| *x));
    wrt.extend(f.inputs.iter().map(|(_, x)| *x));
    let g: Vec<f64> = recorder.gradient(npv, &wrt)?.iter().map(|x| x.mean()).collect();
    let mut curves = Vec::with_capacity(d.inputs.len() + f.inputs.len());
    for (k, (input, _)) in d.inputs.iter().enumerate() {
        curves.push(PrdcCurveSensitivity { domestic: true, input: *input, d_npv: g[first_curve + k] });
    }
    for (k, (input, _)) in f.inputs.iter().enumerate() {
        curves.push(PrdcCurveSensitivity { domestic: false, input: *input, d_npv: g[first_curve + d.inputs.len() + k] });
    }
    let fx_vega_term = g[8..8 + pieces].to_vec();
    Ok(PrdcThreeFactorSensitivity {
        npv: npv.primal(),
        fx_delta: g[0],
        fx_vega: fx_vega_term.iter().sum(),
        fx_vega_term,
        domestic_mean_reversion: g[1],
        domestic_volatility: g[2],
        foreign_mean_reversion: g[3],
        foreign_volatility: g[4],
        correlation: [g[5], g[6], g[7]],
        curves,
    })
}

/// The states of the three-factor paths on the coupon dates: `discount[p][i]` is
/// `exp(-int_0^{T_i} r_d)` and `fx[p][i]` is `S(T_i)` on path `p`.
#[derive(Debug, Clone, PartialEq)]
pub struct PrdcThreeFactorPaths {
    pub discount: Vec<Vec<f64>>,
    pub fx: Vec<Vec<f64>>,
    /// `x_d(T_i)` and `x_f(T_i)`, the deviations of the short rates from their fitted means (`x_f` under
    /// the domestic measure): the rates' state for regressions.
    pub domestic_factor: Vec<Vec<f64>>,
    pub foreign_factor: Vec<Vec<f64>>,
}

/// Paths of the three factors on the coupon dates, with steps of at most `1 / steps_per_year` years
/// between them. `x_d` and `x_f` move by their exact Ornstein-Uhlenbeck transitions (with the quanto
/// drift on `x_f`) jointly with `W_X`; the integrals of `x` are by the trapezoid rule. With `phi` fitting
/// each curve, `exp(-int r_d) = P_d(0, T) exp(-I_d - V_d / 2)` and
/// `ln S(T) = ln S + ln(P_f(0, T) / P_d(0, T)) + I_d - I_f + (V_d - V_f) / 2 - w(T) / 2 + int_0^T sigma_X dW_X`,
/// with `I = int_0^T x`, `V = Var(I)` (the foreign one under the foreign measure) and the FX total
/// variance `w(T) = int_0^T sigma_X^2`. A piecewise-constant FX volatility enters each step through the
/// increments of `w` and of its covariances with the rates over the step, so the time grid and the
/// random numbers are those of a constant volatility.
pub fn generate_prdc_three_factor_paths(
    coupon_times: &[f64],
    model: &PrdcThreeFactor,
    paths: usize,
    steps_per_year: usize,
    seed: u64,
) -> Result<PrdcThreeFactorPaths, Error> {
    let correlation = model.check()?;
    if coupon_times.is_empty() || coupon_times[0] <= 0.0 || coupon_times.windows(2).any(|w| w[1] <= w[0]) {
        return invalid("coupon times must be positive and increasing".into());
    }
    if paths < 2 || steps_per_year == 0 || paths.saturating_mul(coupon_times.len()) > 100_000_000 {
        return invalid(format!("give at least 2 paths and 1 step a year, and not too many paths ({paths})"));
    }
    // the time grid
    let mut grid = vec![0.0];
    for &t in coupon_times {
        let from = *grid.last().expect("not empty");
        let pieces = ((t - from) * steps_per_year as f64).ceil().max(1.0) as usize;
        for k in 1..=pieces {
            grid.push(if k == pieces { t } else { from + (t - from) * k as f64 / pieces as f64 });
        }
    }
    let (d, f) = (model.domestic_rate, model.foreign_rate);
    let [_, _, rho_fx] = model.correlation;
    // the conditional covariance of (x_d, x_f, int sigma_X dW_X) over a step [t0, t1], as
    // Ornstein-Uhlenbeck increments (the FX one with no mean reversion), and its Cholesky factor (zero
    // rate volatilities left out); with the FX volatility constant over the step, the entries are
    // rho_ab sigma_a sigma_b B(kappa_a + kappa_b, h), and across pieces of it the FX entries add up
    // over the pieces: int sigma_X^2 du and rho sigma_r int exp(-kappa_r (t1 - u)) sigma_X(u) du
    let kappa = [d.mean_reversion, f.mean_reversion, 0.0];
    let active: Vec<usize> = (0..3).filter(|&j| j == 2 || [d.volatility, f.volatility][j] > 0.0).collect();
    let step_factor = |t0: f64, t1: f64| -> Result<(Vec<Vec<f64>>, f64), Error> {
        let h = t1 - t0;
        let pieces = model.fx_pieces(t0, t1);
        let single = pieces.len() == 1;
        let sigma = [d.volatility, f.volatility, pieces[0].2];
        // int exp(-kappa (t1 - u)) sigma_X(u)^power du over the step
        let fx_integral = |kappa: f64, power: i32| -> f64 {
            pieces.iter().map(|&(a, b, s)| s.powi(power) * (-kappa * (t1 - b)).exp() * hull_white_b(kappa, b - a)).sum()
        };
        let entry = |a: usize, b: usize| -> f64 {
            if single || (a != 2 && b != 2) {
                correlation[a][b] * sigma[a] * sigma[b] * hull_white_b(kappa[a] + kappa[b], h)
            } else if a == 2 && b == 2 {
                fx_integral(0.0, 2)
            } else {
                let r = if a == 2 { b } else { a };
                correlation[a][b] * sigma[r] * fx_integral(kappa[r], 1)
            }
        };
        let cov: Vec<Vec<f64>> = active.iter().map(|&a| active.iter().map(|&b| entry(a, b)).collect()).collect();
        // the quanto drift of x_f over the step
        let quanto = if single {
            rho_fx * f.volatility * sigma[2] * hull_white_b(f.mean_reversion, h)
        } else {
            rho_fx * f.volatility * fx_integral(f.mean_reversion, 1)
        };
        Ok((crate::math::linear::compute_cholesky(&cov)?, quanto))
    };
    let steps: Vec<(Vec<Vec<f64>>, f64)> = grid.windows(2).map(|w| step_factor(w[0], w[1])).collect::<Result<_, _>>()?;
    let mut fixed = Vec::with_capacity(coupon_times.len());
    for &t in coupon_times {
        let (pd, pf) = (model.domestic.discount_at(t, true)?, model.foreign.discount_at(t, true)?);
        let (vd, vf) = (integrated_variance(&d, t), integrated_variance(&f, t));
        let half_fx_variance = match model.fx_volatility_term {
            None => 0.5 * model.fx_volatility * model.fx_volatility * t,
            Some(_) => 0.5 * model.fx_total_variance(t),
        };
        fixed.push((pd, (model.spot * pf / pd).ln() + 0.5 * (vd - vf) - half_fx_variance, vd));
    }
    let results: Vec<[Vec<f64>; 4]> = (0..paths)
        .into_par_iter()
        .map(|p| {
            let (mut xd, mut xf, mut w) = (0.0, 0.0, 0.0);
            let (mut id, mut i_f) = (0.0, 0.0);
            let mut next = 0;
            let mut discount = Vec::with_capacity(coupon_times.len());
            let mut fx = Vec::with_capacity(coupon_times.len());
            let (mut fd, mut ff) = (Vec::with_capacity(coupon_times.len()), Vec::with_capacity(coupon_times.len()));
            for (k, step) in grid.windows(2).enumerate() {
                let h = step[1] - step[0];
                let z: Vec<f64> = (0..active.len()).map(|j| generate_standard_normal(seed, p as u64, (3 * k + j) as u64)).collect();
                let mut shock = [0.0; 3];
                for (row, &a) in active.iter().enumerate() {
                    shock[a] = steps[k].0[row].iter().zip(&z).map(|(l, x)| l * x).sum();
                }
                let new_xd = xd * (-d.mean_reversion * h).exp() + shock[0];
                let new_xf = xf * (-f.mean_reversion * h).exp() - steps[k].1 + shock[1];
                id += 0.5 * h * (xd + new_xd);
                i_f += 0.5 * h * (xf + new_xf);
                xd = new_xd;
                xf = new_xf;
                w += shock[2];
                if step[1] == coupon_times[next] {
                    let (pd, log_fx, vd) = fixed[next];
                    discount.push(pd * (-id - 0.5 * vd).exp());
                    fx.push((log_fx + id - i_f + w).exp());
                    fd.push(xd);
                    ff.push(xf);
                    next += 1;
                }
            }
            [discount, fx, fd, ff]
        })
        .collect();
    let mut out = PrdcThreeFactorPaths { discount: Vec::with_capacity(paths), fx: Vec::with_capacity(paths), domestic_factor: Vec::with_capacity(paths), foreign_factor: Vec::with_capacity(paths) };
    for [discount, fx, fd, ff] in results {
        out.discount.push(discount);
        out.fx.push(fx);
        out.domestic_factor.push(fd);
        out.foreign_factor.push(ff);
    }
    Ok(out)
}

/// The note's value on the three-factor paths, with its standard error.
pub fn compute_prdc_three_factor_monte_carlo(
    terms: &PrdcTerms,
    model: &PrdcThreeFactor,
    paths: usize,
    steps_per_year: usize,
    seed: u64,
) -> Result<(f64, f64), Error> {
    terms.check()?;
    let states = generate_prdc_three_factor_paths(&terms.coupon_times, model, paths, steps_per_year, seed)?;
    let scale = terms.leverage / terms.fx_base;
    let n = terms.coupon_times.len();
    let values: Vec<f64> = states
        .discount
        .iter()
        .zip(&states.fx)
        .map(|(discount, fx)| {
            let mut total = 0.0;
            for i in 0..n {
                let rate = terms.fixed_rates[i].unwrap_or_else(|| (fx[i] * scale - terms.subtraction).max(terms.floor).min(terms.cap));
                total += rate * discount[i] * terms.notional * terms.accrual_fractions[i];
            }
            total + discount[n - 1] * terms.redemption
        })
        .collect();
    let count = values.len() as f64;
    let mean = values.iter().sum::<f64>() / count;
    let variance = values.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / (count - 1.0);
    Ok((mean, (variance / count).sqrt()))
}

/// A callable note in the three-factor model, on evaluation paths with the issuer's call rule from
/// training paths.
#[derive(Debug, Clone, PartialEq)]
pub struct CallablePrdcResult {
    pub value: f64,
    pub standard_error: f64,
    /// The note without the call, on the same evaluation paths.
    pub noncallable_value: f64,
    /// Each path called where its value is lowest in hindsight (or never): no call rule gives less on
    /// the same paths. It bounds the estimate, not the true value.
    pub hindsight_value: f64,
    /// The share of evaluation paths called on each call date, in the order of `call_dates`.
    pub call_probability: Vec<f64>,
}

/// The regressors of the issuer's continuation value at a coupon date: 1, `S / fx_base`, its square,
/// `x_d` and `x_f`.
fn call_basis(fx: f64, fx_base: f64, domestic: f64, foreign: f64) -> [f64; 5] {
    let s = fx / fx_base;
    [1.0, s, s * s, domestic, foreign]
}

/// The issuer's call rule: the regression coefficients at each coupon date that allows a call.
struct CallRule {
    /// For each coupon date, its position in the call dates, if it allows a call.
    callable: Vec<Option<usize>>,
    coefficients: Vec<Option<[f64; 5]>>,
}

fn coupon_amount(terms: &PrdcTerms, i: usize, fx: f64) -> f64 {
    let scale = terms.leverage / terms.fx_base;
    let rate = terms.fixed_rates[i].unwrap_or_else(|| (fx * scale - terms.subtraction).max(terms.floor).min(terms.cap));
    rate * terms.notional * terms.accrual_fractions[i]
}

/// `d coupon / d S` (0 at a fixed rate, below the floor or above the cap).
fn coupon_slope(terms: &PrdcTerms, i: usize, fx: f64) -> f64 {
    let scale = terms.leverage / terms.fx_base;
    let raw = fx * scale - terms.subtraction;
    if terms.fixed_rates[i].is_some() || raw <= terms.floor || raw >= terms.cap {
        0.0
    } else {
        scale * terms.notional * terms.accrual_fractions[i]
    }
}

fn check_call_terms(terms: &PrdcTerms, call_dates: &[usize], call_price: f64) -> Result<Vec<Option<usize>>, Error> {
    terms.check()?;
    let n = terms.coupon_times.len();
    if call_dates.windows(2).any(|w| w[1] <= w[0]) || call_dates.last().is_some_and(|&c| c + 1 >= n) {
        return invalid(format!("call dates must be increasing positions before the last coupon (of {n})"));
    }
    if !call_price.is_finite() || call_price < 0.0 {
        return invalid(format!("the call price must be finite and non-negative, not {call_price}"));
    }
    let mut callable = vec![None; n];
    for (k, &c) in call_dates.iter().enumerate() {
        callable[c] = Some(k);
    }
    Ok(callable)
}

/// The rule by rolling back on the training paths (see [`compute_callable_prdc_three_factor`]).
// the paths are indexed alongside the vectors of their values
#[allow(clippy::needless_range_loop)]
fn estimate_call_rule(
    terms: &PrdcTerms,
    callable: Vec<Option<usize>>,
    call_price: f64,
    train: &PrdcThreeFactorPaths,
) -> Result<CallRule, Error> {
    let n = terms.coupon_times.len();
    let paths = train.discount.len();
    let mut after: Vec<f64> = (0..paths).map(|p| train.discount[p][n - 1] * terms.redemption).collect();
    let mut coefficients: Vec<Option<[f64; 5]>> = vec![None; n];
    for i in (0..n).rev() {
        if callable[i].is_some() {
            let mut design = Vec::with_capacity(paths * 5);
            let mut target = Vec::with_capacity(paths);
            for p in 0..paths {
                design.extend_from_slice(&call_basis(train.fx[p][i], terms.fx_base, train.domestic_factor[p][i], train.foreign_factor[p][i]));
                target.push(after[p] / train.discount[p][i]);
            }
            let c = crate::math::linear::solve_least_squares(&design, 5, &target)?;
            let rule = [c[0], c[1], c[2], c[3], c[4]];
            for p in 0..paths {
                let basis = call_basis(train.fx[p][i], terms.fx_base, train.domestic_factor[p][i], train.foreign_factor[p][i]);
                let continuation: f64 = rule.iter().zip(&basis).map(|(a, b)| a * b).sum();
                if continuation > call_price {
                    after[p] = call_price * train.discount[p][i];
                }
            }
            coefficients[i] = Some(rule);
        }
        for p in 0..paths {
            after[p] += coupon_amount(terms, i, train.fx[p][i]) * train.discount[p][i];
        }
    }
    Ok(CallRule { callable, coefficients })
}

/// One evaluation path under the rule.
struct FollowedPath {
    value: f64,
    /// The last coupon date with a cash flow: the call date, or the last date.
    stop: usize,
    /// Whether the note was called at `stop`.
    called: bool,
    noncallable: f64,
    hindsight: f64,
}

fn follow_call_rule(terms: &PrdcTerms, rule: &CallRule, call_price: f64, eval: &PrdcThreeFactorPaths, p: usize) -> FollowedPath {
    let n = terms.coupon_times.len();
    let (d, s) = (&eval.discount[p], &eval.fx[p]);
    let mut paid = 0.0;
    let mut stop = None;
    let mut lowest = f64::INFINITY;
    let mut value = 0.0;
    for i in 0..n {
        paid += coupon_amount(terms, i, s[i]) * d[i];
        if let (Some(_), Some(c)) = (rule.callable[i], rule.coefficients[i]) {
            let if_called = paid + call_price * d[i];
            lowest = lowest.min(if_called);
            if stop.is_none() {
                let basis = call_basis(s[i], terms.fx_base, eval.domestic_factor[p][i], eval.foreign_factor[p][i]);
                let continuation: f64 = c.iter().zip(&basis).map(|(a, b)| a * b).sum();
                if continuation > call_price {
                    stop = Some(i);
                    value = if_called;
                }
            }
        }
    }
    let full = paid + terms.redemption * d[n - 1];
    FollowedPath {
        value: if stop.is_some() { value } else { full },
        stop: stop.unwrap_or(n - 1),
        called: stop.is_some(),
        noncallable: full,
        hindsight: lowest.min(full),
    }
}

fn mean_and_error(values: &[f64]) -> PrdcEstimate {
    let count = values.len() as f64;
    let mean = values.iter().sum::<f64>() / count;
    let variance = values.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / (count - 1.0);
    PrdcEstimate { value: mean, standard_error: (variance / count).sqrt() }
}

/// A Monte Carlo estimate and its standard error.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrdcEstimate {
    pub value: f64,
    pub standard_error: f64,
}

/// A PRDC note that the issuer may redeem at `call_price` (an amount, like `redemption`) on the coupon
/// dates `call_dates` (positions in `coupon_times`, increasing, before the last), right after paying
/// that date's coupon. The issuer calls when the value of the rest of the note exceeds the call price.
/// Longstaff and Schwartz: rolling back on `training_paths` paths from `training_seed`, the value of the rest, in money
/// at the date, is regressed on 1, `S / fx_base`, its square, `x_d` and `x_f` over all paths; the
/// value is then the mean over paths from `seed` of the cash flows that follow the rule, discounted by
/// `exp(-int r_d)`. A suboptimal rule overstates the note, so the estimate leans upward (the issuer's
/// option is undervalued), apart from the noise.
#[allow(clippy::too_many_arguments)]
pub fn compute_callable_prdc_three_factor(
    terms: &PrdcTerms,
    model: &PrdcThreeFactor,
    call_dates: &[usize],
    call_price: f64,
    paths: usize,
    training_paths: usize,
    steps_per_year: usize,
    seed: u64,
    training_seed: u64,
) -> Result<CallablePrdcResult, Error> {
    let callable = check_call_terms(terms, call_dates, call_price)?;
    let train = generate_prdc_three_factor_paths(&terms.coupon_times, model, training_paths, steps_per_year, training_seed)?;
    let rule = estimate_call_rule(terms, callable, call_price, &train)?;
    let eval = generate_prdc_three_factor_paths(&terms.coupon_times, model, paths, steps_per_year, seed)?;
    let followed: Vec<FollowedPath> = (0..paths).map(|p| follow_call_rule(terms, &rule, call_price, &eval, p)).collect();
    let mut called = vec![0usize; call_dates.len()];
    for f in followed.iter().filter(|f| f.called) {
        called[rule.callable[f.stop].expect("a call date")] += 1;
    }
    let count = paths as f64;
    let e = mean_and_error(&followed.iter().map(|f| f.value).collect::<Vec<_>>());
    Ok(CallablePrdcResult {
        value: e.value,
        standard_error: e.standard_error,
        noncallable_value: followed.iter().map(|f| f.noncallable).sum::<f64>() / count,
        hindsight_value: followed.iter().map(|f| f.hindsight).sum::<f64>() / count,
        call_probability: called.iter().map(|&c| c as f64 / count).collect(),
    })
}

/// The pieces of `term` shifted up by `h` and down by `h` or less (to no lower than half the lowest
/// piece, as for a constant volatility), and the distance between the two.
fn shift_fx_volatility_term(term: FxVolatilityTerm, h: f64) -> (Vec<f64>, Vec<f64>, f64) {
    let lowest = term.volatilities.iter().copied().fold(f64::INFINITY, f64::min);
    let down = lowest - (lowest - h).max(lowest.min(h) * 0.5);
    (term.volatilities.iter().map(|v| v + h).collect(), term.volatilities.iter().map(|v| v - down).collect(), h + down)
}

/// The model with the pieces of `term` set to `volatilities`.
fn with_fx_volatilities<'a>(model: &PrdcThreeFactor<'a>, term: FxVolatilityTerm<'a>, volatilities: &'a [f64]) -> PrdcThreeFactor<'a> {
    PrdcThreeFactor { fx_volatility_term: Some(FxVolatilityTerm { times: term.times, volatilities }), ..*model }
}

/// A curve input's derivative of a callable note.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CallableCurveSensitivity {
    pub domestic: bool,
    pub input: CurveInput,
    pub d_npv: PrdcEstimate,
}

/// The first derivatives of a callable note, each with its standard error.
#[derive(Debug, Clone, PartialEq)]
pub struct CallablePrdcSensitivity {
    pub value: PrdcEstimate,
    pub fx_delta: PrdcEstimate,
    pub fx_vega: PrdcEstimate,
    pub domestic_mean_reversion: PrdcEstimate,
    pub domestic_volatility: PrdcEstimate,
    pub foreign_mean_reversion: PrdcEstimate,
    pub foreign_volatility: PrdcEstimate,
    pub correlation: [PrdcEstimate; 3],
    pub curves: Vec<CallableCurveSensitivity>,
}

/// The derivatives of the callable note of [`compute_callable_prdc_three_factor`], with the issuer's
/// rule from the training paths held fixed: the call is optimal, so a change of the rule changes the
/// value only to second order (the envelope theorem), up to the rule's error.
///
/// - The spot and the curves' inputs: pathwise. On the paths the discount factor to `T_j` is
///   proportional to `P_d(0, T_j)` and the FX rate to `S P_f(0, T_j) / P_d(0, T_j)`, so each path's cash
///   flows up to its stop are differentiated with respect to `ln S`, `ln P_d(0, T_j)` and
///   `ln P_f(0, T_j)` (the coupon's slope is 0 outside its floor and cap), and chained to the curves'
///   inputs by automatic differentiation of `ln P(0, T_j)`.
/// - The FX volatility (a parallel shift of a piecewise one), the rates' mean reversions and
///   volatilities and the correlations: central
///   differences of `bump` (absolute; `10 * bump` for the correlations) on the same evaluation paths
///   (common random numbers) with the same rule, one-sided where a step would leave the range.
///
/// Each standard error is over the paths' contributions.
#[allow(clippy::too_many_arguments)]
// the paths are indexed alongside the vectors of their values
#[allow(clippy::needless_range_loop)]
pub fn compute_callable_prdc_three_factor_sensitivity<'c>(
    terms: &PrdcTerms,
    model: &PrdcThreeFactor<'c>,
    call_dates: &[usize],
    call_price: f64,
    paths: usize,
    training_paths: usize,
    steps_per_year: usize,
    seed: u64,
    training_seed: u64,
    bump: f64,
) -> Result<CallablePrdcSensitivity, Error> {
    if !(bump > 0.0 && bump < 0.01) {
        return invalid(format!("the bump must be in (0, 0.01), not {bump}"));
    }
    let callable = check_call_terms(terms, call_dates, call_price)?;
    let train = generate_prdc_three_factor_paths(&terms.coupon_times, model, training_paths, steps_per_year, training_seed)?;
    let rule = estimate_call_rule(terms, callable, call_price, &train)?;
    let eval = generate_prdc_three_factor_paths(&terms.coupon_times, model, paths, steps_per_year, seed)?;
    let n = terms.coupon_times.len();
    let followed: Vec<FollowedPath> = (0..paths).map(|p| follow_call_rule(terms, &rule, call_price, &eval, p)).collect();
    let value = mean_and_error(&followed.iter().map(|f| f.value).collect::<Vec<_>>());

    // pathwise: d value / d ln S, d ln P_d(T_j), d ln P_f(T_j)
    let mut by_spot = Vec::with_capacity(paths);
    let mut by_domestic = vec![Vec::with_capacity(paths); n];
    let mut by_foreign = vec![Vec::with_capacity(paths); n];
    for (p, f) in followed.iter().enumerate() {
        let (d, s) = (&eval.discount[p], &eval.fx[p]);
        let mut spot = 0.0;
        for j in 0..n {
            let (dd, ff) = if j <= f.stop {
                let level = coupon_slope(terms, j, s[j]) * s[j] * d[j];
                spot += level;
                let paid_at_stop = if f.called { call_price } else { terms.redemption };
                let end = if j == f.stop { paid_at_stop * d[j] } else { 0.0 };
                (coupon_amount(terms, j, s[j]) * d[j] - level + end, level)
            } else {
                (0.0, 0.0)
            };
            by_domestic[j].push(dd);
            by_foreign[j].push(ff);
        }
        by_spot.push(spot / model.spot);
    }
    let fx_delta = mean_and_error(&by_spot);
    // the Jacobian of ln P(0, T_j) with respect to each curve's inputs
    let mut curves = Vec::new();
    for (domestic, curve, by_date) in [(true, model.domestic, &by_domestic), (false, model.foreign, &by_foreign)] {
        let recorder = Recorder::new();
        let lifted = curve.lift(&recorder)?;
        let wrt: Vec<_> = lifted.inputs.iter().map(|(_, x)| *x).collect();
        let mut jacobian = Vec::with_capacity(n);
        for &t in &terms.coupon_times {
            let log_discount = lifted.curve.discount_at(t, true)?.ln();
            jacobian.push(recorder.gradient(log_discount, &wrt)?.iter().map(|g| g.mean()).collect::<Vec<f64>>());
        }
        for (q, (input, _)) in lifted.inputs.iter().enumerate() {
            let per_path: Vec<f64> = (0..paths).map(|p| (0..n).map(|j| by_date[j][p] * jacobian[j][q]).sum()).collect();
            curves.push(CallableCurveSensitivity { domestic, input: *input, d_npv: mean_and_error(&per_path) });
        }
    }

    // central differences on the same paths, with the same rule; one-sided where a step would leave the
    // parameter's range (a volatility below 0, a correlation beyond 1)
    let m = *model;
    let differences = |set: &dyn Fn(f64) -> PrdcThreeFactor<'c>, x: f64, h: f64, low: f64, high: f64| -> Result<PrdcEstimate, Error> {
        let (up, down) = ((x + h).min(high), (x - h).max(low));
        let (a, b) = (
            generate_prdc_three_factor_paths(&terms.coupon_times, &set(up), paths, steps_per_year, seed)?,
            generate_prdc_three_factor_paths(&terms.coupon_times, &set(down), paths, steps_per_year, seed)?,
        );
        let per_path: Vec<f64> = (0..paths)
            .map(|p| (follow_call_rule(terms, &rule, call_price, &a, p).value - follow_call_rule(terms, &rule, call_price, &b, p).value) / (up - down))
            .collect();
        Ok(mean_and_error(&per_path))
    };
    let h = bump;
    let fx_vega = match m.fx_volatility_term {
        None => differences(&|v| PrdcThreeFactor { fx_volatility: v, ..m }, m.fx_volatility, h, m.fx_volatility.min(h) * 0.5, f64::INFINITY)?,
        Some(term) => {
            // a parallel shift of the pieces
            let (up, down, step) = shift_fx_volatility_term(term, h);
            let (a, b) = (with_fx_volatilities(&m, term, &up), with_fx_volatilities(&m, term, &down));
            let (a, b) = (
                generate_prdc_three_factor_paths(&terms.coupon_times, &a, paths, steps_per_year, seed)?,
                generate_prdc_three_factor_paths(&terms.coupon_times, &b, paths, steps_per_year, seed)?,
            );
            let per_path: Vec<f64> = (0..paths)
                .map(|p| (follow_call_rule(terms, &rule, call_price, &a, p).value - follow_call_rule(terms, &rule, call_price, &b, p).value) / step)
                .collect();
            mean_and_error(&per_path)
        }
    };
    let rate = |f: HullWhiteFactor, a: Option<f64>, s: Option<f64>| HullWhiteFactor {
        mean_reversion: a.unwrap_or(f.mean_reversion),
        volatility: s.unwrap_or(f.volatility),
    };
    let (dr, fr) = (m.domestic_rate, m.foreign_rate);
    let domestic_mean_reversion = differences(&|a| PrdcThreeFactor { domestic_rate: rate(dr, Some(a), None), ..m }, dr.mean_reversion, h, 0.0, f64::INFINITY)?;
    let domestic_volatility = differences(&|s| PrdcThreeFactor { domestic_rate: rate(dr, None, Some(s)), ..m }, dr.volatility, h, dr.volatility.min(h) * 0.5, f64::INFINITY)?;
    let foreign_mean_reversion = differences(&|a| PrdcThreeFactor { foreign_rate: rate(fr, Some(a), None), ..m }, fr.mean_reversion, h, 0.0, f64::INFINITY)?;
    let foreign_volatility = differences(&|s| PrdcThreeFactor { foreign_rate: rate(fr, None, Some(s)), ..m }, fr.volatility, h, fr.volatility.min(h) * 0.5, f64::INFINITY)?;
    let mut correlation = [PrdcEstimate { value: 0.0, standard_error: 0.0 }; 3];
    for (k, c) in correlation.iter_mut().enumerate() {
        let set = |r: f64| {
            let mut rho = m.correlation;
            rho[k] = r;
            PrdcThreeFactor { correlation: rho, ..m }
        };
        *c = differences(&set, m.correlation[k], 10.0 * h, -1.0, 1.0)?;
    }
    Ok(CallablePrdcSensitivity {
        value,
        fx_delta,
        fx_vega,
        domestic_mean_reversion,
        domestic_volatility,
        foreign_mean_reversion,
        foreign_volatility,
        correlation,
        curves,
    })
}

// ---- target redemption and knock-out ----

/// What the coupon of the date that reaches the target pays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetCoupon {
    /// The whole coupon, even beyond the target.
    Full,
    /// Only what is left up to the target (an exact target).
    Capped,
}

/// The target redemption of a note: once the sum of the coupon rates times their accrual fractions
/// (fixed coupons included) reaches `target`, the note pays that date's coupon (see [`TargetCoupon`])
/// and the redemption, and ends. With `make_whole`, a note that has not reached the target by the last
/// date also pays the shortfall then.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetRedemption {
    /// In units of the notional, as the sum of `rate * accrual_fraction`. Infinity for no target.
    pub target: f64,
    pub coupon: TargetCoupon,
    pub make_whole: bool,
}

/// An FX knock-out: on each coupon date of `dates` (positions in `coupon_times`, increasing, before the
/// last) where the FX rate is at or above `level`, the note pays that date's coupon and the redemption,
/// and ends. The FX rate is observed on those dates only.
#[derive(Debug, Clone, PartialEq)]
pub struct KnockOut {
    /// Domestic per foreign.
    pub level: f64,
    pub dates: Vec<usize>,
}

/// A note with a target redemption, a knock-out, or both, on the three-factor paths.
#[derive(Debug, Clone, PartialEq)]
pub struct PathDependentPrdcResult {
    pub value: f64,
    pub standard_error: f64,
    /// The note without either on the same paths.
    pub plain_value: f64,
    /// The share of paths that reach the target on each coupon date.
    pub target_probability: Vec<f64>,
    /// The share of paths knocked out on each coupon date (a date that also reaches the target counts
    /// as the target's).
    pub knock_out_probability: Vec<f64>,
    /// The mean time to the note's end, in years.
    pub expected_life: f64,
}

/// A target redemption note on the three-factor paths.
#[derive(Debug, Clone, PartialEq)]
pub struct TargetRedemptionResult {
    pub value: f64,
    pub standard_error: f64,
    /// The note without the target on the same paths.
    pub plain_value: f64,
    /// The share of paths that reach the target on each coupon date.
    pub redemption_probability: Vec<f64>,
    /// The mean time to the note's end, in years.
    pub expected_life: f64,
}

/// Why a path ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ending {
    Maturity,
    Target,
    KnockOut,
}

/// One path: its discounted cash flows, the date it ends and why. On a date the target is checked
/// first, then the knock-out (after the coupon).
fn follow_path_dependent(
    terms: &PrdcTerms,
    target: Option<&TargetRedemption>,
    knock_out: Option<(&[bool], f64)>,
    discount: &[f64],
    fx: &[f64],
) -> (f64, usize, Ending) {
    let n = terms.coupon_times.len();
    let scale = terms.leverage / terms.fx_base;
    let mut sum = 0.0;
    let mut value = 0.0;
    for i in 0..n {
        let rate = terms.fixed_rates[i].unwrap_or_else(|| (fx[i] * scale - terms.subtraction).max(terms.floor).min(terms.cap));
        let accrued = rate * terms.accrual_fractions[i];
        if let Some(t) = target {
            if sum + accrued >= t.target {
                let paid = match t.coupon {
                    TargetCoupon::Full => accrued,
                    TargetCoupon::Capped => t.target - sum,
                };
                value += (paid * terms.notional + terms.redemption) * discount[i];
                return (value, i, Ending::Target);
            }
        }
        sum += accrued;
        value += accrued * terms.notional * discount[i];
        if let Some((observed, level)) = knock_out {
            if observed[i] && fx[i] >= level {
                value += terms.redemption * discount[i];
                return (value, i, Ending::KnockOut);
            }
        }
    }
    let shortfall = match target {
        Some(t) if t.make_whole => (t.target - sum).max(0.0) * terms.notional,
        _ => 0.0,
    };
    value += (terms.redemption + shortfall) * discount[n - 1];
    (value, n - 1, Ending::Maturity)
}

/// A note with a target redemption (see [`TargetRedemption`]), an FX knock-out (see [`KnockOut`]), or
/// both, on the paths of the three-factor model, by Monte Carlo: the note is path-dependent, but needs no
/// regression. With zero rate volatilities it is the note of the first model.
pub fn compute_path_dependent_prdc(
    terms: &PrdcTerms,
    model: &PrdcThreeFactor,
    target: Option<&TargetRedemption>,
    knock_out: Option<&KnockOut>,
    paths: usize,
    steps_per_year: usize,
    seed: u64,
) -> Result<PathDependentPrdcResult, Error> {
    let observed = check_path_dependent(terms, target, knock_out)?;
    let n = terms.coupon_times.len();
    let states = generate_prdc_three_factor_paths(&terms.coupon_times, model, paths, steps_per_year, seed)?;
    let knock = knock_out.map(|k| (observed.as_slice(), k.level));
    let mut values = Vec::with_capacity(paths);
    let mut plain = 0.0;
    let (mut targets, mut knocks) = (vec![0usize; n], vec![0usize; n]);
    let mut life = 0.0;
    for (discount, fx) in states.discount.iter().zip(&states.fx) {
        let (value, end, why) = follow_path_dependent(terms, target, knock, discount, fx);
        values.push(value);
        match why {
            Ending::Target => targets[end] += 1,
            Ending::KnockOut => knocks[end] += 1,
            Ending::Maturity => {}
        }
        life += terms.coupon_times[end];
        plain += follow_path_dependent(terms, None, None, discount, fx).0;
    }
    let e = mean_and_error(&values);
    let count = paths as f64;
    Ok(PathDependentPrdcResult {
        value: e.value,
        standard_error: e.standard_error,
        plain_value: plain / count,
        target_probability: targets.iter().map(|&c| c as f64 / count).collect(),
        knock_out_probability: knocks.iter().map(|&c| c as f64 / count).collect(),
        expected_life: life / count,
    })
}

/// The terms of a target and a knock-out checked, and the coupon dates where the knock-out is observed.
fn check_path_dependent(terms: &PrdcTerms, target: Option<&TargetRedemption>, knock_out: Option<&KnockOut>) -> Result<Vec<bool>, Error> {
    terms.check()?;
    let n = terms.coupon_times.len();
    if let Some(t) = target {
        if !(t.target > 0.0) {
            return invalid(format!("the target must be positive (infinity for none), not {}", t.target));
        }
        if t.make_whole && !t.target.is_finite() {
            return invalid("a make-whole needs a finite target".into());
        }
    }
    let mut observed = vec![false; n];
    if let Some(k) = knock_out {
        if !(k.level > 0.0) {
            return invalid(format!("the knock-out level must be positive (infinity for none), not {}", k.level));
        }
        if k.dates.windows(2).any(|w| w[1] <= w[0]) || k.dates.last().is_some_and(|&c| c + 1 >= n) {
            return invalid(format!("knock-out dates must be increasing positions before the last coupon (of {n})"));
        }
        for &c in &k.dates {
            observed[c] = true;
        }
    }
    Ok(observed)
}

/// A derivative by central differences on common random numbers, with the share of paths whose
/// ending (its date or its reason) differs between the two sides: each such path adds a jump of its
/// cash flows divided by the step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathDependentDifference {
    pub value: f64,
    pub standard_error: f64,
    pub changed_paths: f64,
}

/// A curve input's derivative of a path-dependent note.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathDependentCurveSensitivity {
    pub domestic: bool,
    pub input: CurveInput,
    pub d_npv: PathDependentDifference,
}

/// The first derivatives of a note with a target redemption or a knock-out.
#[derive(Debug, Clone, PartialEq)]
pub struct PathDependentPrdcSensitivity {
    pub value: PrdcEstimate,
    pub fx_delta: PathDependentDifference,
    /// With respect to a parallel shift of the FX volatility.
    pub fx_vega: PathDependentDifference,
    pub domestic_mean_reversion: PathDependentDifference,
    pub domestic_volatility: PathDependentDifference,
    pub foreign_mean_reversion: PathDependentDifference,
    pub foreign_volatility: PathDependentDifference,
    pub correlation: [PathDependentDifference; 3],
    pub curves: Vec<PathDependentCurveSensitivity>,
}

/// One path's value and ending.
type PathEnding = (f64, usize, Ending);

/// The derivatives of the note of [`compute_path_dependent_prdc`] by central differences on common
/// random numbers: each input is moved up and down, the paths are generated again from the same `seed`
/// (the same random numbers), and the difference of each path's value is divided by the step. A path whose
/// ending changes between the two sides (a target reached or a knock-out on another date) contributes
/// a jump divided by the step, so a small `bump` makes the estimate noisy and a large one biased; the
/// result records the share of such paths.
///
/// - The spot: relative steps `S (1 +- bump)`.
/// - The FX volatility (a parallel shift of a piecewise one), the rates' mean reversions and
///   volatilities: absolute steps of `bump`; the correlations: `10 * bump`. One-sided where a step
///   would leave the range. A volatility steps down to no less than half its value: at 0 a rate
///   leaves the paths' Cholesky factor and the random numbers fall on other factors, so the two sides
///   would no longer share them.
/// - The curves: on the paths, the discount factor to `T_j` is proportional to `P_d(0, T_j)` and the
///   FX rate to `S P_f(0, T_j) / P_d(0, T_j)`, so steps of `bump` in each `ln P_d(0, T_j)` and
///   `ln P_f(0, T_j)` move the same paths exactly; the differences are chained to the curves' inputs by
///   automatic differentiation of `ln P(0, T_j)`.
#[allow(clippy::too_many_arguments)]
// the paths are indexed alongside the vectors of their values
#[allow(clippy::needless_range_loop)]
pub fn compute_path_dependent_prdc_sensitivity<'c>(
    terms: &PrdcTerms,
    model: &PrdcThreeFactor<'c>,
    target: Option<&TargetRedemption>,
    knock_out: Option<&KnockOut>,
    paths: usize,
    steps_per_year: usize,
    seed: u64,
    bump: f64,
) -> Result<PathDependentPrdcSensitivity, Error> {
    if !(bump > 0.0 && bump < 0.1) {
        return invalid(format!("the bump must be in (0, 0.1), not {bump}"));
    }
    let observed = check_path_dependent(terms, target, knock_out)?;
    let knock = knock_out.map(|k| (observed.as_slice(), k.level));
    let n = terms.coupon_times.len();
    let follow = |discount: &[f64], fx: &[f64]| follow_path_dependent(terms, target, knock, discount, fx);
    let follow_all = |states: &PrdcThreeFactorPaths| -> Vec<PathEnding> {
        states.discount.iter().zip(&states.fx).map(|(discount, fx)| follow(discount, fx)).collect()
    };
    let compare = |up: &[PathEnding], down: &[PathEnding], step: f64| -> PathDependentDifference {
        let per_path: Vec<f64> = up.iter().zip(down).map(|(a, b)| (a.0 - b.0) / step).collect();
        let changed = up.iter().zip(down).filter(|(a, b)| (a.1, a.2) != (b.1, b.2)).count();
        let e = mean_and_error(&per_path);
        PathDependentDifference { value: e.value, standard_error: e.standard_error, changed_paths: changed as f64 / paths as f64 }
    };
    let generate = |m: &PrdcThreeFactor| generate_prdc_three_factor_paths(&terms.coupon_times, m, paths, steps_per_year, seed);
    let base = generate(model)?;
    let value = mean_and_error(&follow_all(&base).iter().map(|e| e.0).collect::<Vec<_>>());

    // the spot and the curves move the paths exactly
    let moved = |scale_discount: &dyn Fn(usize) -> f64, scale_fx: &dyn Fn(usize) -> f64| -> Vec<PathEnding> {
        let discount_scale: Vec<f64> = (0..n).map(scale_discount).collect();
        let fx_scale: Vec<f64> = (0..n).map(scale_fx).collect();
        base.discount
            .iter()
            .zip(&base.fx)
            .map(|(discount, fx)| {
                let d: Vec<f64> = discount.iter().zip(&discount_scale).map(|(x, s)| x * s).collect();
                let s: Vec<f64> = fx.iter().zip(&fx_scale).map(|(x, s)| x * s).collect();
                follow(&d, &s)
            })
            .collect()
    };
    let h = bump;
    let fx_delta = compare(&moved(&|_| 1.0, &|_| 1.0 + h), &moved(&|_| 1.0, &|_| 1.0 - h), 2.0 * h * model.spot);
    // per date: (domestic, foreign) of each path's difference and whether its ending changed
    let mut by_date: Vec<[(Vec<f64>, Vec<bool>); 2]> = Vec::with_capacity(n);
    for j in 0..n {
        let at = |x: f64| move |i: usize| if i == j { x } else { 1.0 };
        let (up, down) = ((h).exp(), (-h).exp());
        let domestic = (moved(&at(up), &at(down)), moved(&at(down), &at(up)));
        let foreign = (moved(&|_| 1.0, &at(up)), moved(&|_| 1.0, &at(down)));
        let split = |(a, b): (Vec<PathEnding>, Vec<PathEnding>)| -> (Vec<f64>, Vec<bool>) {
            a.iter().zip(&b).map(|(x, y)| ((x.0 - y.0) / (2.0 * h), (x.1, x.2) != (y.1, y.2))).unzip()
        };
        by_date.push([split(domestic), split(foreign)]);
    }
    let mut curves = Vec::new();
    for (side, (domestic, curve)) in [(true, model.domestic), (false, model.foreign)].into_iter().enumerate() {
        let recorder = Recorder::new();
        let lifted = curve.lift(&recorder)?;
        let wrt: Vec<_> = lifted.inputs.iter().map(|(_, x)| *x).collect();
        let mut jacobian = Vec::with_capacity(n);
        for &t in &terms.coupon_times {
            let log_discount = lifted.curve.discount_at(t, true)?.ln();
            jacobian.push(recorder.gradient(log_discount, &wrt)?.iter().map(|g| g.mean()).collect::<Vec<f64>>());
        }
        for (q, (input, _)) in lifted.inputs.iter().enumerate() {
            let per_path: Vec<f64> = (0..paths).map(|p| (0..n).map(|j| by_date[j][side].0[p] * jacobian[j][q]).sum()).collect();
            let changed = (0..paths).filter(|&p| (0..n).any(|j| jacobian[j][q] != 0.0 && by_date[j][side].1[p])).count();
            let e = mean_and_error(&per_path);
            curves.push(PathDependentCurveSensitivity {
                domestic,
                input: *input,
                d_npv: PathDependentDifference { value: e.value, standard_error: e.standard_error, changed_paths: changed as f64 / paths as f64 },
            });
        }
    }

    // the other inputs: the paths again from the same seed
    let m = *model;
    let differences = |set: &dyn Fn(f64) -> PrdcThreeFactor<'c>, x: f64, h: f64, low: f64, high: f64| -> Result<PathDependentDifference, Error> {
        let (up, down) = ((x + h).min(high), (x - h).max(low));
        Ok(compare(&follow_all(&generate(&set(up))?), &follow_all(&generate(&set(down))?), up - down))
    };
    let fx_vega = match m.fx_volatility_term {
        None => differences(&|v| PrdcThreeFactor { fx_volatility: v, ..m }, m.fx_volatility, h, m.fx_volatility.min(h) * 0.5, f64::INFINITY)?,
        Some(term) => {
            let (up, down, step) = shift_fx_volatility_term(term, h);
            let (a, b) = (with_fx_volatilities(&m, term, &up), with_fx_volatilities(&m, term, &down));
            compare(&follow_all(&generate(&a)?), &follow_all(&generate(&b)?), step)
        }
    };
    let rate = |f: HullWhiteFactor, a: Option<f64>, s: Option<f64>| HullWhiteFactor {
        mean_reversion: a.unwrap_or(f.mean_reversion),
        volatility: s.unwrap_or(f.volatility),
    };
    let (dr, fr) = (m.domestic_rate, m.foreign_rate);
    let domestic_mean_reversion = differences(&|a| PrdcThreeFactor { domestic_rate: rate(dr, Some(a), None), ..m }, dr.mean_reversion, h, 0.0, f64::INFINITY)?;
    let domestic_volatility = differences(&|s| PrdcThreeFactor { domestic_rate: rate(dr, None, Some(s)), ..m }, dr.volatility, h, dr.volatility.min(h) * 0.5, f64::INFINITY)?;
    let foreign_mean_reversion = differences(&|a| PrdcThreeFactor { foreign_rate: rate(fr, Some(a), None), ..m }, fr.mean_reversion, h, 0.0, f64::INFINITY)?;
    let foreign_volatility = differences(&|s| PrdcThreeFactor { foreign_rate: rate(fr, None, Some(s)), ..m }, fr.volatility, h, fr.volatility.min(h) * 0.5, f64::INFINITY)?;
    let zero = PathDependentDifference { value: 0.0, standard_error: 0.0, changed_paths: 0.0 };
    let mut correlation = [zero; 3];
    for (k, c) in correlation.iter_mut().enumerate() {
        let set = |r: f64| {
            let mut rho = m.correlation;
            rho[k] = r;
            PrdcThreeFactor { correlation: rho, ..m }
        };
        *c = differences(&set, m.correlation[k], (10.0 * h).min(0.5), -1.0, 1.0)?;
    }
    Ok(PathDependentPrdcSensitivity {
        value,
        fx_delta,
        fx_vega,
        domestic_mean_reversion,
        domestic_volatility,
        foreign_mean_reversion,
        foreign_volatility,
        correlation,
        curves,
    })
}

/// A target redemption note (see [`TargetRedemption`]) on the paths of the three-factor model: the
/// note of [`compute_path_dependent_prdc`] with the target only.
pub fn compute_target_redemption_prdc(
    terms: &PrdcTerms,
    model: &PrdcThreeFactor,
    target: &TargetRedemption,
    paths: usize,
    steps_per_year: usize,
    seed: u64,
) -> Result<TargetRedemptionResult, Error> {
    let r = compute_path_dependent_prdc(terms, model, Some(target), None, paths, steps_per_year, seed)?;
    Ok(TargetRedemptionResult {
        value: r.value,
        standard_error: r.standard_error,
        plain_value: r.plain_value,
        redemption_probability: r.target_probability,
        expected_life: r.expected_life,
    })
}

// ---- calibration of the three-factor model ----

/// The fit of the FX volatility of the three-factor model to FX option volatilities.
#[derive(Debug, Clone, PartialEq)]
pub struct FxVolatilityFit {
    pub fx_volatility: f64,
    /// `sqrt(V(T) / T)` of the fitted model at each quote's maturity.
    pub model_volatilities: Vec<f64>,
    pub root_mean_square_error: f64,
}

/// Fits the constant FX volatility `sigma_X` of the three-factor model, with its rates and correlations
/// fixed, to the Black volatilities of FX options (`volatilities` at `maturities`, in years). Under the
/// domestic forward measure of `T` the FX forward is lognormal with the variance `V(T)` of
/// [`PrdcThreeFactor::fx_forward_variance`], so a European FX option is Black's formula with
/// `sqrt(V(T) / T)`. `V` is a quadratic in `sigma_X` (`sigma_X^2 T + 2 sigma_X L + C`, with `L` and `C`
/// from the rates): one quote is solved exactly, and several by least squares on the volatilities (a
/// golden section on `(0, 3]`). A quote below what the rates alone give is an error. `model.fx_volatility`
/// is not used.
pub fn calibrate_prdc_fx_volatility(model: &PrdcThreeFactor, maturities: &[f64], volatilities: &[f64]) -> Result<FxVolatilityFit, Error> {
    if maturities.is_empty() || maturities.len() != volatilities.len() {
        return invalid("give one volatility per maturity".into());
    }
    if maturities.iter().any(|t| !(*t > 0.0) || !t.is_finite()) || volatilities.iter().any(|v| !(*v > 0.0) || !v.is_finite()) {
        return invalid("the maturities and volatilities must be positive and finite".into());
    }
    let at = |sx: f64| PrdcThreeFactor { fx_volatility: sx, fx_volatility_term: None, ..*model };
    at(1.0).check()?;
    // the quadratic's coefficients at each maturity, exactly: V(0) = C and V(1) = T + 2L + C
    let terms: Vec<(f64, f64, f64)> = maturities
        .iter()
        .map(|&t| {
            let c = at(0.0).fx_forward_variance(t);
            let l = (at(1.0).fx_forward_variance(t) - t - c) / 2.0;
            (t, l, c)
        })
        .collect();
    let model_volatility = |sx: f64, (t, l, c): (f64, f64, f64)| ((sx * sx * t + 2.0 * sx * l + c).max(0.0) / t).sqrt();
    let fx_volatility = if terms.len() == 1 {
        let (t, l, c) = terms[0];
        let v = volatilities[0];
        let discriminant = l * l - t * (c - v * v * t);
        let root = (-l + discriminant.max(0.0).sqrt()) / t;
        if discriminant < 0.0 || !(root > 0.0) {
            return invalid(format!("the FX volatility {v} at {t} years is below what the rates alone give"));
        }
        root
    } else {
        let objective = |sx: f64| -> f64 { terms.iter().zip(volatilities).map(|(&q, v)| (model_volatility(sx, q) - v).powi(2)).sum() };
        let ratio = (5f64.sqrt() - 1.0) / 2.0;
        let (mut lo, mut hi) = (1e-8, 3.0);
        let mut x1 = hi - ratio * (hi - lo);
        let mut x2 = lo + ratio * (hi - lo);
        let (mut f1, mut f2) = (objective(x1), objective(x2));
        for _ in 0..200 {
            if hi - lo < 1e-13 {
                break;
            }
            if f1 <= f2 {
                hi = x2;
                x2 = x1;
                f2 = f1;
                x1 = hi - ratio * (hi - lo);
                f1 = objective(x1);
            } else {
                lo = x1;
                x1 = x2;
                f1 = f2;
                x2 = lo + ratio * (hi - lo);
                f2 = objective(x2);
            }
        }
        (lo + hi) / 2.0
    };
    let model_volatilities: Vec<f64> = terms.iter().map(|&q| model_volatility(fx_volatility, q)).collect();
    let root_mean_square_error =
        (model_volatilities.iter().zip(volatilities).map(|(m, v)| (m - v).powi(2)).sum::<f64>() / volatilities.len() as f64).sqrt();
    Ok(FxVolatilityFit { fx_volatility, model_volatilities, root_mean_square_error })
}

/// The fit of a piecewise-constant FX volatility to FX option volatilities by maturity.
#[derive(Debug, Clone, PartialEq)]
pub struct FxVolatilityTermFit {
    /// The ends of the pieces: the quotes' maturities.
    pub times: Vec<f64>,
    pub volatilities: Vec<f64>,
    /// `sqrt(V(T) / T)` of the fitted model at each maturity: the quotes, up to rounding.
    pub model_volatilities: Vec<f64>,
}

impl FxVolatilityTermFit {
    /// The fitted volatility for [`PrdcThreeFactor::fx_volatility_term`].
    pub fn term(&self) -> FxVolatilityTerm<'_> {
        FxVolatilityTerm { times: &self.times, volatilities: &self.volatilities }
    }
}

/// Fits a piecewise-constant FX volatility, one piece per quote ending at its maturity, so that the
/// three-factor model gives each ATM (Black) volatility `volatilities[i]` at `maturities[i]` (in years,
/// increasing) exactly, with the rates and correlations of `model` fixed. A quote gives the total
/// variance `V(T_i) = vol_i^2 T_i` of the FX forward, and the pieces are bootstrapped from the first:
/// with the earlier pieces fixed, `V(T_i)` is a quadratic in the piece on `(T_{i-1}, T_i]`, solved
/// exactly. With zero rate volatilities the piece is `sqrt((V(T_i) - V(T_{i-1})) / (T_i - T_{i-1}))`,
/// the increment of the total variance; with stochastic rates the rates' share of `V` and their
/// covariance with the FX rate are taken out. A total variance that the earlier pieces and the rates
/// already exceed (no positive piece) is an error. `model.fx_volatility` and `model.fx_volatility_term`
/// are not used.
pub fn calibrate_prdc_fx_volatility_term(model: &PrdcThreeFactor, maturities: &[f64], volatilities: &[f64]) -> Result<FxVolatilityTermFit, Error> {
    if maturities.is_empty() || maturities.len() != volatilities.len() {
        return invalid("give one volatility per maturity".into());
    }
    if maturities.iter().any(|t| !(*t > 0.0) || !t.is_finite()) || volatilities.iter().any(|v| !(*v > 0.0) || !v.is_finite()) {
        return invalid("the maturities and volatilities must be positive and finite".into());
    }
    if maturities.windows(2).any(|w| !(w[1] > w[0])) {
        return invalid("the maturities must be increasing".into());
    }
    let flat = PrdcThreeFactor { fx_volatility: 1.0, fx_volatility_term: None, ..*model };
    flat.check()?;
    let base = flat.parameters();
    let mut pieces: Vec<f64> = Vec::with_capacity(maturities.len());
    for (i, (&t, &quote)) in maturities.iter().zip(volatilities).enumerate() {
        // V(T_i) with the last piece at x: a x^2 + b x + c
        let variance_with = |x: f64| {
            let mut fx_volatility = pieces.clone();
            fx_volatility.push(x);
            let p = ThreeFactorParameters { fx_volatility, fx_ends: maturities[..=i].to_vec(), ..base.clone() };
            fx_forward_variance_of(&p, t)
        };
        let (c, up, down) = (variance_with(0.0), variance_with(1.0), variance_with(-1.0));
        let (a, b) = ((up + down) / 2.0 - c, (up - down) / 2.0);
        let discriminant = b * b - 4.0 * a * (c - quote * quote * t);
        let root = (-b + discriminant.max(0.0).sqrt()) / (2.0 * a);
        if discriminant < 0.0 || !(root > 0.0) || !root.is_finite() {
            return invalid(format!(
                "the FX volatility {quote} at {t} years gives a total variance that the earlier maturities and the rates already exceed"
            ));
        }
        pieces.push(root);
    }
    let fit = FxVolatilityTermFit { times: maturities.to_vec(), volatilities: pieces, model_volatilities: Vec::new() };
    let fitted = PrdcThreeFactor { fx_volatility_term: Some(fit.term()), ..flat };
    let model_volatilities = maturities.iter().map(|&t| (fitted.fx_forward_variance(t) / t).sqrt()).collect();
    Ok(FxVolatilityTermFit { model_volatilities, ..fit })
}

/// The calibration of the three-factor model.
#[derive(Debug, Clone, PartialEq)]
pub struct PrdcCalibration {
    pub domestic: crate::hull_white::HullWhiteCalibration,
    pub foreign: crate::hull_white::HullWhiteCalibration,
    pub fx: FxVolatilityFit,
}

/// Calibrates the three-factor model: each short rate's Hull-White mean reversion and volatility to its
/// own currency's option prices on its own curve ([`crate::hull_white::calibrate_hull_white`]), then the FX
/// volatility to FX option volatilities with those rates and the given correlations
/// ([`calibrate_prdc_fx_volatility`]). The correlations are inputs (historical estimates, for example),
/// not fitted.
#[allow(clippy::too_many_arguments)]
pub fn calibrate_prdc_three_factor(
    spot: f64,
    domestic: &YieldCurve,
    foreign: &YieldCurve,
    domestic_quotes: &[crate::hull_white::CalibrationQuote],
    foreign_quotes: &[crate::hull_white::CalibrationQuote],
    correlation: [f64; 3],
    fx_maturities: &[f64],
    fx_volatilities: &[f64],
    settings: &crate::hull_white::CalibrationSettings,
) -> Result<PrdcCalibration, Error> {
    let d = crate::hull_white::calibrate_hull_white(domestic_quotes, domestic, settings)?;
    let f = crate::hull_white::calibrate_hull_white(foreign_quotes, foreign, settings)?;
    let model = PrdcThreeFactor {
        spot,
        fx_volatility: 1.0,
        fx_volatility_term: None,
        domestic,
        foreign,
        domestic_rate: HullWhiteFactor { mean_reversion: d.mean_reversion, volatility: d.volatility },
        foreign_rate: HullWhiteFactor { mean_reversion: f.mean_reversion, volatility: f.volatility },
        correlation,
    };
    let fx = calibrate_prdc_fx_volatility(&model, fx_maturities, fx_volatilities)?;
    Ok(PrdcCalibration { domestic: d, foreign: f, fx })
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

    fn three_factor<'c>(d: &'c YieldCurve, f: &'c YieldCurve, rates: [f64; 2], correlation: [f64; 3]) -> PrdcThreeFactor<'c> {
        PrdcThreeFactor {
            spot: 150.0,
            fx_volatility: 0.11,
            fx_volatility_term: None,
            domestic: d,
            foreign: f,
            domestic_rate: HullWhiteFactor { mean_reversion: 0.03, volatility: rates[0] },
            foreign_rate: HullWhiteFactor { mean_reversion: 0.05, volatility: rates[1] },
            correlation,
        }
    }

    #[test]
    fn without_rate_volatility_the_three_factor_model_is_the_simple_one() {
        let (d, f) = (jpy(), foreign());
        let model = three_factor(&d, &f, [0.0, 0.0], [0.3, -0.2, 0.4]);
        for t in [0.5, 1.0, 5.0] {
            assert!((model.fx_forward_variance(t) - 0.11 * 0.11 * t).abs() < 1e-15);
        }
        let simple = compute_prdc_analytic_value(&terms(), &PrdcMarket { spot: 150.0, volatility: 0.11, domestic: &d, foreign: &f }).unwrap();
        let three = compute_prdc_three_factor_analytic(&terms(), &model).unwrap();
        assert!((three - simple).abs() < 1e-14 * simple.abs(), "{three} against {simple}");
    }

    #[test]
    fn the_three_factor_paths_keep_no_arbitrage_and_match_the_closed_form() {
        let (d, f) = (jpy(), foreign());
        // rate volatilities of 1% a year, correlated with each other and with the FX rate
        let model = three_factor(&d, &f, [0.01, 0.012], [0.3, -0.2, 0.4]);
        let times = terms().coupon_times;
        let paths = generate_prdc_three_factor_paths(&times, &model, 20_000, 12, 5).unwrap();
        let mean_se = |v: Vec<f64>| -> (f64, f64) {
            let n = v.len() as f64;
            let m = v.iter().sum::<f64>() / n;
            (m, (v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (n - 1.0) / n).sqrt())
        };
        for (i, &t) in times.iter().enumerate() {
            // domestic bonds: E[exp(-int r_d)] = P_d(0, T)
            let (bond, se) = mean_se(paths.discount.iter().map(|x| x[i]).collect());
            let pd = d.discount_at(t, true).unwrap();
            assert!((bond - pd).abs() < 4.0 * se, "bond {t}: {bond} against {pd} ± {se}");
            // the FX forward: E[exp(-int r_d) S(T)] = S P_f(0, T), with the quanto drift
            let (forward, se) = mean_se(paths.discount.iter().zip(&paths.fx).map(|(x, s)| x[i] * s[i]).collect());
            let expected = 150.0 * f.discount_at(t, true).unwrap();
            assert!((forward - expected).abs() < 4.0 * se, "forward {t}: {forward} against {expected} ± {se}");
            // the variance of ln S(T) under the T-forward measure: E[D S^2] / (P_d F^2) = exp(V(T))
            let fwd = expected / pd;
            let (second, se) = mean_se(paths.discount.iter().zip(&paths.fx).map(|(x, s)| x[i] * s[i] * s[i] / (pd * fwd * fwd)).collect());
            let exact = model.fx_forward_variance(t).exp();
            assert!((second - exact).abs() < 4.0 * se, "second moment {t}: {second} against {exact} ± {se}");
        }
        // the note: the paths against the closed form, which the rates move away from the simple model
        let (mc, se) = compute_prdc_three_factor_monte_carlo(&terms(), &model, 20_000, 12, 7).unwrap();
        let exact = compute_prdc_three_factor_analytic(&terms(), &model).unwrap();
        assert!((mc - exact).abs() < 4.0 * se, "{mc} against {exact} ± {se}");
        let simple = compute_prdc_analytic_value(&terms(), &PrdcMarket { spot: 150.0, volatility: 0.11, domestic: &d, foreign: &f }).unwrap();
        assert!((exact - simple).abs() > 1e-3 * simple, "the rates should matter: {exact} against {simple}");
        // the long-dated FX variance grows with the rate volatilities when the FX rate is
        // positively correlated with the domestic rate and negatively with the foreign one
        let more = three_factor(&d, &f, [0.01, 0.012], [0.0, 0.5, -0.5]);
        assert!(more.fx_forward_variance(5.0) > 0.11 * 0.11 * 5.0);
        // invalid inputs
        assert!(generate_prdc_three_factor_paths(&times, &three_factor(&d, &f, [0.01, 0.01], [0.9, 0.9, -0.9]), 10, 4, 1).is_err());
        assert!(generate_prdc_three_factor_paths(&times, &three_factor(&d, &f, [-0.01, 0.01], [0.0; 3]), 10, 4, 1).is_err());
        assert!(generate_prdc_three_factor_paths(&[2.0, 1.0], &model, 10, 4, 1).is_err());
    }

    #[test]
    fn a_callable_note_without_a_useful_call_is_the_note() {
        let (d, f) = (jpy(), foreign());
        let model = three_factor(&d, &f, [0.01, 0.012], [0.3, -0.2, 0.4]);
        let (plain, _) = compute_prdc_three_factor_monte_carlo(&terms(), &model, 4000, 12, 11).unwrap();
        // no call dates, or a call price nobody pays: the note on the same paths
        for (dates, price) in [(vec![], 1e8), (vec![0usize, 1, 2, 3], 1e12)] {
            let r = compute_callable_prdc_three_factor(&terms(), &model, &dates, price, 4000, 4000, 12, 11, 13).unwrap();
            assert!((r.value / plain - 1.0).abs() < 1e-12, "{} against {plain}", r.value);
            assert!((r.noncallable_value / plain - 1.0).abs() < 1e-12);
            assert!(r.call_probability.iter().all(|&q| q == 0.0));
        }
        // a call price of 0 on the first date: the issuer always calls, and only the first coupon is paid
        let r = compute_callable_prdc_three_factor(&terms(), &model, &[0], 0.0, 4000, 4000, 12, 11, 13).unwrap();
        assert_eq!(r.call_probability, vec![1.0]);
        let first = PrdcTerms { coupon_times: vec![1.0], accrual_fractions: vec![1.0], fixed_rates: vec![None], redemption: 0.0, ..terms() };
        let exact = compute_prdc_three_factor_analytic(&first, &model).unwrap();
        assert!((r.value - exact).abs() < 4.0 * r.standard_error, "{} against {exact} ± {}", r.value, r.standard_error);
    }

    #[test]
    fn a_nearly_deterministic_callable_note_is_called_at_its_cheapest_date() {
        let (d, f) = (jpy(), foreign());
        for spot in [140.0, 165.0] {
            let model = PrdcThreeFactor { spot, fx_volatility: 1e-9, ..three_factor(&d, &f, [0.0, 0.0], [0.0; 3]) };
            let t = terms();
            let dates = [0usize, 1, 2, 3];
            let r = compute_callable_prdc_three_factor(&t, &model, &dates, t.redemption, 200, 200, 4, 3, 5).unwrap();
            // the deterministic coupons on the forwards, and the value if called after coupon k (or never)
            let scale = t.leverage / t.fx_base;
            let mut paid = 0.0;
            let mut best = f64::INFINITY;
            for i in 0..5 {
                let time = t.coupon_times[i];
                let (pd, pf) = (d.discount_at(time, true).unwrap(), f.discount_at(time, true).unwrap());
                let rate = (spot * pf / pd * scale - t.subtraction).max(t.floor).min(t.cap);
                paid += rate * t.notional * pd;
                // called after coupon i, or (i = 4) redeemed at maturity
                best = best.min(paid + t.redemption * pd);
            }
            assert!((r.value / best - 1.0).abs() < 1e-7, "{spot}: {} against {best}", r.value);
            assert!(r.value <= r.noncallable_value * (1.0 + 1e-12));
        }
    }

    #[test]
    fn the_call_lowers_the_note_within_its_bounds() {
        let (d, f) = (jpy(), foreign());
        // a spot above the base makes the coupons rich, so that the issuer has reasons to call
        let model = PrdcThreeFactor { spot: 158.0, ..three_factor(&d, &f, [0.01, 0.012], [0.3, -0.2, 0.4]) };
        let t = terms();
        let r = compute_callable_prdc_three_factor(&t, &model, &[0, 1, 2, 3], t.redemption, 20_000, 20_000, 12, 17, 19).unwrap();
        assert!(r.hindsight_value <= r.value && r.value < r.noncallable_value, "{} {} {}", r.hindsight_value, r.value, r.noncallable_value);
        assert!(r.noncallable_value - r.value > 4.0 * r.standard_error, "the call should be worth something");
        let total: f64 = r.call_probability.iter().sum();
        assert!(total > 0.0 && total <= 1.0);
        // the same rule from the evaluation paths themselves differs only by the regression's noise
        let same = compute_callable_prdc_three_factor(&t, &model, &[0, 1, 2, 3], t.redemption, 20_000, 20_000, 12, 17, 17).unwrap();
        assert!((same.value - r.value).abs() < 4.0 * r.standard_error);
        // invalid call dates and prices
        assert!(compute_callable_prdc_three_factor(&t, &model, &[4], t.redemption, 10, 10, 4, 1, 2).is_err());
        assert!(compute_callable_prdc_three_factor(&t, &model, &[2, 1], t.redemption, 10, 10, 4, 1, 2).is_err());
        assert!(compute_callable_prdc_three_factor(&t, &model, &[1], -1.0, 10, 10, 4, 1, 2).is_err());
    }

    /// The curve of `curve` with the log discount factor of node `node` (0 for the first year) moved by `h`.
    fn bumped_curve(rates: &[f64], node: usize, h: f64) -> YieldCurve {
        let today = Date::from_ymd(2026, 11, 2).unwrap();
        let mut dates = vec![today];
        let mut discounts = vec![1.0];
        let mut sum = 0.0;
        for (i, r) in rates.iter().enumerate() {
            sum += r;
            dates.push(Date::from_serial(today.serial() as i64 + 365 * (i as i64 + 1)).unwrap());
            discounts.push((-sum + if i == node { h } else { 0.0 }).exp());
        }
        YieldCurve::interpolated_discounts(dates, discounts, dc(), LogDiscountInterpolation::LogLinear).unwrap()
    }

    #[test]
    fn the_three_factor_derivatives_agree_with_numeric_differences() {
        let (d, f) = (jpy(), foreign());
        let model = three_factor(&d, &f, [0.01, 0.012], [0.3, -0.2, 0.4]);
        let t = terms();
        let a = compute_prdc_three_factor_sensitivity(&t, &model).unwrap();
        assert!((a.npv - compute_prdc_three_factor_analytic(&t, &model).unwrap()).abs() < 1e-7);
        let value = |m: &PrdcThreeFactor| compute_prdc_three_factor_analytic(&t, m).unwrap();
        // central differences, each input moved by h
        let check = |name: &str, got: f64, up: PrdcThreeFactor, down: PrdcThreeFactor, h: f64| {
            let numeric = (value(&up) - value(&down)) / (2.0 * h);
            assert!((got - numeric).abs() < 1e-5 * numeric.abs().max(1e3), "{name}: {got} against {numeric}");
        };
        check("spot", a.fx_delta, PrdcThreeFactor { spot: 150.0 + 1e-3, ..model }, PrdcThreeFactor { spot: 150.0 - 1e-3, ..model }, 1e-3);
        check("fx vol", a.fx_vega, PrdcThreeFactor { fx_volatility: 0.11 + 1e-6, ..model }, PrdcThreeFactor { fx_volatility: 0.11 - 1e-6, ..model }, 1e-6);
        let rate = |a: f64, s: f64| HullWhiteFactor { mean_reversion: a, volatility: s };
        let h = 1e-6;
        check("a_d", a.domestic_mean_reversion, PrdcThreeFactor { domestic_rate: rate(0.03 + h, 0.01), ..model }, PrdcThreeFactor { domestic_rate: rate(0.03 - h, 0.01), ..model }, h);
        check("sigma_d", a.domestic_volatility, PrdcThreeFactor { domestic_rate: rate(0.03, 0.01 + h), ..model }, PrdcThreeFactor { domestic_rate: rate(0.03, 0.01 - h), ..model }, h);
        check("a_f", a.foreign_mean_reversion, PrdcThreeFactor { foreign_rate: rate(0.05 + h, 0.012), ..model }, PrdcThreeFactor { foreign_rate: rate(0.05 - h, 0.012), ..model }, h);
        check("sigma_f", a.foreign_volatility, PrdcThreeFactor { foreign_rate: rate(0.05, 0.012 + h), ..model }, PrdcThreeFactor { foreign_rate: rate(0.05, 0.012 - h), ..model }, h);
        for k in 0..3 {
            let (mut up, mut down) = (model.correlation, model.correlation);
            up[k] += h;
            down[k] -= h;
            check("rho", a.correlation[k], PrdcThreeFactor { correlation: up, ..model }, PrdcThreeFactor { correlation: down, ..model }, h);
        }
        // every node of both curves, against rebuilding the curve
        let (jpy_rates, foreign_rates) = ([0.01, 0.012, 0.014, 0.016, 0.018], [0.04, 0.038, 0.036, 0.034, 0.032]);
        for domestic in [true, false] {
            let nodes: Vec<f64> = a.curves.iter().filter(|c| c.domestic == domestic).map(|c| c.d_npv).collect();
            assert_eq!(nodes.len(), 5);
            for (node, got) in nodes.iter().enumerate() {
                let at = |h: f64| {
                    let (dd, ff) = if domestic { (bumped_curve(&jpy_rates, node, h), f.clone()) } else { (d.clone(), bumped_curve(&foreign_rates, node, h)) };
                    compute_prdc_three_factor_analytic(&t, &PrdcThreeFactor { domestic: &dd, foreign: &ff, ..model }).unwrap()
                };
                let numeric = (at(1e-6) - at(-1e-6)) / 2e-6;
                assert!((got - numeric).abs() < 1e-4 * numeric.abs().max(1e3), "{domestic} node {node}: {got} against {numeric}");
            }
        }
        // without rate volatility the FX derivatives are the simple model's
        let flat = three_factor(&d, &f, [0.0, 0.0], [0.3, -0.2, 0.4]);
        let simple = compute_prdc_analytic(&t, 150.0, 0.11, &d, &f).unwrap();
        let three = compute_prdc_three_factor_sensitivity(&t, &flat).unwrap();
        assert!((three.fx_delta / simple.fx_delta - 1.0).abs() < 1e-10 && (three.fx_vega / simple.fx_vega - 1.0).abs() < 1e-10);
    }

    #[test]
    fn the_paths_have_the_closed_form_derivatives() {
        // central differences of the Monte Carlo with common random numbers against the closed form's
        let (d, f) = (jpy(), foreign());
        let model = three_factor(&d, &f, [0.01, 0.012], [0.3, -0.2, 0.4]);
        let t = terms();
        let a = compute_prdc_three_factor_sensitivity(&t, &model).unwrap();
        let mc = |m: &PrdcThreeFactor| compute_prdc_three_factor_monte_carlo(&t, m, 20_000, 12, 23).unwrap().0;
        let delta = (mc(&PrdcThreeFactor { spot: 151.0, ..model }) - mc(&PrdcThreeFactor { spot: 149.0, ..model })) / 2.0;
        assert!((delta - a.fx_delta).abs() < 0.03 * a.fx_delta.abs(), "delta {delta} against {}", a.fx_delta);
        let vega = (mc(&PrdcThreeFactor { fx_volatility: 0.12, ..model }) - mc(&PrdcThreeFactor { fx_volatility: 0.10, ..model })) / 0.02;
        assert!((vega - a.fx_vega).abs() < 0.05 * a.fx_vega.abs(), "vega {vega} against {}", a.fx_vega);
        let rate = |s: f64| HullWhiteFactor { mean_reversion: 0.05, volatility: s };
        let foreign_vega = (mc(&PrdcThreeFactor { foreign_rate: rate(0.022), ..model }) - mc(&PrdcThreeFactor { foreign_rate: rate(0.002), ..model })) / 0.02;
        assert!((foreign_vega - a.foreign_volatility).abs() < 0.1 * a.foreign_volatility.abs(), "foreign vega {foreign_vega} against {}", a.foreign_volatility);
    }

    #[test]
    fn callable_derivatives_without_a_call_are_the_closed_forms() {
        let (d, f) = (jpy(), foreign());
        let model = three_factor(&d, &f, [0.01, 0.012], [0.3, -0.2, 0.4]);
        let t = terms();
        let a = compute_prdc_three_factor_sensitivity(&t, &model).unwrap();
        let s = compute_callable_prdc_three_factor_sensitivity(&t, &model, &[], t.redemption, 20_000, 20_000, 12, 3, 5, 1e-4).unwrap();
        let near = |name: &str, e: PrdcEstimate, exact: f64| {
            assert!((e.value - exact).abs() < 4.0 * e.standard_error + 1e-9 * exact.abs(), "{name}: {} ± {} against {exact}", e.value, e.standard_error);
        };
        near("delta", s.fx_delta, a.fx_delta);
        near("vega", s.fx_vega, a.fx_vega);
        near("a_d", s.domestic_mean_reversion, a.domestic_mean_reversion);
        near("sigma_d", s.domestic_volatility, a.domestic_volatility);
        near("a_f", s.foreign_mean_reversion, a.foreign_mean_reversion);
        near("sigma_f", s.foreign_volatility, a.foreign_volatility);
        for k in 0..3 {
            near("rho", s.correlation[k], a.correlation[k]);
        }
        assert_eq!(s.curves.len(), a.curves.len());
        for (c, e) in s.curves.iter().zip(&a.curves) {
            assert_eq!((c.domestic, c.input), (e.domestic, e.input));
            near("curve", c.d_npv, e.d_npv);
        }
    }

    #[test]
    fn the_pathwise_callable_derivatives_are_those_of_the_paths_with_their_stops() {
        // each evaluation path keeps the stop of the base model; then the value is smooth in the inputs,
        // and central differences of it must give the pathwise derivatives
        let (d, f) = (jpy(), foreign());
        let model = PrdcThreeFactor { spot: 158.0, ..three_factor(&d, &f, [0.01, 0.012], [0.3, -0.2, 0.4]) };
        let t = terms();
        let dates = [0usize, 1, 2, 3];
        let paths = 4000;
        let s = compute_callable_prdc_three_factor_sensitivity(&t, &model, &dates, t.redemption, paths, paths, 12, 3, 5, 1e-4).unwrap();
        let train = generate_prdc_three_factor_paths(&t.coupon_times, &model, paths, 12, 5).unwrap();
        let rule = estimate_call_rule(&t, check_call_terms(&t, &dates, t.redemption).unwrap(), t.redemption, &train).unwrap();
        let base = generate_prdc_three_factor_paths(&t.coupon_times, &model, paths, 12, 3).unwrap();
        let stops: Vec<(usize, bool)> = (0..paths).map(|p| { let r = follow_call_rule(&t, &rule, t.redemption, &base, p); (r.stop, r.called) }).collect();
        let with_stops = |m: &PrdcThreeFactor| -> f64 {
            let e = generate_prdc_three_factor_paths(&t.coupon_times, m, paths, 12, 3).unwrap();
            (0..paths)
                .map(|p| {
                    // the call price is the redemption here, so a call and the maturity pay the same amount
                    let (stop, _) = stops[p];
                    let paid: f64 = (0..=stop).map(|i| coupon_amount(&t, i, e.fx[p][i]) * e.discount[p][i]).sum();
                    paid + t.redemption * e.discount[p][stop]
                })
                .sum::<f64>()
                / paths as f64
        };
        let delta = (with_stops(&PrdcThreeFactor { spot: 158.0 + 1e-4, ..model }) - with_stops(&PrdcThreeFactor { spot: 158.0 - 1e-4, ..model })) / 2e-4;
        assert!((s.fx_delta.value - delta).abs() < 1e-6 * delta.abs(), "delta {} against {delta}", s.fx_delta.value);
        let (jpy_rates, foreign_rates) = ([0.01, 0.012, 0.014, 0.016, 0.018], [0.04, 0.038, 0.036, 0.034, 0.032]);
        for (domestic, node) in [(true, 0usize), (true, 2), (true, 4), (false, 1), (false, 3)] {
            let at = |h: f64| {
                let (dd, ff) = if domestic { (bumped_curve(&jpy_rates, node, h), f.clone()) } else { (d.clone(), bumped_curve(&foreign_rates, node, h)) };
                with_stops(&PrdcThreeFactor { domestic: &dd, foreign: &ff, ..model })
            };
            let numeric = (at(1e-6) - at(-1e-6)) / 2e-6;
            let got = s.curves.iter().filter(|c| c.domestic == domestic).nth(node).unwrap().d_npv.value;
            assert!((got - numeric).abs() < 1e-4 * numeric.abs().max(1e3), "{domestic} node {node}: {got} against {numeric}");
        }
        // the call lowers the delta: the issuer takes away the coupons of a high FX rate
        let a = compute_prdc_three_factor_sensitivity(&t, &model).unwrap();
        assert!(s.fx_delta.value < 0.7 * a.fx_delta);
    }

    #[test]
    fn a_nearly_deterministic_callable_delta_is_that_of_its_cheapest_branch() {
        let (d, f) = (jpy(), foreign());
        let t = terms();
        for spot in [140.0, 165.0] {
            let model = PrdcThreeFactor { spot, fx_volatility: 1e-9, ..three_factor(&d, &f, [0.0, 0.0], [0.0; 3]) };
            let s = compute_callable_prdc_three_factor_sensitivity(&t, &model, &[0, 1, 2, 3], t.redemption, 200, 200, 4, 3, 5, 1e-4).unwrap();
            // the branch: the cheapest of calling after coupon k or redeeming at maturity
            let scale = t.leverage / t.fx_base;
            let (mut paid, mut slope) = (0.0, 0.0);
            let (mut best, mut best_slope) = (f64::INFINITY, 0.0);
            for i in 0..5 {
                let time = t.coupon_times[i];
                let (pd, pf) = (d.discount_at(time, true).unwrap(), f.discount_at(time, true).unwrap());
                let forward = spot * pf / pd;
                let raw = forward * scale - t.subtraction;
                paid += raw.max(t.floor).min(t.cap) * t.notional * pd;
                if raw > t.floor && raw < t.cap {
                    slope += scale * t.notional * pd * forward / spot;
                }
                if paid + t.redemption * pd < best {
                    best = paid + t.redemption * pd;
                    best_slope = slope;
                }
            }
            assert!((s.fx_delta.value - best_slope).abs() < 1e-6 * best_slope.abs().max(1.0), "{spot}: {} against {best_slope}", s.fx_delta.value);
        }
    }

    #[test]
    fn with_an_accurate_rule_the_callable_delta_is_the_rule_re_estimating_one() {
        // the envelope theorem: with many training paths, holding the rule changes the delta little
        let (d, f) = (jpy(), foreign());
        let model = PrdcThreeFactor { spot: 158.0, ..three_factor(&d, &f, [0.01, 0.012], [0.3, -0.2, 0.4]) };
        let t = terms();
        let dates = [0usize, 1, 2, 3];
        let s = compute_callable_prdc_three_factor_sensitivity(&t, &model, &dates, t.redemption, 20_000, 200_000, 12, 3, 5, 1e-4).unwrap();
        let value = |m: &PrdcThreeFactor| compute_callable_prdc_three_factor(&t, m, &dates, t.redemption, 20_000, 200_000, 12, 3, 5).unwrap().value;
        let re_estimated = (value(&PrdcThreeFactor { spot: 158.5, ..model }) - value(&PrdcThreeFactor { spot: 157.5, ..model })) / 1.0;
        assert!((s.fx_delta.value - re_estimated).abs() < 0.02 * re_estimated.abs(), "{} against {re_estimated}", s.fx_delta.value);
        assert!(compute_callable_prdc_three_factor_sensitivity(&t, &model, &dates, t.redemption, 10, 10, 4, 3, 5, 0.0).is_err());
    }

    #[test]
    fn a_target_redemption_note_against_its_limits() {
        let (d, f) = (jpy(), foreign());
        let model = three_factor(&d, &f, [0.01, 0.012], [0.3, -0.2, 0.4]);
        let t = terms();
        // no target: the note on the same paths
        let none = TargetRedemption { target: f64::INFINITY, coupon: TargetCoupon::Full, make_whole: false };
        let r = compute_target_redemption_prdc(&t, &model, &none, 4000, 12, 7).unwrap();
        let (plain, _) = compute_prdc_three_factor_monte_carlo(&t, &model, 4000, 12, 7).unwrap();
        assert!((r.value / plain - 1.0).abs() < 1e-12 && (r.plain_value / plain - 1.0).abs() < 1e-12);
        assert!(r.redemption_probability.iter().all(|&q| q == 0.0) && r.expected_life == 5.0);
        // a target below the floor: every path ends on the first date, paying the target and the redemption
        let floored = PrdcTerms { floor: 0.01, ..t.clone() };
        let tiny = TargetRedemption { target: 0.005, coupon: TargetCoupon::Capped, make_whole: false };
        let r = compute_target_redemption_prdc(&floored, &model, &tiny, 4000, 12, 7).unwrap();
        assert_eq!(r.redemption_probability[0], 1.0);
        assert!((r.expected_life - 1.0).abs() < 1e-15);
        let exact = (0.005 * floored.notional + floored.redemption) * d.discount_at(1.0, true).unwrap();
        assert!((r.value - exact).abs() < 4.0 * r.standard_error + 1e-9 * exact, "{} against {exact} ± {}", r.value, r.standard_error);
        // the same paths: a full last coupon pays at least the capped one, a make-whole at least nothing
        for target in [0.06, 0.12, 0.2] {
            let capped = compute_target_redemption_prdc(&t, &model, &TargetRedemption { target, coupon: TargetCoupon::Capped, make_whole: false }, 4000, 12, 7).unwrap();
            let full = compute_target_redemption_prdc(&t, &model, &TargetRedemption { target, coupon: TargetCoupon::Full, make_whole: false }, 4000, 12, 7).unwrap();
            let whole = compute_target_redemption_prdc(&t, &model, &TargetRedemption { target, coupon: TargetCoupon::Capped, make_whole: true }, 4000, 12, 7).unwrap();
            assert!(full.value >= capped.value && whole.value >= capped.value, "{target}");
            assert_eq!(full.redemption_probability, capped.redemption_probability);
            assert!(capped.redemption_probability.iter().sum::<f64>() <= 1.0);
            assert!(capped.expected_life <= 5.0);
        }
        assert!(compute_target_redemption_prdc(&t, &model, &TargetRedemption { target: 0.0, coupon: TargetCoupon::Full, make_whole: false }, 10, 4, 1).is_err());
        assert!(compute_target_redemption_prdc(&t, &model, &TargetRedemption { make_whole: true, ..none }, 10, 4, 1).is_err());
    }

    #[test]
    fn a_nearly_deterministic_target_redemption_follows_its_forward_path() {
        let (d, f) = (jpy(), foreign());
        let t = terms();
        for (spot, target) in [(158.0, 0.15), (165.0, 0.1), (140.0, 0.02)] {
            let model = PrdcThreeFactor { spot, fx_volatility: 1e-9, ..three_factor(&d, &f, [0.0, 0.0], [0.0; 3]) };
            let rule = TargetRedemption { target, coupon: TargetCoupon::Capped, make_whole: true };
            let r = compute_target_redemption_prdc(&t, &model, &rule, 50, 4, 3).unwrap();
            // the coupons on the forwards, until the sum reaches the target
            let scale = t.leverage / t.fx_base;
            let (mut sum, mut value) = (0.0, 0.0);
            for i in 0..5 {
                let time = t.coupon_times[i];
                let (pd, pf) = (d.discount_at(time, true).unwrap(), f.discount_at(time, true).unwrap());
                let rate = (spot * pf / pd * scale - t.subtraction).max(t.floor).min(t.cap);
                if sum + rate >= target {
                    value += ((target - sum) * t.notional + t.redemption) * pd;
                    sum = f64::NAN;
                    break;
                }
                sum += rate;
                value += rate * t.notional * pd;
                if i == 4 {
                    value += (t.redemption + (target - sum) * t.notional) * pd;
                }
            }
            assert!((r.value / value - 1.0).abs() < 1e-7, "{spot} {target}: {} against {value} (sum {sum})", r.value);
        }
    }

    #[test]
    fn a_knock_out_note_against_its_limits_and_the_digital_formula() {
        let (d, f) = (jpy(), foreign());
        let model = three_factor(&d, &f, [0.01, 0.012], [0.3, -0.2, 0.4]);
        let t = terms();
        let all = vec![0usize, 1, 2, 3];
        // a level never reached: the note on the same paths
        let never = KnockOut { level: f64::INFINITY, dates: all.clone() };
        let r = compute_path_dependent_prdc(&t, &model, None, Some(&never), 4000, 12, 7).unwrap();
        assert!((r.value / r.plain_value - 1.0).abs() < 1e-12 && r.knock_out_probability.iter().all(|&q| q == 0.0));
        // a level always reached: the first coupon and the redemption on the first date, in closed form
        let always = KnockOut { level: 1e-6, dates: all.clone() };
        let r = compute_path_dependent_prdc(&t, &model, None, Some(&always), 4000, 12, 7).unwrap();
        assert_eq!(r.knock_out_probability[0], 1.0);
        let first = PrdcTerms { coupon_times: vec![1.0], accrual_fractions: vec![1.0], fixed_rates: vec![None], ..t.clone() };
        let exact = compute_prdc_three_factor_analytic(&first, &model).unwrap();
        assert!((r.value - exact).abs() < 4.0 * r.standard_error, "{} against {exact} ± {}", r.value, r.standard_error);
        // E[D(T) 1{S(T) >= K}] = P_d(0, T) N(d_2) under the T-forward measure, with the variance of 5 章
        let paths = generate_prdc_three_factor_paths(&t.coupon_times, &model, 20_000, 12, 9).unwrap();
        for (i, level) in [(0usize, 152.0), (2, 140.0)] {
            let time = t.coupon_times[i];
            let pd = d.discount_at(time, true).unwrap();
            let forward = 150.0 * f.discount_at(time, true).unwrap() / pd;
            let v = model.fx_forward_variance(time);
            let d2 = ((forward / level).ln() - 0.5 * v) / v.sqrt();
            let exact = pd * crate::math::normal::compute_normal_cdf(d2);
            let hits: Vec<f64> = paths.discount.iter().zip(&paths.fx).map(|(x, s)| if s[i] >= level { x[i] } else { 0.0 }).collect();
            let e = mean_and_error(&hits);
            assert!((e.value - exact).abs() < 4.0 * e.standard_error, "date {i}: {} against {exact} ± {}", e.value, e.standard_error);
        }
        // with the knock-out out of reach, the target note is unchanged; both can end paths together
        let target = TargetRedemption { target: 0.12, coupon: TargetCoupon::Capped, make_whole: true };
        let alone = compute_path_dependent_prdc(&t, &model, Some(&target), None, 4000, 12, 7).unwrap();
        let with_never = compute_path_dependent_prdc(&t, &model, Some(&target), Some(&never), 4000, 12, 7).unwrap();
        assert_eq!(alone, with_never);
        let both = compute_path_dependent_prdc(&t, &model, Some(&target), Some(&KnockOut { level: 155.0, dates: all.clone() }), 4000, 12, 7).unwrap();
        let ended: f64 = both.target_probability.iter().chain(&both.knock_out_probability).sum();
        assert!(ended <= 1.0 && both.knock_out_probability.iter().sum::<f64>() > 0.0 && both.target_probability.iter().sum::<f64>() > 0.0);
        // invalid knock-outs
        assert!(compute_path_dependent_prdc(&t, &model, None, Some(&KnockOut { level: 150.0, dates: vec![4] }), 10, 4, 1).is_err());
        assert!(compute_path_dependent_prdc(&t, &model, None, Some(&KnockOut { level: 150.0, dates: vec![2, 1] }), 10, 4, 1).is_err());
        assert!(compute_path_dependent_prdc(&t, &model, None, Some(&KnockOut { level: 0.0, dates: vec![1] }), 10, 4, 1).is_err());
    }

    #[test]
    fn a_nearly_deterministic_knock_out_follows_its_forward_path() {
        let (d, f) = (jpy(), foreign());
        let t = terms();
        for (spot, level) in [(150.0, 147.0), (158.0, 150.0), (150.0, 160.0)] {
            let model = PrdcThreeFactor { spot, fx_volatility: 1e-9, ..three_factor(&d, &f, [0.0, 0.0], [0.0; 3]) };
            let r = compute_path_dependent_prdc(&t, &model, None, Some(&KnockOut { level, dates: vec![0, 1, 2, 3] }), 50, 4, 3).unwrap();
            let scale = t.leverage / t.fx_base;
            let mut value = 0.0;
            for i in 0..5 {
                let time = t.coupon_times[i];
                let (pd, pf) = (d.discount_at(time, true).unwrap(), f.discount_at(time, true).unwrap());
                let forward = spot * pf / pd;
                value += (forward * scale - t.subtraction).max(t.floor).min(t.cap) * t.notional * pd;
                if i == 4 || forward >= level {
                    value += t.redemption * pd;
                    break;
                }
            }
            assert!((r.value / value - 1.0).abs() < 1e-7, "{spot} {level}: {} against {value}", r.value);
        }
    }

    /// ATM payer swaptions into two-year annual swaps, priced by Hull-White with `(a, sigma)` on `curve`.
    // ---- the FX volatility's term structure ----

    #[test]
    fn a_flat_term_structure_gives_back_the_constant_volatility() {
        let (d, f) = (jpy(), foreign());
        let model = three_factor(&d, &f, [0.01, 0.012], [0.3, -0.2, 0.4]);
        let times = terms().coupon_times;
        let base = generate_prdc_three_factor_paths(&times, &model, 2000, 12, 5).unwrap();
        // one piece: the same computation
        let one = FxVolatilityTerm { times: &[10.0], volatilities: &[0.11] };
        let single = PrdcThreeFactor { fx_volatility: 0.5, fx_volatility_term: Some(one), ..model };
        assert_eq!(generate_prdc_three_factor_paths(&times, &single, 2000, 12, 5).unwrap(), base);
        assert_eq!(single.fx_forward_variance(3.0), model.fx_forward_variance(3.0));
        // pieces of the same volatility ending on and off the time grid (0.7 and 2.45 split steps): the
        // same random numbers, so the same paths up to rounding
        let ends = [0.7, 1.0, 2.45, 4.0, 6.0];
        let flat = FxVolatilityTerm { times: &ends, volatilities: &[0.11; 5] };
        let split = PrdcThreeFactor { fx_volatility_term: Some(flat), ..model };
        let other = generate_prdc_three_factor_paths(&times, &split, 2000, 12, 5).unwrap();
        let worst = |a: &Vec<Vec<f64>>, b: &Vec<Vec<f64>>| -> f64 {
            a.iter().flatten().zip(b.iter().flatten()).map(|(x, y)| ((x - y) / x.abs().max(1e-3)).abs()).fold(0.0, f64::max)
        };
        assert!(worst(&base.fx, &other.fx) < 1e-12 && worst(&base.discount, &other.discount) < 1e-12, "{}", worst(&base.fx, &other.fx));
        assert!(worst(&base.foreign_factor, &other.foreign_factor) < 1e-12);
        for t in [0.5, 1.0, 3.0, 7.0] {
            assert!((split.fx_forward_variance(t) / model.fx_forward_variance(t) - 1.0).abs() < 1e-13);
            assert!((split.fx_total_variance(t) / (0.11 * 0.11 * t) - 1.0).abs() < 1e-14);
        }
        let (a, b) = (compute_prdc_three_factor_analytic(&terms(), &model).unwrap(), compute_prdc_three_factor_analytic(&terms(), &split).unwrap());
        assert!((a / b - 1.0).abs() < 1e-13);
        let target = TargetRedemption { target: 0.12, coupon: TargetCoupon::Capped, make_whole: false };
        let (a, b) = (
            compute_path_dependent_prdc(&terms(), &model, Some(&target), None, 2000, 12, 5).unwrap(),
            compute_path_dependent_prdc(&terms(), &split, Some(&target), None, 2000, 12, 5).unwrap(),
        );
        assert!((a.value / b.value - 1.0).abs() < 1e-12 && a.target_probability == b.target_probability);
        // the closed form's derivatives: the parallel vega is the constant one, split over the pieces
        let (sa, sb) = (compute_prdc_three_factor_sensitivity(&terms(), &model).unwrap(), compute_prdc_three_factor_sensitivity(&terms(), &split).unwrap());
        assert!((sa.fx_vega / sb.fx_vega - 1.0).abs() < 1e-12 && sb.fx_vega_term.len() == 5 && sa.fx_vega_term == vec![sa.fx_vega]);
        assert!((sa.fx_delta / sb.fx_delta - 1.0).abs() < 1e-12);
        // the callable note's vega: the parallel shift of the pieces is the constant one's step
        let callable = |m: &PrdcThreeFactor| compute_callable_prdc_three_factor_sensitivity(&terms(), m, &[1, 2], 1e8, 2000, 2000, 12, 5, 6, 1e-3).unwrap();
        let (ca, cb) = (callable(&model), callable(&split));
        assert!((ca.fx_vega.value / cb.fx_vega.value - 1.0).abs() < 1e-9, "{:?} against {:?}", ca.fx_vega, cb.fx_vega);
        // invalid terms
        for (t, v) in [(&[1.0, 1.0][..], &[0.1, 0.1][..]), (&[0.0][..], &[0.1][..]), (&[1.0][..], &[0.0][..]), (&[1.0, 2.0][..], &[0.1][..])] {
            let bad = PrdcThreeFactor { fx_volatility_term: Some(FxVolatilityTerm { times: t, volatilities: v }), ..model };
            assert!(compute_prdc_three_factor_analytic(&terms(), &bad).is_err());
        }
    }

    /// `P_d(0, T) Black(F, K, V)` for a call.
    fn black_call(pd: f64, forward: f64, strike: f64, variance: f64) -> f64 {
        let s = variance.sqrt();
        let d1 = ((forward / strike).ln() + 0.5 * variance) / s;
        let n = crate::math::normal::compute_normal_cdf;
        pd * (forward * n(d1) - strike * n(d1 - s))
    }

    #[test]
    fn term_structure_paths_price_fx_options_by_their_total_variance() {
        let (d, f) = (jpy(), foreign());
        let ends = [1.0, 2.0, 3.0, 5.0];
        let vols = [0.08, 0.13, 0.16, 0.10];
        let term = FxVolatilityTerm { times: &ends, volatilities: &vols };
        let model = PrdcThreeFactor { fx_volatility_term: Some(term), ..three_factor(&d, &f, [0.01, 0.012], [0.3, -0.2, 0.4]) };
        // without rates, V is the total variance
        let still = PrdcThreeFactor { domestic_rate: HullWhiteFactor { mean_reversion: 0.03, volatility: 0.0 }, foreign_rate: HullWhiteFactor { mean_reversion: 0.05, volatility: 0.0 }, ..model };
        for (t, w) in [(0.5, 0.08 * 0.08 * 0.5), (2.5, 0.0064 + 0.0169 + 0.0256 * 0.5), (6.0, 0.0064 + 0.0169 + 0.0256 + 0.01 * 3.0)] {
            assert!((still.fx_forward_variance(t) / w - 1.0).abs() < 1e-13 && (term.total_variance(t) / w - 1.0).abs() < 1e-14);
        }
        // the paths: FX calls on each date against Black's formula with V(T), and the note against its closed form
        let times = terms().coupon_times;
        let paths = generate_prdc_three_factor_paths(&times, &model, 40_000, 12, 11).unwrap();
        for (i, &t) in times.iter().enumerate() {
            let pd = d.discount_at(t, true).unwrap();
            let forward = 150.0 * f.discount_at(t, true).unwrap() / pd;
            let v = model.fx_forward_variance(t);
            for strike in [130.0, 150.0, 170.0] {
                let calls: Vec<f64> = paths.discount.iter().zip(&paths.fx).map(|(x, s)| x[i] * (s[i] - strike).max(0.0)).collect();
                let e = mean_and_error(&calls);
                let exact = black_call(pd, forward, strike, v);
                assert!((e.value - exact).abs() < 4.0 * e.standard_error, "{t} {strike}: {} against {exact} ± {}", e.value, e.standard_error);
            }
        }
        let (mc, se) = compute_prdc_three_factor_monte_carlo(&terms(), &model, 40_000, 12, 13).unwrap();
        let exact = compute_prdc_three_factor_analytic(&terms(), &model).unwrap();
        assert!((mc - exact).abs() < 4.0 * se, "{mc} against {exact} ± {se}");
        let flat = compute_prdc_three_factor_analytic(&terms(), &three_factor(&d, &f, [0.01, 0.012], [0.3, -0.2, 0.4])).unwrap();
        assert!((exact - flat).abs() > 2e-4 * flat, "the term structure should matter: {exact} against {flat}");
        // the closed form's derivative with respect to each piece
        let s = compute_prdc_three_factor_sensitivity(&terms(), &model).unwrap();
        for k in 0..4 {
            let at = |h: f64| {
                let mut v = vols;
                v[k] += h;
                compute_prdc_three_factor_analytic(&terms(), &PrdcThreeFactor { fx_volatility_term: Some(FxVolatilityTerm { times: &ends, volatilities: &v }), ..model }).unwrap()
            };
            let numeric = (at(1e-6) - at(-1e-6)) / 2e-6;
            assert!((s.fx_vega_term[k] - numeric).abs() < 1e-6 * s.fx_vega_term.iter().map(|x| x.abs()).fold(0.0, f64::max), "piece {k}: {} against {numeric}", s.fx_vega_term[k]);
        }
    }

    #[test]
    fn the_term_structure_calibration_matches_each_quote() {
        let (d, f) = (jpy(), foreign());
        let model = three_factor(&d, &f, [0.008, 0.011], [0.3, -0.2, 0.4]);
        let maturities = [0.5, 1.0, 2.0, 3.0, 5.0];
        let quotes = [0.10, 0.105, 0.115, 0.12, 0.118];
        let fit = calibrate_prdc_fx_volatility_term(&model, &maturities, &quotes).unwrap();
        let fitted = PrdcThreeFactor { fx_volatility_term: Some(fit.term()), ..model };
        for (i, &t) in maturities.iter().enumerate() {
            let v = (fitted.fx_forward_variance(t) / t).sqrt();
            assert!((v - quotes[i]).abs() < 1e-13 && (fit.model_volatilities[i] - quotes[i]).abs() < 1e-13, "{t}: {v}");
        }
        // without rates the pieces are the increments of the total variance
        let still = three_factor(&d, &f, [0.0, 0.0], [0.3, -0.2, 0.4]);
        let plain = calibrate_prdc_fx_volatility_term(&still, &maturities, &quotes).unwrap();
        let mut before = (0.0, 0.0);
        for (i, &t) in maturities.iter().enumerate() {
            let w = quotes[i] * quotes[i] * t;
            let expected = ((w - before.1) / (t - before.0)).sqrt();
            assert!((plain.volatilities[i] - expected).abs() < 1e-14, "{t}: {} against {expected}", plain.volatilities[i]);
            before = (t, w);
        }
        // the rates' share differs from that of no rates
        assert!((fit.volatilities[4] - plain.volatilities[4]).abs() > 1e-3);
        // one quote: the constant fit
        let one = calibrate_prdc_fx_volatility_term(&model, &[3.0], &[0.12]).unwrap();
        let constant = calibrate_prdc_fx_volatility(&model, &[3.0], &[0.12]).unwrap();
        assert!((one.volatilities[0] - constant.fx_volatility).abs() < 1e-13);
        // a total variance that falls, unordered maturities, and mismatched lengths are errors
        assert!(calibrate_prdc_fx_volatility_term(&still, &[1.0, 2.0], &[0.2, 0.1]).is_err());
        assert!(calibrate_prdc_fx_volatility_term(&model, &[2.0, 1.0], &[0.1, 0.1]).is_err());
        assert!(calibrate_prdc_fx_volatility_term(&model, &[1.0], &[0.1, 0.1]).is_err());
    }

    // ---- derivatives of target redemption and knock-out notes ----

    #[test]
    fn path_dependent_derivatives_without_an_ending_are_the_closed_forms() {
        let (d, f) = (jpy(), foreign());
        let model = three_factor(&d, &f, [0.01, 0.012], [0.3, -0.2, 0.4]);
        let never = KnockOut { level: f64::INFINITY, dates: vec![0, 1, 2, 3] };
        let s = compute_path_dependent_prdc_sensitivity(&terms(), &model, None, Some(&never), 20_000, 12, 3, 1e-3).unwrap();
        let exact = compute_prdc_three_factor_sensitivity(&terms(), &model).unwrap();
        let check = |name: &str, e: PathDependentDifference, x: f64| {
            assert!(e.changed_paths == 0.0, "{name}: {}", e.changed_paths);
            assert!((e.value - x).abs() < 4.0 * e.standard_error + 2e-3 * x.abs(), "{name}: {} ± {} against {x}", e.value, e.standard_error);
        };
        check("delta", s.fx_delta, exact.fx_delta);
        check("vega", s.fx_vega, exact.fx_vega);
        check("domestic volatility", s.domestic_volatility, exact.domestic_volatility);
        check("foreign mean reversion", s.foreign_mean_reversion, exact.foreign_mean_reversion);
        check("rho fX", s.correlation[2], exact.correlation[2]);
        assert_eq!(s.curves.len(), exact.curves.len());
        for (a, b) in s.curves.iter().zip(&exact.curves) {
            assert_eq!((a.domestic, a.input), (b.domestic, b.input));
            check("curve", a.d_npv, b.d_npv);
        }
    }

    /// Deterministic rates and a piecewise FX volatility: `Pr(S(T) >= L)`, its derivatives with respect to
    /// the spot and a parallel shift of the volatility, and `P_d(0, T)` for each coupon date.
    fn digital(model: &PrdcThreeFactor, d: &YieldCurve, f: &YieldCurve, time: f64, level: f64) -> (f64, f64, f64) {
        let pd = d.discount_at(time, true).unwrap();
        let forward = model.spot * f.discount_at(time, true).unwrap() / pd;
        let w = model.fx_total_variance(time);
        let d2 = ((forward / level).ln() - 0.5 * w) / w.sqrt();
        let density = crate::math::normal::compute_normal_pdf(d2);
        // dw / d(shift) = 2 int_0^T sigma_X
        let term = model.fx_volatility_term.unwrap();
        let slope: f64 = split_pieces(term.times, 0.0, time).iter().map(|&(a, b, k)| 2.0 * term.volatilities[k] * (b - a)).sum();
        (crate::math::normal::compute_normal_cdf(d2), density / (model.spot * w.sqrt()), density * (-(d2 + w.sqrt()) / (2.0 * w)) * slope)
    }

    #[test]
    fn knock_out_derivatives_against_the_digital_formula() {
        let (d, f) = (jpy(), foreign());
        let ends = [1.0, 2.0, 5.0];
        let vols = [0.09, 0.14, 0.11];
        let model = PrdcThreeFactor { fx_volatility_term: Some(FxVolatilityTerm { times: &ends, volatilities: &vols }), ..three_factor(&d, &f, [0.0, 0.0], [0.3, -0.2, 0.4]) };
        // fixed coupons and one observation on the second date: the value is linear in the digital
        let t = PrdcTerms { fixed_rates: vec![Some(0.02); 5], ..terms() };
        let (j, level) = (1usize, 152.0);
        let knock = KnockOut { level, dates: vec![j] };
        let p: Vec<f64> = t.coupon_times.iter().map(|&x| d.discount_at(x, true).unwrap()).collect();
        let coupon = 0.02 * t.notional;
        let after: f64 = p[j + 1..].iter().map(|x| coupon * x).sum::<f64>() + t.redemption * p[4];
        let jump = t.redemption * p[j] - after;
        let (pr, dpr_ds, dpr_dv) = digital(&model, &d, &f, t.coupon_times[j], level);
        let value = p[..=j].iter().map(|x| coupon * x).sum::<f64>() + after + pr * jump;
        let mut errors = Vec::new();
        for bump in [1e-2, 1e-3] {
            let s = compute_path_dependent_prdc_sensitivity(&t, &model, None, Some(&knock), 40_000, 12, 21, bump).unwrap();
            assert!((s.value.value - value).abs() < 4.0 * s.value.standard_error);
            for (name, e, x) in [("delta", s.fx_delta, jump * dpr_ds), ("vega", s.fx_vega, jump * dpr_dv)] {
                assert!(e.changed_paths > 0.0, "{name} {bump}");
                assert!((e.value - x).abs() < 4.0 * e.standard_error + 5e-3 * x.abs(), "{name} {bump}: {} ± {} against {x}", e.value, e.standard_error);
            }
            // the rates do not move the paths' endings here, and the correlations with no rate volatility do nothing
            assert!(s.correlation.iter().all(|c| c.value.abs() < 1e-6 * value));
            errors.push(s.fx_delta.standard_error);
        }
        // a smaller step: more noise, from the paths that cross the level
        assert!(errors[1] > 2.0 * errors[0], "{errors:?}");
    }

    #[test]
    fn target_redemption_derivatives_against_the_digital_formula() {
        let (d, f) = (jpy(), foreign());
        let ends = [0.5, 1.0, 5.0];
        let vols = [0.12, 0.08, 0.11];
        let model = PrdcThreeFactor { fx_volatility_term: Some(FxVolatilityTerm { times: &ends, volatilities: &vols }), ..three_factor(&d, &f, [0.0, 0.0], [0.3, -0.2, 0.4]) };
        // the first coupon floats, the others are 0: the target of 5% is reached on the first date or
        // never, when S(T_1) >= (0.05 + 0.78) * 150 / 0.8
        let mut fixed = vec![Some(0.0); 5];
        fixed[0] = None;
        let t = PrdcTerms { fixed_rates: fixed, ..terms() };
        let target = TargetRedemption { target: 0.05, coupon: TargetCoupon::Full, make_whole: false };
        let level = (0.05 + 0.78) * 150.0 / 0.8;
        let p: Vec<f64> = t.coupon_times.iter().map(|&x| d.discount_at(x, true).unwrap()).collect();
        let jump = t.redemption * (p[0] - p[4]);
        let (pr, dpr_ds, dpr_dv) = digital(&model, &d, &f, 1.0, level);
        // the first coupon, always paid in full, by the closed form
        let first = PrdcTerms { coupon_times: vec![1.0], accrual_fractions: vec![1.0], fixed_rates: vec![None], redemption: 0.0, ..t.clone() };
        let c = compute_prdc_three_factor_sensitivity(&first, &model).unwrap();
        let value = c.npv + t.redemption * p[4] + pr * jump;
        for bump in [1e-2, 1e-3] {
            let s = compute_path_dependent_prdc_sensitivity(&t, &model, Some(&target), None, 40_000, 12, 23, bump).unwrap();
            assert!((s.value.value - value).abs() < 4.0 * s.value.standard_error, "{} against {value}", s.value.value);
            for (name, e, x) in [("delta", s.fx_delta, c.fx_delta + jump * dpr_ds), ("vega", s.fx_vega, c.fx_vega + jump * dpr_dv)] {
                assert!(e.changed_paths > 0.0, "{name} {bump}");
                assert!((e.value - x).abs() < 4.0 * e.standard_error + 5e-3 * x.abs(), "{name} {bump}: {} ± {} against {x}", e.value, e.standard_error);
            }
        }
        assert!(compute_path_dependent_prdc_sensitivity(&t, &model, Some(&target), None, 100, 12, 23, 0.0).is_err());
    }

    fn swaption_quotes(curve: &YieldCurve, a: f64, sigma: f64) -> Vec<crate::hull_white::CalibrationQuote> {
        use crate::hull_white::{compute_jamshidian_swaption, CalibrationInstrument, CalibrationQuote, HullWhite, SwaptionFlows};
        let model = HullWhite { mean_reversion: a, volatility: sigma, curve };
        [0.5, 1.0, 2.0, 3.0]
            .iter()
            .map(|&expiry| {
                let payments = vec![expiry + 1.0, expiry + 2.0];
                let p = |t: f64| curve.discount_at(t, true).unwrap();
                let rate = (p(expiry) - p(expiry + 2.0)) / (p(expiry + 1.0) + p(expiry + 2.0));
                let flows = SwaptionFlows { payer: true, maturity: expiry, value_time: expiry, payment_times: payments, coupons: vec![rate, rate], nominal: 1.0 };
                let price = compute_jamshidian_swaption(&model, &flows).unwrap();
                CalibrationQuote { instrument: CalibrationInstrument::Swaption(flows), price, weight: 1.0 }
            })
            .collect()
    }

    #[test]
    fn the_three_factor_calibration_gives_back_its_rates_and_fx_volatility() {
        use crate::hull_white::{calibrate_hull_white, CalibrationSettings};
        let (d, f) = (jpy(), foreign());
        let (dq, fq) = (swaption_quotes(&d, 0.03, 0.008), swaption_quotes(&f, 0.06, 0.011));
        let settings = CalibrationSettings::default();
        let rho = [0.3, -0.2, 0.4];
        let fit = calibrate_prdc_three_factor(150.0, &d, &f, &dq, &fq, rho, &[3.0], &[0.12], &settings).unwrap();
        // the rates: each currency's own calibration, which gives back the parameters of its prices
        assert_eq!(fit.domestic, calibrate_hull_white(&dq, &d, &settings).unwrap());
        assert_eq!(fit.foreign, calibrate_hull_white(&fq, &f, &settings).unwrap());
        assert!((fit.domestic.mean_reversion / 0.03 - 1.0).abs() < 1e-5 && (fit.domestic.volatility / 0.008 - 1.0).abs() < 1e-6, "{:?}", fit.domestic);
        assert!((fit.foreign.mean_reversion / 0.06 - 1.0).abs() < 1e-5 && (fit.foreign.volatility / 0.011 - 1.0).abs() < 1e-6, "{:?}", fit.foreign);
        // the FX volatility: one quote is matched exactly
        let model = PrdcThreeFactor {
            spot: 150.0,
            fx_volatility: fit.fx.fx_volatility,
            fx_volatility_term: None,
            domestic: &d,
            foreign: &f,
            domestic_rate: HullWhiteFactor { mean_reversion: fit.domestic.mean_reversion, volatility: fit.domestic.volatility },
            foreign_rate: HullWhiteFactor { mean_reversion: fit.foreign.mean_reversion, volatility: fit.foreign.volatility },
            correlation: rho,
        };
        assert!((model.fx_forward_variance(3.0) / 3.0 - 0.0144).abs() < 1e-14 && fit.fx.root_mean_square_error < 1e-12);
        // and the paths price an FX call at the quoted volatility
        let paths = generate_prdc_three_factor_paths(&[3.0], &model, 40_000, 12, 31).unwrap();
        let (pd, pf) = (d.discount_at(3.0, true).unwrap(), f.discount_at(3.0, true).unwrap());
        let forward = 150.0 * pf / pd;
        for strike in [130.0, 145.0, 160.0] {
            let payoffs: Vec<f64> = paths.discount.iter().zip(&paths.fx).map(|(x, s)| x[0] * (s[0] - strike).max(0.0)).collect();
            let e = mean_and_error(&payoffs);
            let black = compute_forward_value(
                ForwardModel::Black,
                OptionType::Call,
                &ForwardInputs { forward, strike, volatility: 0.12, time: 3.0, discount: pd },
            )
            .unwrap();
            assert!((e.value - black).abs() < 4.0 * e.standard_error, "{strike}: {} against {black} ± {}", e.value, e.standard_error);
        }
        // without rate volatility the FX volatility is the quote
        let still = PrdcThreeFactor { domestic_rate: HullWhiteFactor { mean_reversion: 0.03, volatility: 0.0 }, foreign_rate: HullWhiteFactor { mean_reversion: 0.06, volatility: 0.0 }, ..model };
        assert!((calibrate_prdc_fx_volatility(&still, &[3.0], &[0.12]).unwrap().fx_volatility - 0.12).abs() < 1e-14);
        // several quotes: a least-squares fit, no better one nearby
        let (maturities, vols) = ([1.0, 3.0, 5.0], [0.115, 0.12, 0.118]);
        let many = calibrate_prdc_fx_volatility(&model, &maturities, &vols).unwrap();
        let rms = |sx: f64| {
            let m = PrdcThreeFactor { fx_volatility: sx, ..model };
            (maturities.iter().zip(&vols).map(|(&t, v)| ((m.fx_forward_variance(t) / t).sqrt() - v).powi(2)).sum::<f64>() / 3.0).sqrt()
        };
        assert!((rms(many.fx_volatility) - many.root_mean_square_error).abs() < 1e-14);
        assert!(rms(many.fx_volatility + 1e-4) > many.root_mean_square_error && rms(many.fx_volatility - 1e-4) > many.root_mean_square_error);
        // a quote below what the rates give, and invalid quotes
        let wild = PrdcThreeFactor { domestic_rate: HullWhiteFactor { mean_reversion: 0.0, volatility: 0.05 }, correlation: [0.0, 0.0, 0.0], ..model };
        assert!(calibrate_prdc_fx_volatility(&wild, &[5.0], &[0.01]).is_err());
        assert!(calibrate_prdc_fx_volatility(&model, &[3.0], &[-0.1]).is_err());
        assert!(calibrate_prdc_fx_volatility(&model, &[3.0, 4.0], &[0.1]).is_err());
    }

    fn settings(threads: usize) -> PrdcMonteCarlo {
        PrdcMonteCarlo { paths: 400_000, seed: 20261101, antithetic: true, threads }
    }

    #[test]
    fn monte_carlo_agrees_with_the_closed_form() {
        let (d, f) = (jpy(), foreign());
        let a = compute_prdc_analytic(&terms(), 150.0, 0.12, &d, &f).unwrap();
        let m = compute_prdc_monte_carlo(&terms(), 150.0, 0.12, &d, &f, &settings(4)).unwrap();
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
        let a = compute_prdc_analytic(&terms(), 150.0, 0.12, &d, &f).unwrap();
        let value = |s: f64, v: f64| {
            compute_prdc_analytic_value(&terms(), &PrdcMarket { spot: s, volatility: v, domestic: &d, foreign: &f }).unwrap()
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
            compute_prdc_analytic_value(&terms(), &PrdcMarket { spot: 150.0, volatility: 0.12, domestic: &d, foreign: &bumped }).unwrap()
        };
        let numeric = (rebuilt(1e-6) - rebuilt(-1e-6)) / 2e-6;
        let got = a.curves.iter().filter(|c| !c.domestic).nth(node).unwrap().d_npv;
        assert!((got - numeric).abs() < 1e-4 * numeric.abs().max(1.0), "{got} {numeric}");
    }

    #[test]
    fn pathwise_derivatives_are_the_derivatives_of_the_same_paths() {
        let (d, f) = (jpy(), foreign());
        let s = PrdcMonteCarlo { paths: 40_000, ..settings(2) };
        let m = compute_prdc_monte_carlo(&terms(), 150.0, 0.12, &d, &f, &s).unwrap();
        let value = |x: f64, v: f64| compute_prdc_monte_carlo(&terms(), x, v, &d, &f, &s).unwrap().npv;
        let delta = (value(150.0 + 1e-3, 0.12) - value(150.0 - 1e-3, 0.12)) / 2e-3;
        let vega = (value(150.0, 0.12 + 1e-5) - value(150.0, 0.12 - 1e-5)) / 2e-5;
        assert!((m.fx_delta - delta).abs() < 1e-3 * delta.abs(), "{} {delta}", m.fx_delta);
        assert!((m.fx_vega - vega).abs() < 1e-3 * vega.abs(), "{} {vega}", m.fx_vega);
    }

    #[test]
    fn monte_carlo_does_not_depend_on_the_number_of_threads() {
        let (d, f) = (jpy(), foreign());
        let s = |t| PrdcMonteCarlo { paths: 40_000, ..settings(t) };
        let one = compute_prdc_monte_carlo(&terms(), 150.0, 0.12, &d, &f, &s(1)).unwrap();
        assert_eq!(one, compute_prdc_monte_carlo(&terms(), 150.0, 0.12, &d, &f, &s(4)).unwrap());
    }

    #[test]
    fn fixed_coupons_and_no_cap_behave() {
        let (d, f) = (jpy(), foreign());
        let mut fixed = terms();
        fixed.fixed_rates = vec![Some(0.03); 5];
        let v = compute_prdc_analytic(&fixed, 150.0, 0.12, &d, &f).unwrap();
        // a note with fixed coupons has no FX risk
        assert_eq!(v.fx_delta, 0.0);
        assert_eq!(v.fx_vega, 0.0);
        let exact: f64 = (1..=5).map(|i| 0.03 * 1e8 * d.discount_at(i as f64, true).unwrap()).sum::<f64>()
            + 1e8 * d.discount_at(5.0, true).unwrap();
        assert!((v.npv - exact).abs() < 1e-6);
        let mut uncapped = terms();
        uncapped.cap = f64::INFINITY;
        let capped = compute_prdc_analytic(&terms(), 150.0, 0.12, &d, &f).unwrap();
        assert!(compute_prdc_analytic(&uncapped, 150.0, 0.12, &d, &f).unwrap().npv > capped.npv);
    }

    #[test]
    fn invalid_terms_are_errors() {
        let (d, f) = (jpy(), foreign());
        let mut t = terms();
        t.cap = -1.0;
        assert!(compute_prdc_analytic(&t, 150.0, 0.12, &d, &f).is_err());
        let mut t = terms();
        t.coupon_times = vec![1.0, 1.0, 3.0, 4.0, 5.0];
        assert!(compute_prdc_analytic(&t, 150.0, 0.12, &d, &f).is_err());
        assert!(compute_prdc_analytic(&terms(), -1.0, 0.12, &d, &f).is_err());
    }
}
