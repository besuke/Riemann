//! The Hull-White one-factor model fitted to a yield curve: zero-coupon bonds and options on them,
//! European swaptions by Jamshidian's decomposition, and callable bonds and Bermudan swaptions on a
//! state grid, and the calibration of the parameters to market prices.
//!
//! Closed forms follow QuantLib v1.42 `ql/models/shortrate/onefactormodels/hullwhite.cpp` (Copyright
//! (C) 2001, 2002, 2003 Sadruddin Rejeb; (C) 2006 Chiara Fornarola; (C) 2007 StatPro Italia srl) and
//! `ql/pricingengines/swaption/jamshidianswaptionengine.cpp` (Copyright (C) 2001, 2002, 2003 Sadruddin
//! Rejeb; (C) 2013 Peter Caspers); modified BSD license. See THIRD_PARTY_NOTICES.
//!
//! The option on a bond factors QuantLib's expression, which loses its digits when `a` is small. The
//! calibration is this library's own method, and its objective, constraints and stopping tests differ
//! from QuantLib's and LagrangeFinance's as `docs/spec_hull_white.md` sets out.
//!
//! The grid follows LagrangeFinance II-8.4: the state and the discount over a step are jointly
//! normal, so each step is an exact conditional expectation, computed by Gauss-Hermite quadrature
//! and linear interpolation of the next values.

// `!(x > 0.0)` is intended: it also rejects NaN
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::autodiff::{Operand, Recorder};
use crate::curves::YieldCurve;
use crate::interest_rate::Compounding;
use crate::math::linear::solve;
use crate::math::solver1d::solve_brent;
use crate::options::{compute_forward_value, ForwardInputs, ForwardModel, OptionType};
use crate::Error;

fn invalid<T>(reason: String) -> Result<T, Error> {
    Err(Error::InvalidInput { reason })
}

/// `(1 - exp(-y)) / y`, accurate for small `y`, where its value and derivative tend to 1 and -1/2.
fn one_minus_exp_over<T: Operand>(y: T) -> T {
    if y.primal().abs() < 1e-4 {
        let cubic = y * y * y * (1.0 / 24.0);
        y * y * (1.0 / 6.0) - y * 0.5 + 1.0 - cubic
    } else {
        (-(-y).exp() + 1.0) / y
    }
}

/// The model: mean reversion, volatility and the initial curve.
#[derive(Debug, Clone, Copy)]
pub struct HullWhite<'c, T = f64> {
    pub mean_reversion: T,
    pub volatility: T,
    pub curve: &'c YieldCurve<T>,
}

impl<T: Operand> HullWhite<'_, T> {
    fn check(&self) -> Result<(), Error> {
        if !(self.mean_reversion.primal() >= 0.0) || !(self.volatility.primal() >= 0.0) {
            return invalid(format!(
                "mean reversion and volatility must not be negative ({}, {})",
                self.mean_reversion.primal(),
                self.volatility.primal()
            ));
        }
        Ok(())
    }

    fn discount(&self, t: f64) -> Result<T, Error> {
        self.curve.discount_at(t, true)
    }

    /// `(1 - exp(-a (big_t - t))) / a`, which tends to `big_t - t` as the mean reversion tends to zero.
    pub fn b(&self, t: f64, big_t: f64) -> T {
        one_minus_exp_over(self.mean_reversion * (big_t - t)) * (big_t - t)
    }

    /// QuantLib `HullWhite::A`.
    fn a_term(&self, t: f64, big_t: f64) -> Result<T, Error> {
        let (d1, d2) = (self.discount(t)?, self.discount(big_t)?);
        let forward = self
            .curve
            .forward_rate_at_time(t, Compounding::Continuous)?
            .rate;
        let b = self.b(t, big_t);
        let temp = self.volatility * b;
        let b0 = self.b(0.0, 2.0 * t);
        Ok((b * forward - temp * temp * b0 * 0.25).exp() * d2 / d1)
    }

    /// The price at `t` of a zero-coupon bond maturing at `big_t`, given the short rate at `t`.
    pub fn discount_bond(&self, t: f64, big_t: f64, rate: T) -> Result<T, Error> {
        Ok(self.a_term(t, big_t)? * (-self.b(t, big_t) * rate).exp())
    }

    /// The price of a European option expiring at `maturity` on a zero-coupon bond maturing at
    /// `bond_maturity`, with values measured at `bond_start` (QuantLib's five-argument
    /// `discountBondOption`). With `bond_start = maturity` it is the usual option on a bond.
    pub fn discount_bond_option(
        &self,
        kind: OptionType,
        strike: T,
        maturity: f64,
        bond_start: f64,
        bond_maturity: f64,
    ) -> Result<T, Error> {
        self.check()?;
        let a = self.mean_reversion;
        let sigma = self.volatility;
        // QuantLib's form subtracts exponentials, and rounding dominates it when a is small. This is the
        // same value factored: (1 - exp(-2 a T)) (exp(-a (s - T)) - exp(-a (S - T)))^2 under its square root.
        let variance_factor = one_minus_exp_over(a * (2.0 * maturity)) * maturity;
        let v = sigma * variance_factor.sqrt() * (-a * (bond_start - maturity)).exp() * self.b(bond_start, bond_maturity);
        let forward = self.discount(bond_maturity)?;
        let k = self.discount(bond_start)? * strike;
        // the standard deviation is the Black volatility over a time of 1
        let one = forward.like(1.0);
        compute_forward_value(
            ForwardModel::Black,
            kind,
            &ForwardInputs { forward, strike: k, volatility: v, time: one, discount: one },
        )
    }
}

/// A European swaption on fixed payments: pay or receive the fixed leg from the exercise.
#[derive(Debug, Clone, PartialEq)]
pub struct SwaptionFlows {
    pub payer: bool,
    /// Exercise time.
    pub maturity: f64,
    /// Start of the first period of the underlying swap.
    pub value_time: f64,
    pub payment_times: Vec<f64>,
    /// Fixed coupons, without the notional.
    pub coupons: Vec<f64>,
    pub nominal: f64,
}

