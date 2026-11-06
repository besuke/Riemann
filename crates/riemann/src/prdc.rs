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
//! a lognormal FX rate under the domestic measure, with a closed form per coupon (the FX forward is
//! lognormal under each coupon's forward measure) and a Monte Carlo of the three factors, in `f64`
//! without sensitivities (`docs/spec_prdc.md` 5 章).
//!
//! A second model has stochastic rates: Hull-White domestic and foreign short rates and a lognormal FX
//! rate ([`PrdcThreeFactor`], `docs/spec_prdc.md` 5 章), in f64 only, with a closed form on each coupon's
//! forward measure and a Monte Carlo of the three factors.

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
/// `dS / S = (r_d - r_f) dt + sigma_X dW_X`.
#[derive(Debug, Clone, Copy)]
pub struct PrdcThreeFactor<'c> {
    /// Domestic per foreign.
    pub spot: f64,
    pub fx_volatility: f64,
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

impl PrdcThreeFactor<'_> {
    fn check(&self) -> Result<Vec<Vec<f64>>, Error> {
        if !(self.spot > 0.0) || !(self.fx_volatility > 0.0) || !self.spot.is_finite() || !self.fx_volatility.is_finite() {
            return invalid("the spot and FX volatility must be positive and finite".into());
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
#[derive(Debug, Clone, Copy)]
struct ThreeFactorParameters<T> {
    fx_volatility: T,
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

/// `V(T)` of [`PrdcThreeFactor::fx_forward_variance`]: the integrand is smooth, and 32-point
/// Gauss-Legendre quadrature integrates it to rounding for the mean reversions of practice.
fn fx_forward_variance_of<T: Operand>(p: &ThreeFactorParameters<T>, t: f64) -> T {
    let (nodes, weights) = gauss_legendre();
    let [df, dx, fx] = p.correlation;
    let sx = p.fx_volatility;
    let mut sum = sx.like(0.0);
    for (node, weight) in nodes.iter().zip(weights) {
        // tau = T - u over [0, T]
        let tau = 0.5 * t * (node + 1.0);
        let bd = p.domestic.1 * hull_white_b_of(p.domestic.0, tau);
        let bf = p.foreign.1 * hull_white_b_of(p.foreign.0, tau);
        let integrand = sx * sx + bf * bf + bd * bd - fx * sx * bf * 2.0 + dx * sx * bd * 2.0 - df * bd * bf * 2.0;
        sum = sum + integrand * (0.5 * t * weight);
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
        ThreeFactorParameters {
            fx_volatility: self.fx_volatility,
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
    pub fx_vega: f64,
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
    let fx_volatility = recorder.input(model.fx_volatility);
    let domestic = (recorder.input(model.domestic_rate.mean_reversion), recorder.input(model.domestic_rate.volatility));
    let foreign = (recorder.input(model.foreign_rate.mean_reversion), recorder.input(model.foreign_rate.volatility));
    let correlation = model.correlation.map(|r| recorder.input(r));
    let d = model.domestic.lift(&recorder)?;
    let f = model.foreign.lift(&recorder)?;
    let p = ThreeFactorParameters { fx_volatility, domestic, foreign, correlation };
    let npv = three_factor_analytic_of(terms, spot, &d.curve, &f.curve, &p)?;
    let mut wrt = vec![spot, fx_volatility, domestic.0, domestic.1, foreign.0, foreign.1, correlation[0], correlation[1], correlation[2]];
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
    Ok(PrdcThreeFactorSensitivity {
        npv: npv.primal(),
        fx_delta: g[0],
        fx_vega: g[1],
        domestic_mean_reversion: g[2],
        domestic_volatility: g[3],
        foreign_mean_reversion: g[4],
        foreign_volatility: g[5],
        correlation: [g[6], g[7], g[8]],
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
/// `ln S(T) = ln S + ln(P_f(0, T) / P_d(0, T)) + I_d - I_f + (V_d - V_f) / 2 - sigma_X^2 T / 2 + sigma_X W_X(T)`,
/// with `I = int_0^T x` and `V = Var(I)` (the foreign one under the foreign measure).
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
    let sx = model.fx_volatility;
    let [_, _, rho_fx] = model.correlation;
    // the conditional covariance of (x_d, x_f, W_X) over a step of length h, as Ornstein-Uhlenbeck
    // increments (W_X with no mean reversion), and its Cholesky factor (zero volatilities left out)
    let sigma = [d.volatility, f.volatility, sx];
    let kappa = [d.mean_reversion, f.mean_reversion, 0.0];
    let active: Vec<usize> = (0..3).filter(|&j| sigma[j] > 0.0).collect();
    let step_factor = |h: f64| -> Result<Vec<Vec<f64>>, Error> {
        let cov: Vec<Vec<f64>> = active
            .iter()
            .map(|&a| active.iter().map(|&b| correlation[a][b] * sigma[a] * sigma[b] * hull_white_b(kappa[a] + kappa[b], h)).collect())
            .collect();
        crate::math::linear::compute_cholesky(&cov)
    };
    let factors: Vec<Vec<Vec<f64>>> = grid.windows(2).map(|w| step_factor(w[1] - w[0])).collect::<Result<_, _>>()?;
    let mut fixed = Vec::with_capacity(coupon_times.len());
    for &t in coupon_times {
        let (pd, pf) = (model.domestic.discount_at(t, true)?, model.foreign.discount_at(t, true)?);
        let (vd, vf) = (integrated_variance(&d, t), integrated_variance(&f, t));
        fixed.push((pd, (model.spot * pf / pd).ln() + 0.5 * (vd - vf) - 0.5 * sx * sx * t, vd));
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
                    shock[a] = factors[k][row].iter().zip(&z).map(|(l, x)| l * x).sum();
                }
                let new_xd = xd * (-d.mean_reversion * h).exp() + shock[0];
                let new_xf = xf * (-f.mean_reversion * h).exp() - rho_fx * f.volatility * sx * hull_white_b(f.mean_reversion, h) + shock[1];
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

/// A PRDC note that the issuer may redeem at `call_price` (an amount, like `redemption`) on the coupon
/// dates `call_dates` (positions in `coupon_times`, increasing, before the last), right after paying
/// that date's coupon. The issuer calls when the value of the rest of the note exceeds the call price.
/// Longstaff and Schwartz: rolling back on paths from `training_seed`, the value of the rest, in money
/// at the date, is regressed on 1, `S / fx_base`, its square, `x_d` and `x_f` over all paths; the
/// value is then the mean over paths from `seed` of the cash flows that follow the rule, discounted by
/// `exp(-int r_d)`. A suboptimal rule overstates the note, so the estimate leans upward (the issuer's
/// option is undervalued), apart from the noise.
#[allow(clippy::too_many_arguments)]
// the paths are indexed alongside the vectors of their values
#[allow(clippy::needless_range_loop)]
pub fn compute_callable_prdc_three_factor(
    terms: &PrdcTerms,
    model: &PrdcThreeFactor,
    call_dates: &[usize],
    call_price: f64,
    paths: usize,
    steps_per_year: usize,
    seed: u64,
    training_seed: u64,
) -> Result<CallablePrdcResult, Error> {
    terms.check()?;
    let n = terms.coupon_times.len();
    if call_dates.windows(2).any(|w| w[1] <= w[0]) || call_dates.last().is_some_and(|&c| c + 1 >= n) {
        return invalid(format!("call dates must be increasing positions before the last coupon (of {n})"));
    }
    if !call_price.is_finite() || call_price < 0.0 {
        return invalid(format!("the call price must be finite and non-negative, not {call_price}"));
    }
    let scale = terms.leverage / terms.fx_base;
    let coupon = |i: usize, fx: f64| -> f64 {
        let rate = terms.fixed_rates[i].unwrap_or_else(|| (fx * scale - terms.subtraction).max(terms.floor).min(terms.cap));
        rate * terms.notional * terms.accrual_fractions[i]
    };
    let mut callable = vec![None; n];
    for (k, &c) in call_dates.iter().enumerate() {
        callable[c] = Some(k);
    }

    // the rule, rolling back on the training paths
    let train = generate_prdc_three_factor_paths(&terms.coupon_times, model, paths, steps_per_year, training_seed)?;
    let mut after: Vec<f64> = (0..paths).map(|p| train.discount[p][n - 1] * terms.redemption).collect();
    let mut rules: Vec<Option<[f64; 5]>> = vec![None; n];
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
            rules[i] = Some(rule);
        }
        for p in 0..paths {
            after[p] += coupon(i, train.fx[p][i]) * train.discount[p][i];
        }
    }

    // the evaluation paths follow the rule
    let eval = generate_prdc_three_factor_paths(&terms.coupon_times, model, paths, steps_per_year, seed)?;
    let mut values = Vec::with_capacity(paths);
    let (mut noncallable, mut hindsight) = (0.0, 0.0);
    let mut called = vec![0usize; call_dates.len()];
    for p in 0..paths {
        let (d, s) = (&eval.discount[p], &eval.fx[p]);
        let mut paid = 0.0;
        let mut value = None;
        // the value if called at each call date, for hindsight
        let mut lowest = f64::INFINITY;
        for i in 0..n {
            paid += coupon(i, s[i]) * d[i];
            if let (Some(k), Some(rule)) = (callable[i], rules[i]) {
                let if_called = paid + call_price * d[i];
                lowest = lowest.min(if_called);
                if value.is_none() {
                    let basis = call_basis(s[i], terms.fx_base, eval.domestic_factor[p][i], eval.foreign_factor[p][i]);
                    let continuation: f64 = rule.iter().zip(&basis).map(|(a, b)| a * b).sum();
                    if continuation > call_price {
                        value = Some(if_called);
                        called[k] += 1;
                    }
                }
            }
        }
        let full = paid + terms.redemption * d[n - 1];
        noncallable += full;
        hindsight += lowest.min(full);
        values.push(value.unwrap_or(full));
    }
    let count = paths as f64;
    let mean = values.iter().sum::<f64>() / count;
    let variance = values.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / (count - 1.0);
    Ok(CallablePrdcResult {
        value: mean,
        standard_error: (variance / count).sqrt(),
        noncallable_value: noncallable / count,
        hindsight_value: hindsight / count,
        call_probability: called.iter().map(|&c| c as f64 / count).collect(),
    })
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
            let r = compute_callable_prdc_three_factor(&terms(), &model, &dates, price, 4000, 12, 11, 13).unwrap();
            assert!((r.value / plain - 1.0).abs() < 1e-12, "{} against {plain}", r.value);
            assert!((r.noncallable_value / plain - 1.0).abs() < 1e-12);
            assert!(r.call_probability.iter().all(|&q| q == 0.0));
        }
        // a call price of 0 on the first date: the issuer always calls, and only the first coupon is paid
        let r = compute_callable_prdc_three_factor(&terms(), &model, &[0], 0.0, 4000, 12, 11, 13).unwrap();
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
            let r = compute_callable_prdc_three_factor(&t, &model, &dates, t.redemption, 200, 4, 3, 5).unwrap();
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
        let r = compute_callable_prdc_three_factor(&t, &model, &[0, 1, 2, 3], t.redemption, 20_000, 12, 17, 19).unwrap();
        assert!(r.hindsight_value <= r.value && r.value < r.noncallable_value, "{} {} {}", r.hindsight_value, r.value, r.noncallable_value);
        assert!(r.noncallable_value - r.value > 4.0 * r.standard_error, "the call should be worth something");
        let total: f64 = r.call_probability.iter().sum();
        assert!(total > 0.0 && total <= 1.0);
        // the same rule from the evaluation paths themselves differs only by the regression's noise
        let same = compute_callable_prdc_three_factor(&t, &model, &[0, 1, 2, 3], t.redemption, 20_000, 12, 17, 17).unwrap();
        assert!((same.value - r.value).abs() < 4.0 * r.standard_error);
        // invalid call dates and prices
        assert!(compute_callable_prdc_three_factor(&t, &model, &[4], t.redemption, 10, 4, 1, 2).is_err());
        assert!(compute_callable_prdc_three_factor(&t, &model, &[2, 1], t.redemption, 10, 4, 1, 2).is_err());
        assert!(compute_callable_prdc_three_factor(&t, &model, &[1], -1.0, 10, 4, 1, 2).is_err());
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