impl SwaptionFlows {
    fn amounts(&self) -> Result<Vec<f64>, Error> {
        if self.payment_times.is_empty() || self.payment_times.len() != self.coupons.len() {
            return invalid("a swaption needs as many coupons as payment times".into());
        }
        let mut amounts = self.coupons.clone();
        *amounts.last_mut().expect("not empty") += self.nominal;
        Ok(amounts)
    }
}

/// The value of a European swaption by Jamshidian's decomposition. The critical rate is found on
/// plain numbers; one Newton step on the recorded values carries its derivatives, so that the first
/// derivatives of the result are exact (implicit function theorem).
pub fn compute_jamshidian_swaption<T: Operand>(
    model: &HullWhite<T>,
    flows: &SwaptionFlows,
) -> Result<T, Error> {
    model.check()?;
    let amounts = flows.amounts()?;
    let objective_at = |x: T| -> Result<T, Error> {
        let b = model.discount_bond(flows.maturity, flows.value_time, x)?;
        let mut value = b.like(flows.nominal);
        for (&time, &amount) in flows.payment_times.iter().zip(&amounts) {
            value = value - model.discount_bond(flows.maturity, time, x)? / b * amount;
        }
        Ok(value)
    };
    let zero = model.mean_reversion.like(0.0);
    let plain = |x: f64| Ok(objective_at(zero.like(x))?.primal());
    let root = solve_brent(plain, 1e-12, 0.05, -10.0, 10.0, 10_000)?;

    let slope = {
        let b_value = model.b(flows.maturity, flows.value_time).primal();
        let b_at_root = model.discount_bond(flows.maturity, flows.value_time, zero.like(root))?.primal();
        let mut slope = 0.0;
        for (&time, &amount) in flows.payment_times.iter().zip(&amounts) {
            let p = model.discount_bond(flows.maturity, time, zero.like(root))?.primal() / b_at_root;
            slope += amount * p * (model.b(flows.maturity, time).primal() - b_value);
        }
        slope
    };
    let critical = zero.like(root) - objective_at(zero.like(root))? / slope;

    let kind = if flows.payer { OptionType::Put } else { OptionType::Call };
    let b = model.discount_bond(flows.maturity, flows.value_time, critical)?;
    let mut value = zero;
    for (&time, &amount) in flows.payment_times.iter().zip(&amounts) {
        let strike = model.discount_bond(flows.maturity, time, critical)? / b;
        let option = model.discount_bond_option(kind, strike, flows.maturity, flows.value_time, time)?;
        value = value + option * amount;
    }
    Ok(value)
}

/// A swaption value and its derivatives with respect to the model's parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwaptionSensitivity {
    pub value: f64,
    pub d_mean_reversion: f64,
    pub d_volatility: f64,
}

/// A European swaption with its derivatives by automatic differentiation.
pub fn compute_jamshidian_swaption_sensitivity(
    mean_reversion: f64,
    volatility: f64,
    curve: &YieldCurve,
    flows: &SwaptionFlows,
) -> Result<SwaptionSensitivity, Error> {
    let recorder = Recorder::new();
    let lifted = curve.lift(&recorder)?;
    let (a, sigma) = (recorder.input(mean_reversion), recorder.input(volatility));
    let model = HullWhite { mean_reversion: a, volatility: sigma, curve: &lifted.curve };
    let value = compute_jamshidian_swaption(&model, flows)?;
    let g = recorder.gradient(value, &[a, sigma])?;
    Ok(SwaptionSensitivity {
        value: value.primal(),
        d_mean_reversion: g[0].mean(),
        d_volatility: g[1].mean(),
    })
}

/// What a calibration is fitted to.
#[derive(Debug, Clone, PartialEq)]
pub enum CalibrationInstrument {
    /// A European option on a zero-coupon bond, expiring at `maturity`.
    BondOption { kind: OptionType, strike: f64, maturity: f64, bond_maturity: f64 },
    /// A European swaption by Jamshidian's decomposition.
    Swaption(SwaptionFlows),
}

/// A market price to fit. The term of the objective is `weight * (model - price)^2`.
#[derive(Debug, Clone, PartialEq)]
pub struct CalibrationQuote {
    pub instrument: CalibrationInstrument,
    pub price: f64,
    pub weight: f64,
}

/// The starting point and options of [`calibrate_hull_white`]. The default start is LagrangeFinance's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalibrationSettings {
    pub initial_mean_reversion: f64,
    pub initial_volatility: f64,
    /// Keep the mean reversion at its initial value and fit the volatility alone.
    pub fix_mean_reversion: bool,
    pub max_iterations: usize,
}

impl Default for CalibrationSettings {
    fn default() -> Self {
        CalibrationSettings {
            initial_mean_reversion: 0.05,
            initial_volatility: 0.01,
            fix_mean_reversion: false,
            max_iterations: 200,
        }
    }
}

/// Why a calibration stopped. Only [`CalibrationStop::MaxIterations`] means that no test was met.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationStop {
    /// The relative reduction of the objective fell to 1e-14 or below.
    CostChange,
    /// No free variable moved by more than 1e-10, in the logarithm of the parameter.
    SmallStep,
    /// The residuals are orthogonal to the Jacobian's columns, to a cosine of 1e-10.
    Gradient,
    /// No trial step lowered the objective, with the damping raised 30 times.
    NoFurtherReduction,
    MaxIterations,
}

impl CalibrationStop {
    pub fn name(self) -> &'static str {
        match self {
            CalibrationStop::CostChange => "cost_change",
            CalibrationStop::SmallStep => "small_step",
            CalibrationStop::Gradient => "gradient",
            CalibrationStop::NoFurtherReduction => "no_further_reduction",
            CalibrationStop::MaxIterations => "max_iterations",
        }
    }
}

/// The result of [`calibrate_hull_white`].
#[derive(Debug, Clone, PartialEq)]
pub struct HullWhiteCalibration {
    pub mean_reversion: f64,
    pub volatility: f64,
    /// The sum of `weight * (model - price)^2`.
    pub objective: f64,
    /// `sqrt(objective / number of quotes)`.
    pub root_mean_square_error: f64,
    /// The model's price of each quote, in their order.
    pub model_prices: Vec<f64>,
    /// The mean reversion ended at [`MIN_FITTED_MEAN_REVERSION`]: the data asks for no mean reversion.
    pub at_lower_limit: bool,
    pub iterations: usize,
    pub stop: CalibrationStop,
}

fn value_of_quote<T: Operand>(model: &HullWhite<T>, quote: &CalibrationQuote) -> Result<T, Error> {
    match &quote.instrument {
        CalibrationInstrument::BondOption { kind, strike, maturity, bond_maturity } => {
            let strike = model.mean_reversion.like(*strike);
            model.discount_bond_option(*kind, strike, *maturity, *maturity, *bond_maturity)
        }
        CalibrationInstrument::Swaption(flows) => compute_jamshidian_swaption(model, flows),
    }
}

/// The weighted residuals, the model's prices and the Jacobian in the free variables.
#[allow(clippy::type_complexity)]
fn calibration_residuals(
    x: &[f64],
    fixed_mean_reversion: Option<f64>,
    quotes: &[CalibrationQuote],
    curve: &YieldCurve,
) -> Result<(Vec<f64>, Vec<f64>, Vec<Vec<f64>>), Error> {
    let mut residuals = Vec::with_capacity(quotes.len());
    let mut prices = Vec::with_capacity(quotes.len());
    let mut jacobian = Vec::with_capacity(quotes.len());
    for quote in quotes {
        let recorder = Recorder::new();
        let lifted = curve.lift(&recorder)?;
        let inputs: Vec<_> = x.iter().map(|&v| recorder.input(v)).collect();
        let (a, sigma) = match fixed_mean_reversion {
            Some(a) => (recorder.constant(a), inputs[0].exp()),
            None => (inputs[0].exp(), inputs[1].exp()),
        };
        let model = HullWhite { mean_reversion: a, volatility: sigma, curve: &lifted.curve };
        let value = value_of_quote(&model, quote)?;
        let gradient = recorder.gradient(value, &inputs)?;
        let scale = quote.weight.sqrt();
        prices.push(value.primal());
        residuals.push(scale * (value.primal() - quote.price));
        jacobian.push(gradient.iter().map(|g| scale * g.mean()).collect());
    }
    Ok((residuals, prices, jacobian))
}

/// The lower limit of a fitted mean reversion: QuantLib's square root of the machine epsilon, below
/// which it takes the limit of `a = 0`. A logarithm cannot reach zero, and QuantLib's constraint excludes
/// zero too. To fit with `a = 0` exactly, hold it fixed. The objective at this limit was at most 6.7e-7
/// above its value at zero, relatively, in the tests of `docs/spec_hull_white.md`.
pub const MIN_FITTED_MEAN_REVERSION: f64 = 1.490_116_119_384_765_6e-8;

/// The largest move of `ln(a)` or `ln(sigma)` in one step. As `a` tends to zero its column of the
/// Jacobian vanishes with it and the scaled step in `ln(a)` grows without limit, which would otherwise
/// hold back the step in the volatility too.
const MAX_LOG_STEP: f64 = 2.0;

/// Fits the mean reversion and the volatility to market prices, or the volatility alone, by
/// Levenberg-Marquardt on `ln(a)` and `ln(sigma)`. The Jacobian comes from automatic differentiation.
/// The objective is the weighted sum of squared price differences, and the result depends only on the
/// inputs. `docs/spec_hull_white.md` sets out how this differs from QuantLib and LagrangeFinance.
pub fn calibrate_hull_white(
    quotes: &[CalibrationQuote],
    curve: &YieldCurve,
    settings: &CalibrationSettings,
) -> Result<HullWhiteCalibration, Error> {
    let CalibrationSettings { initial_mean_reversion, initial_volatility, fix_mean_reversion, max_iterations } = *settings;
    if !(initial_volatility > 0.0) || !initial_volatility.is_finite() {
        return invalid(format!("the initial volatility must be positive, not {initial_volatility}"));
    }
    let start_is_valid = if fix_mean_reversion { initial_mean_reversion >= 0.0 } else { initial_mean_reversion > 0.0 };
    if !start_is_valid || !initial_mean_reversion.is_finite() {
        let bound = if fix_mean_reversion { "must not be negative" } else { "must be positive" };
        return invalid(format!("the initial mean reversion {bound}, not {initial_mean_reversion}"));
    }
    let free = if fix_mean_reversion { 1 } else { 2 };
    if quotes.len() < free {
        return invalid(format!("calibration needs at least {free} quotes for {free} free parameters, not {}", quotes.len()));
    }
    if quotes.iter().any(|q| !q.price.is_finite() || !q.weight.is_finite() || !(q.weight > 0.0)) {
        return invalid("calibration needs finite prices and positive, finite weights".into());
    }
    if max_iterations == 0 {
        return invalid("calibration needs at least 1 iteration".into());
    }

    let fixed = fix_mean_reversion.then_some(initial_mean_reversion);
    let mut x: Vec<f64> = if fix_mean_reversion {
        vec![initial_volatility.ln()]
    } else {
        vec![initial_mean_reversion.ln(), initial_volatility.ln()]
    };
    let cost = |r: &[f64]| r.iter().map(|e| e * e).sum::<f64>();
    let (mut r, mut prices, mut j) = calibration_residuals(&x, fixed, quotes, curve)?;
    let log_lower_limit = MIN_FITTED_MEAN_REVERSION.ln();
    let mut lambda = 1e-3;
    let mut iterations = 0;
    let mut stop = CalibrationStop::MaxIterations;
    'outer: for _ in 0..max_iterations {
        iterations += 1;
        let mut jtj = vec![vec![0.0; free]; free];
        let mut rhs = vec![vec![0.0]; free];
        for (row, e) in j.iter().zip(&r) {
            for a in 0..free {
                rhs[a][0] -= row[a] * e;
                for b in 0..free {
                    jtj[a][b] += row[a] * row[b];
                }
            }
        }
        // At the lower limit of a, with the descent pointing below it, only the volatility moves.
        let held = fixed.is_none() && x[0] <= log_lower_limit && rhs[0][0] < 0.0;
        if held {
            rhs[0][0] = 0.0;
            jtj[0].fill(0.0);
            for row in &mut jtj {
                row[0] = 0.0;
            }
        }
        let residual_norm = cost(&r).sqrt();
        let largest_cosine = (0..free)
            .filter(|&k| jtj[k][k] > 0.0)
            .map(|k| rhs[k][0].abs() / (jtj[k][k].sqrt() * residual_norm))
            .fold(0.0, f64::max);
        if residual_norm == 0.0 || largest_cosine <= 1e-10 {
            stop = CalibrationStop::Gradient;
            break;
        }
        let scale: Vec<f64> = (0..free).map(|k| if jtj[k][k] > 0.0 { jtj[k][k].sqrt() } else { 1.0 }).collect();
        let mut accepted = false;
        for _ in 0..30 {
            // Marquardt's damping, on columns scaled to unit length so that a small column of a
            // tiny mean reversion cannot make the system look singular
            let mut scaled = vec![vec![0.0; free]; free];
            let mut scaled_rhs = vec![vec![0.0]; free];
            for a in 0..free {
                scaled_rhs[a][0] = rhs[a][0] / scale[a];
                for b in 0..free {
                    scaled[a][b] = jtj[a][b] / (scale[a] * scale[b]);
                }
                scaled[a][a] += lambda;
            }
            let Ok(step) = solve(&scaled, &scaled_rhs) else {
                lambda *= 4.0;
                continue;
            };
            let limited: Vec<f64> = step.iter().zip(&scale).map(|(s, d)| (s[0] / d).clamp(-MAX_LOG_STEP, MAX_LOG_STEP)).collect();
            let mut trial: Vec<f64> = x.iter().zip(&limited).map(|(v, s)| v + s).collect();
            if fixed.is_none() {
                trial[0] = trial[0].max(log_lower_limit);
            }
            // a failed or non-finite evaluation counts as a step that did not improve
            let evaluated = calibration_residuals(&trial, fixed, quotes, curve);
            if let Ok((rt, pt, jt)) = evaluated {
                let (before, after) = (cost(&r), cost(&rt));
                if after < before {
                    let largest_step = x.iter().zip(&trial).map(|(old, new)| (new - old).abs()).fold(0.0, f64::max);
                    x = trial;
                    r = rt;
                    prices = pt;
                    j = jt;
                    lambda = (lambda * 0.3).max(1e-12);
                    accepted = true;
                    if (before - after) / before <= 1e-14 {
                        stop = CalibrationStop::CostChange;
                        break 'outer;
                    }
                    if largest_step <= 1e-10 {
                        stop = CalibrationStop::SmallStep;
                        break 'outer;
                    }
                    break;
                }
            }
            lambda *= 4.0;
        }
        if !accepted {
            stop = CalibrationStop::NoFurtherReduction;
            break;
        }
    }
    let (mean_reversion, volatility) = match fixed {
        Some(a) => (a, x[0].exp()),
        None if x[0] <= log_lower_limit => (MIN_FITTED_MEAN_REVERSION, x[1].exp()),
        None => (x[0].exp(), x[1].exp()),
    };
    let objective = cost(&r);
    Ok(HullWhiteCalibration {
        mean_reversion,
        volatility,
        objective,
        root_mean_square_error: (objective / quotes.len() as f64).sqrt(),
        model_prices: prices,
        at_lower_limit: fixed.is_none() && x[0] <= log_lower_limit,
        iterations,
        stop,
    })
}

/// The nodes and weights of Gauss-Hermite quadrature for the standard normal distribution, in
/// increasing order. They are the roots of the normalized Hermite polynomials.
pub fn generate_normal_quadrature(n: usize) -> Result<(Vec<f64>, Vec<f64>), Error> {
    if !(2..=128).contains(&n) {
        return invalid(format!("quadrature needs 2 to 128 nodes, not {n}"));
    }
    // values of the normalized polynomials of degrees n and n - 1
    let polynomial = |x: f64| {
        let (mut previous, mut current) = (0.0, 1.0);
        for k in 0..n {
            let next = (x * current - (k as f64).sqrt() * previous) / ((k + 1) as f64).sqrt();
            previous = current;
            current = next;
        }
        (current, previous)
    };
    let limit = 2.0 * (n as f64).sqrt() + 2.0;
    let steps = 200 * n;
    let width = 2.0 * limit / steps as f64;
    let mut nodes = Vec::with_capacity(n);
    let mut left = -limit;
    let mut f_left = polynomial(left).0;
    for i in 1..=steps {
        let right = -limit + i as f64 * width;
        let f_right = polynomial(right).0;
        if f_left == 0.0 || f_left * f_right < 0.0 {
            let (mut a, mut b, mut fa) = (left, right, f_left);
            for _ in 0..200 {
                let m = 0.5 * (a + b);
                let fm = polynomial(m).0;
                if fa * fm <= 0.0 {
                    b = m;
                } else {
                    a = m;
                    fa = fm;
                }
            }
            let mut x = 0.5 * (a + b);
            for _ in 0..3 {
                let (p, q) = polynomial(x);
                x -= p / ((n as f64).sqrt() * q);
            }
            nodes.push(x);
        }
        left = right;
        f_left = f_right;
    }
    if nodes.len() != n {
        return Err(Error::NoConvergence {
            reason: format!("found {} of {n} quadrature nodes", nodes.len()),
        });
    }
    let weights = nodes
        .iter()
        .map(|&x| {
            let q = polynomial(x).1;
            1.0 / (n as f64 * q * q)
        })
        .collect();
    Ok((nodes, weights))
}

/// How the expectation over the next state is integrated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Quadrature {
    /// Gauss-Hermite with this many nodes. Exact for smooth values; the kink at an exercise
    /// boundary limits its accuracy, as `docs/spec_hull_white.md` shows.
    GaussHermite(usize),
    /// The trapezoid rule on standard normal shocks from -8 to 8, with this spacing.
    Trapezoid(f64),
}

/// The state grid and quadrature of a backward induction. The defaults are LagrangeFinance's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridSettings {
    pub points: usize,
    pub limit: f64,
    pub quadrature: Quadrature,
}

impl Default for GridSettings {
    fn default() -> Self {
        GridSettings { points: 321, limit: 0.13, quadrature: Quadrature::GaussHermite(24) }
    }
}

/// Nodes and weights for the standard normal distribution.
fn quadrature_rule(rule: Quadrature) -> Result<(Vec<f64>, Vec<f64>), Error> {
    match rule {
        Quadrature::GaussHermite(n) => generate_normal_quadrature(n),
        Quadrature::Trapezoid(spacing) => {
            if !(spacing > 0.0 && spacing <= 1.0) {
                return invalid(format!("the trapezoid spacing must be in (0, 1], not {spacing}"));
            }
            let n = (8.0 / spacing).round() as i64;
            let nodes: Vec<f64> = (-n..=n).map(|i| i as f64 * spacing).collect();
            let weights = nodes
                .iter()
                .map(|&z| spacing * crate::math::normal::compute_normal_pdf(z))
                .collect();
            Ok((nodes, weights))
        }
    }
}

struct Grid<'c> {
    states: Vec<f64>,
    nodes: Vec<f64>,
    weights: Vec<f64>,
    a: f64,
    sigma: f64,
    curve: &'c YieldCurve,
}

/// Linear in the state, flat outside the grid (R's `approx` with `rule = 2`).
fn interpolate(states: &[f64], values: &[f64], x: f64) -> f64 {
    let n = states.len();
    if x <= states[0] {
        return values[0];
    }
    if x >= states[n - 1] {
        return values[n - 1];
    }
    let j = states.partition_point(|&s| s <= x) - 1;
    let w = (x - states[j]) / (states[j + 1] - states[j]);
    values[j] + w * (values[j + 1] - values[j])
}

impl Grid<'_> {
    fn new<'c>(
        a: f64,
        sigma: f64,
        curve: &'c YieldCurve,
        settings: &GridSettings,
    ) -> Result<Grid<'c>, Error> {
        if !(a > 0.0) || !(sigma >= 0.0) {
            return invalid(format!("the grid needs a > 0 and volatility >= 0 ({a}, {sigma})"));
        }
        if settings.points < 3 || !(settings.limit > 0.0) {
            return invalid("the grid needs at least 3 points and a positive limit".into());
        }
        let (nodes, weights) = quadrature_rule(settings.quadrature)?;
        let n = settings.points;
        let states = (0..n)
            .map(|i| -settings.limit + 2.0 * settings.limit * i as f64 / (n - 1) as f64)
            .collect();
        Ok(Grid { states, nodes, weights, a, sigma, curve })
    }

    /// `E[exp(-integral of r from t1 to t2) V(x at t2) | x at t1 = x]` at each of `from`. The
    /// integral of the shift is exact for any initial curve.
    fn step(&self, next: &[f64], from: &[f64], t1: f64, t2: f64) -> Result<Vec<f64>, Error> {
        let (a, sigma) = (self.a, self.sigma);
        let h = t2 - t1;
        let b = (-a * h).exp();
        let big_b = -(-a * h).exp_m1() / a;
        let shift = -(self.curve.discount_at(t2, true)? / self.curve.discount_at(t1, true)?).ln()
            + sigma * sigma / (2.0 * a * a)
                * (h - 2.0 * ((-a * t1).exp() - (-a * t2).exp()) / a
                    + ((-2.0 * a * t1).exp() - (-2.0 * a * t2).exp()) / (2.0 * a));
        if sigma == 0.0 {
            return Ok(from
                .iter()
                .map(|&x| (-shift - big_b * x).exp() * interpolate(&self.states, next, b * x))
                .collect());
        }
        let m2 = -(-2.0 * a * h).exp_m1();
        let variance_next = sigma * sigma * m2 / (2.0 * a);
        let variance_integral = sigma * sigma / (a * a) * (h - 2.0 * big_b + m2 / (2.0 * a));
        let e1 = -(-a * h).exp_m1();
        let covariance = sigma * sigma * e1 * e1 / (2.0 * a * a);
        let residual = (variance_integral - covariance * covariance / variance_next).max(0.0);
        let sd = variance_next.sqrt();
        let conditional: Vec<f64> = self
            .nodes
            .iter()
            .zip(&self.weights)
            .map(|(&z, &w)| w * (-covariance / sd * z).exp())
            .collect();
        Ok(from
            .iter()
            .map(|&x| {
                let mean: f64 = self
                    .nodes
                    .iter()
                    .zip(&conditional)
                    .map(|(&z, &w)| w * interpolate(&self.states, next, b * x + sd * z))
                    .sum();
                (-shift - big_b * x + residual / 2.0).exp() * mean
            })
            .collect())
    }
}

/// One payment date of a callable bond.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CallableFlow {
    pub time: f64,
    pub coupon: f64,
    pub principal: f64,
    /// The price the issuer can pay at this date, after the coupon.
    pub call_price: Option<f64>,
}

/// The value of a bond with and without the issuer's call.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CallableBondValue {
    pub bullet: f64,
    pub callable: f64,
    pub issuer_call_value: f64,
}

/// A bond callable by the issuer on its coupon dates, by backward induction (LagrangeFinance II-8.4).
/// No default, spread or cost is included.
pub fn compute_callable_bond(
    flows: &[CallableFlow],
    mean_reversion: f64,
    volatility: f64,
    curve: &YieldCurve,
    settings: &GridSettings,
) -> Result<CallableBondValue, Error> {
    if flows.len() < 2 {
        return invalid("a callable bond needs at least 2 payments".into());
    }
    if flows.windows(2).any(|w| w[1].time <= w[0].time) || flows[0].time <= 0.0 {
        return invalid("payment times must be positive and increasing".into());
    }
    let grid = Grid::new(mean_reversion, volatility, curve, settings)?;
    let last = flows[flows.len() - 1];
    let mut bullet = vec![last.coupon + last.principal; grid.states.len()];
    let mut callable = bullet.clone();
    for i in (0..flows.len() - 1).rev() {
        let (now, next_time) = (flows[i], flows[i + 1].time);
        let continuation_bullet = grid.step(&bullet, &grid.states, now.time, next_time)?;
        let continuation = grid.step(&callable, &grid.states, now.time, next_time)?;
        bullet = continuation_bullet.iter().map(|v| now.coupon + v).collect();
        callable = continuation
            .iter()
            .map(|&v| now.coupon + now.call_price.map_or(v, |c| v.min(c)))
            .collect();
    }
    let first = flows[0].time;
    let bullet = grid.step(&bullet, &[0.0], 0.0, first)?[0];
    let callable = grid.step(&callable, &[0.0], 0.0, first)?[0];
    Ok(CallableBondValue { bullet, callable, issuer_call_value: bullet - callable })
}

/// A Bermudan swaption, exercisable into the remaining payments on each of `exercise_times`, which
/// must be among the period starts. The value comes from the same backward induction as
/// [`callable_bond`].
pub fn compute_bermudan_swaption(
    flows: &SwaptionFlows,
    exercise_times: &[f64],
    mean_reversion: f64,
    volatility: f64,
    curve: &YieldCurve,
    settings: &GridSettings,
) -> Result<f64, Error> {
    let amounts = flows.amounts()?;
    if exercise_times.is_empty() || exercise_times.windows(2).any(|w| w[1] <= w[0]) || exercise_times[0] <= 0.0 {
        return invalid("exercise times must be positive and increasing".into());
    }
    let grid = Grid::new(mean_reversion, volatility, curve, settings)?;
    let sign = if flows.payer { 1.0 } else { -1.0 };
    let mut times: Vec<f64> = flows.payment_times.iter().chain(exercise_times).copied().collect();
    times.sort_by(f64::total_cmp);
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    let is_exercise = |t: f64| exercise_times.iter().any(|e| (e - t).abs() < 1e-12);
    let amount_at = |t: f64| {
        flows
            .payment_times
            .iter()
            .position(|p| (p - t).abs() < 1e-12)
            .map_or(0.0, |i| amounts[i])
    };

    let n = grid.states.len();
    let last = times.len() - 1;
    // the bond's remaining value, excluding any payment at the current time
    let mut bond = vec![0.0; n];
    let mut option = vec![0.0; n];
    let mut time = times[last];
    let mut paid = amount_at(time);
    for i in (0..last).rev() {
        let now = times[i];
        let with_payment: Vec<f64> = bond.iter().map(|b| b + paid).collect();
        bond = grid.step(&with_payment, &grid.states, now, time)?;
        option = grid.step(&option, &grid.states, now, time)?;
        if is_exercise(now) {
            option = option
                .iter()
                .zip(&bond)
                .map(|(&held, &b)| held.max(sign * (flows.nominal - b)))
                .collect();
        }
        time = now;
        paid = amount_at(now);
    }
    Ok(grid.step(&option, &[0.0], 0.0, time)?[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interest_rate::InterestRate;
    use crate::time::{Actual365Kind, Date, DayCount};

    fn flat(rate: f64) -> YieldCurve {
        let dc = DayCount::Actual365Fixed(Actual365Kind::Standard);
        YieldCurve::flat(
            Date::from_ymd(2026, 11, 2).unwrap(),
            InterestRate::new(rate, dc, Compounding::Continuous, 0).unwrap(),
        )
    }

    #[test]
    fn quadrature_integrates_the_moments_of_the_normal() {
        let (x, w) = generate_normal_quadrature(24).unwrap();
        assert_eq!(x.len(), 24);
        let moment = |p: i32| x.iter().zip(&w).map(|(x, w)| w * x.powi(p)).sum::<f64>();
        assert!((moment(0) - 1.0).abs() < 1e-13);
        assert!(moment(1).abs() < 1e-13);
        assert!((moment(2) - 1.0).abs() < 1e-12);
        assert!((moment(4) - 3.0).abs() < 1e-11);
        assert!((moment(6) - 15.0).abs() < 1e-10);
        assert!(generate_normal_quadrature(1).is_err());
    }

    #[test]
    fn bond_prices_reproduce_the_curve_in_expectation_of_the_grid() {
        // with no call, the grid must return the curve's price of the bond
        let curve = flat(0.02);
        let flows: Vec<CallableFlow> = (1..=10)
            .map(|i| CallableFlow {
                time: i as f64 * 0.5,
                coupon: 1.5,
                principal: if i == 10 { 100.0 } else { 0.0 },
                call_price: None,
            })
            .collect();
        let v = compute_callable_bond(&flows, 0.15, 0.012, &curve, &GridSettings::default()).unwrap();
        let exact: f64 = flows
            .iter()
            .map(|f| (f.coupon + f.principal) * (-0.02 * f.time).exp())
            .sum();
        assert!((v.bullet - exact).abs() < 0.02, "{} {exact}", v.bullet);
        assert!((v.callable - v.bullet).abs() < 1e-10);
    }

    #[test]
    fn a_call_lowers_the_value() {
        let curve = flat(0.02);
        let flows: Vec<CallableFlow> = (1..=10)
            .map(|i| CallableFlow {
                time: i as f64 * 0.5,
                coupon: 1.5,
                principal: if i == 10 { 100.0 } else { 0.0 },
                call_price: if (2..10).contains(&i) { Some(100.0) } else { None },
            })
            .collect();
        let v = compute_callable_bond(&flows, 0.15, 0.012, &curve, &GridSettings::default()).unwrap();
        assert!(v.issuer_call_value > 0.0);
    }

    fn swaption(payer: bool) -> SwaptionFlows {
        SwaptionFlows {
            payer,
            maturity: 2.0,
            value_time: 2.0,
            payment_times: vec![3.0, 4.0, 5.0, 6.0, 7.0],
            coupons: vec![0.025; 5],
            nominal: 1.0,
        }
    }

    #[test]
    fn jamshidian_agrees_with_the_grid_for_one_exercise() {
        let curve = flat(0.02);
        for payer in [true, false] {
            let f = swaption(payer);
            let model = HullWhite { mean_reversion: 0.1, volatility: 0.01, curve: &curve };
            let analytic = compute_jamshidian_swaption(&model, &f).unwrap();
            let settings = GridSettings { points: 1201, limit: 0.2, quadrature: Quadrature::Trapezoid(0.02) };
            let grid = compute_bermudan_swaption(&f, &[2.0], 0.1, 0.01, &curve, &settings).unwrap();
            assert!((analytic - grid).abs() < 1e-5, "{analytic} {grid}");
        }
    }

    #[test]
    fn payer_receiver_parity() {
        // payer - receiver = the forward swap's value on the curve
        let curve = flat(0.02);
        let model = HullWhite { mean_reversion: 0.1, volatility: 0.01, curve: &curve };
        let payer = compute_jamshidian_swaption(&model, &swaption(true)).unwrap();
        let receiver = compute_jamshidian_swaption(&model, &swaption(false)).unwrap();
        let f = swaption(true);
        let fixed: f64 = f.payment_times.iter().zip(&f.coupons).map(|(t, c)| c * (-0.02 * t).exp()).sum();
        let swap = (-0.02f64 * 2.0).exp() - (-0.02f64 * 7.0).exp() - fixed;
        assert!((payer - receiver - swap).abs() < 1e-12, "{}", payer - receiver - swap);
    }

    #[test]
    fn bermudan_is_at_least_the_european() {
        let curve = flat(0.02);
        let f = swaption(true);
        let settings = GridSettings { points: 641, limit: 0.2, quadrature: Quadrature::Trapezoid(0.05) };
        let model = HullWhite { mean_reversion: 0.1, volatility: 0.01, curve: &curve };
        let european = compute_jamshidian_swaption(&model, &f).unwrap();
        let bermudan = compute_bermudan_swaption(&f, &[2.0, 3.0, 4.0, 5.0, 6.0], 0.1, 0.01, &curve, &settings).unwrap();
        assert!(bermudan >= european - 1e-5, "{bermudan} {european}");
    }

    #[test]
    fn swaption_derivatives_agree_with_numeric_differences() {
        let curve = flat(0.02);
        let f = swaption(true);
        let s = compute_jamshidian_swaption_sensitivity(0.1, 0.01, &curve, &f).unwrap();
        let value = |a: f64, v: f64| {
            compute_jamshidian_swaption(&HullWhite { mean_reversion: a, volatility: v, curve: &curve }, &f).unwrap()
        };
        assert!((s.value - value(0.1, 0.01)).abs() < 1e-14);
        let h = 1e-6;
        let d_a = (value(0.1 + h, 0.01) - value(0.1 - h, 0.01)) / (2.0 * h);
        let d_v = (value(0.1, 0.01 + h) - value(0.1, 0.01 - h)) / (2.0 * h);
        assert!((s.d_mean_reversion - d_a).abs() < 1e-8, "{} {d_a}", s.d_mean_reversion);
        assert!((s.d_volatility - d_v).abs() < 1e-7, "{} {d_v}", s.d_volatility);
    }

    #[test]
    fn invalid_parameters_are_errors() {
        let curve = flat(0.02);
        let model = HullWhite { mean_reversion: -0.1, volatility: 0.01, curve: &curve };
        assert!(compute_jamshidian_swaption(&model, &swaption(true)).is_err());
        assert!(compute_callable_bond(&[], 0.1, 0.01, &curve, &GridSettings::default()).is_err());
    }

    fn bond_option_quotes(a: f64, sigma: f64, curve: &YieldCurve) -> Vec<CalibrationQuote> {
        let model = HullWhite { mean_reversion: a, volatility: sigma, curve };
        [(0.5, 2.0, 0.95), (1.0, 3.0, 0.94), (2.0, 5.0, 0.92), (3.0, 7.0, 0.89)]
            .iter()
            .map(|&(maturity, bond_maturity, strike)| CalibrationQuote {
                instrument: CalibrationInstrument::BondOption { kind: OptionType::Call, strike, maturity, bond_maturity },
                price: model.discount_bond_option(OptionType::Call, strike, maturity, maturity, bond_maturity).unwrap(),
                weight: 1.0,
            })
            .collect()
    }

    #[test]
    fn calibration_recovers_the_parameters_of_its_own_prices() {
        let curve = flat(0.03);
        let quotes = bond_option_quotes(0.08, 0.012, &curve);
        let fit = calibrate_hull_white(&quotes, &curve, &CalibrationSettings::default()).unwrap();
        assert!((fit.mean_reversion - 0.08).abs() < 1e-9, "{}", fit.mean_reversion);
        assert!((fit.volatility - 0.012).abs() < 1e-10, "{}", fit.volatility);
        assert!(fit.root_mean_square_error < 1e-13, "{}", fit.root_mean_square_error);
        assert_ne!(fit.stop, CalibrationStop::MaxIterations);
        // the same inputs give the same result
        assert_eq!(fit, calibrate_hull_white(&quotes, &curve, &CalibrationSettings::default()).unwrap());
    }

    #[test]
    fn calibration_recovers_from_swaptions_and_from_other_starts() {
        let curve = flat(0.02);
        let model = HullWhite { mean_reversion: 0.1, volatility: 0.01, curve: &curve };
        let quotes: Vec<CalibrationQuote> = (1..=3)
            .flat_map(|start| [3, 6].map(|years| (start, years)))
            .map(|(start, years)| {
                let flows = SwaptionFlows {
                    payer: start % 2 == 0,
                    maturity: start as f64,
                    value_time: start as f64,
                    payment_times: (1..=years).map(|i| (start + i) as f64).collect(),
                    coupons: vec![0.02; years],
                    nominal: 1.0,
                };
                let price = compute_jamshidian_swaption(&model, &flows).unwrap();
                CalibrationQuote { instrument: CalibrationInstrument::Swaption(flows), price, weight: 1.0 }
            })
            .collect();
        for (a, sigma) in [(0.05, 0.01), (0.5, 0.03), (0.005, 0.002)] {
            let settings = CalibrationSettings { initial_mean_reversion: a, initial_volatility: sigma, ..Default::default() };
            let fit = calibrate_hull_white(&quotes, &curve, &settings).unwrap();
            assert!((fit.mean_reversion - 0.1).abs() < 1e-8, "{a} {sigma} {}", fit.mean_reversion);
            assert!((fit.volatility - 0.01).abs() < 1e-9, "{a} {sigma} {}", fit.volatility);
        }
    }

    #[test]
    fn calibration_holds_a_fixed_mean_reversion() {
        let curve = flat(0.03);
        let quotes = bond_option_quotes(0.08, 0.012, &curve);
        let settings = CalibrationSettings { initial_mean_reversion: 0.08, fix_mean_reversion: true, ..Default::default() };
        let fit = calibrate_hull_white(&quotes[..1], &curve, &settings).unwrap();
        assert_eq!(fit.mean_reversion, 0.08);
        assert!((fit.volatility - 0.012).abs() < 1e-10, "{}", fit.volatility);
    }

    #[test]
    fn a_common_weight_scales_the_objective_and_keeps_the_result() {
        let curve = flat(0.03);
        let mut quotes = bond_option_quotes(0.08, 0.012, &curve);
        quotes[0].price += 1e-4;
        let plain = calibrate_hull_white(&quotes, &curve, &CalibrationSettings::default()).unwrap();
        for q in &mut quotes {
            q.weight = 1e8;
        }
        let scaled = calibrate_hull_white(&quotes, &curve, &CalibrationSettings::default()).unwrap();
        assert!((scaled.objective / plain.objective / 1e8 - 1.0).abs() < 1e-8, "{}", scaled.objective / plain.objective);
        assert!((scaled.mean_reversion - plain.mean_reversion).abs() < 1e-9);
        assert!((scaled.volatility - plain.volatility).abs() < 1e-10);
    }

    #[test]
    fn the_objective_is_stationary_at_the_result() {
        // noisy prices: moving either parameter away from the result must not lower the objective
        let curve = flat(0.03);
        let mut quotes = bond_option_quotes(0.08, 0.012, &curve);
        quotes[1].price += 2e-4;
        quotes[3].price -= 1e-4;
        let fit = calibrate_hull_white(&quotes, &curve, &CalibrationSettings::default()).unwrap();
        let objective = |a: f64, sigma: f64| {
            let model = HullWhite { mean_reversion: a, volatility: sigma, curve: &curve };
            quotes.iter().map(|q| (value_of_quote(&model, q).unwrap() - q.price).powi(2)).sum::<f64>()
        };
        assert!((objective(fit.mean_reversion, fit.volatility) - fit.objective).abs() < 1e-18);
        for (da, ds) in [(1e-4, 0.0), (-1e-4, 0.0), (0.0, 1e-5), (0.0, -1e-5)] {
            assert!(objective(fit.mean_reversion + da, fit.volatility + ds) >= fit.objective);
        }
    }

    #[test]
    fn calibration_rejects_invalid_inputs() {
        let curve = flat(0.03);
        let quotes = bond_option_quotes(0.08, 0.012, &curve);
        let run = |q: &[CalibrationQuote], s: CalibrationSettings| calibrate_hull_white(q, &curve, &s);
        let base = CalibrationSettings::default();
        assert!(run(&quotes[..1], base).is_err());
        assert!(run(&quotes, CalibrationSettings { initial_mean_reversion: 0.0, ..base }).is_err());
        assert!(run(&quotes, CalibrationSettings { initial_volatility: -0.01, ..base }).is_err());
        assert!(run(&quotes, CalibrationSettings { max_iterations: 0, ..base }).is_err());
        let mut bad = quotes.clone();
        bad[0].weight = 0.0;
        assert!(run(&bad, base).is_err());
        bad = quotes.clone();
        bad[0].price = f64::NAN;
        assert!(run(&bad, base).is_err());
        // a mean reversion of zero is allowed when it is held fixed
        let fixed = CalibrationSettings { initial_mean_reversion: 0.0, fix_mean_reversion: true, ..base };
        assert_eq!(run(&quotes, fixed).unwrap().mean_reversion, 0.0);
    }

    #[test]
    fn prices_are_smooth_as_the_mean_reversion_tends_to_zero() {
        // QuantLib's form loses all its digits here: its value at a = 1e-7 can be off by more than the value
        let curve = flat(0.03);
        let f = swaption(true);
        let at = |a: f64| compute_jamshidian_swaption(&HullWhite { mean_reversion: a, volatility: 0.01, curve: &curve }, &f).unwrap();
        let limit = at(0.0);
        let slope = (at(1e-6) - limit) / 1e-6;
        for a in [1e-12, 1e-10, 1e-8, 1e-7, 1e-6, 1e-5, 9.9e-5, 1.01e-4, 1e-3] {
            let expected = limit + slope * a;
            assert!((at(a) - expected).abs() <= 1e-11 + (a * a), "{a} {} {expected}", at(a));
        }
        // the zero-coupon limit, against the closed form of a = 0
        let model = HullWhite { mean_reversion: 0.0, volatility: 0.01, curve: &curve };
        let direct = model.discount_bond_option(OptionType::Call, 0.9, 2.0, 2.0, 5.0).unwrap();
        let v: f64 = 0.01 * 3.0 * 2.0f64.sqrt();
        let inputs = ForwardInputs { forward: (-0.15f64).exp(), strike: (-0.06f64).exp() * 0.9, volatility: v, time: 1.0, discount: 1.0 };
        assert!((direct - compute_forward_value(ForwardModel::Black, OptionType::Call, &inputs).unwrap()).abs() < 1e-15);
    }

    #[test]
    fn calibration_stops_at_the_lower_limit_of_the_mean_reversion() {
        // prices that a negative mean reversion would fit best: long options priced above a = 0
        let curve = flat(0.03);
        let mut quotes = bond_option_quotes(0.0, 0.012, &curve);
        for (q, bump) in quotes.iter_mut().zip([0.0, 1e-4, 3e-4, 6e-4]) {
            q.price += bump;
        }
        let fit = calibrate_hull_white(&quotes, &curve, &CalibrationSettings::default()).unwrap();
        assert!(fit.at_lower_limit, "{}", fit.mean_reversion);
        assert_eq!(fit.mean_reversion, MIN_FITTED_MEAN_REVERSION);
        assert_ne!(fit.stop, CalibrationStop::MaxIterations);
        // the volatility is still fitted: moving it does not lower the objective
        let objective = |sigma: f64| {
            let model = HullWhite { mean_reversion: fit.mean_reversion, volatility: sigma, curve: &curve };
            quotes.iter().map(|q| (value_of_quote(&model, q).unwrap() - q.price).powi(2)).sum::<f64>()
        };
        assert!(objective(fit.volatility * 1.001) >= fit.objective);
        assert!(objective(fit.volatility * 0.999) >= fit.objective);
    }

    #[test]
    fn calibration_reports_when_it_runs_out_of_iterations() {
        let curve = flat(0.03);
        let quotes = bond_option_quotes(0.08, 0.012, &curve);
        let settings = CalibrationSettings { max_iterations: 1, ..Default::default() };
        let fit = calibrate_hull_white(&quotes, &curve, &settings).unwrap();
        assert_eq!(fit.iterations, 1);
        assert_eq!(fit.stop, CalibrationStop::MaxIterations);
    }
}
