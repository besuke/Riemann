//! The Hull-White one-factor model fitted to a yield curve: zero-coupon bonds and options on them,
//! European swaptions by Jamshidian's decomposition, and callable bonds and Bermudan swaptions on a
//! state grid.
//!
//! Closed forms follow QuantLib v1.42 `ql/models/shortrate/onefactormodels/hullwhite.cpp` (Copyright
//! (C) 2001, 2002, 2003 Sadruddin Rejeb; (C) 2006 Chiara Fornarola; (C) 2007 StatPro Italia srl) and
//! `ql/pricingengines/swaption/jamshidianswaptionengine.cpp` (Copyright (C) 2001, 2002, 2003 Sadruddin
//! Rejeb; (C) 2013 Peter Caspers); modified BSD license. See THIRD_PARTY_NOTICES.
//!
//! The grid follows LagrangeFinance II-8.4: the state and the discount over a step are jointly
//! normal, so each step is an exact conditional expectation, computed by Gauss-Hermite quadrature
//! and linear interpolation of the next values.

// `!(x > 0.0)` is intended: it also rejects NaN
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::autodiff::{Operand, Recorder};
use crate::curves::YieldCurve;
use crate::interest_rate::Compounding;
use crate::math::solver1d::brent;
use crate::options::{forward_value, ForwardInputs, ForwardModel, OptionType};
use crate::Error;

fn invalid<T>(reason: String) -> Result<T, Error> {
    Err(Error::InvalidInput { reason })
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

    /// `(1 - exp(-a (big_t - t))) / a`, and `big_t - t` when the mean reversion is too small.
    pub fn b(&self, t: f64, big_t: f64) -> T {
        let a = self.mean_reversion;
        if a.primal() < f64::EPSILON.sqrt() {
            a.like(big_t - t)
        } else {
            (-(a * (big_t - t)).exp().recip() + 1.0) / a
        }
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
        let v = if a.primal() < f64::EPSILON.sqrt() {
            sigma * self.b(bond_start, bond_maturity) * maturity.sqrt()
        } else {
            let e = |x: T| x.exp();
            let c = e(a * (-2.0) * (bond_start - maturity))
                - e(a * (-2.0) * bond_start)
                - (e(-a * (bond_start + bond_maturity - 2.0 * maturity))
                    - e(-a * (bond_start + bond_maturity)))
                    * 2.0
                + e(a * (-2.0) * (bond_maturity - maturity))
                - e(a * (-2.0) * bond_maturity);
            let floored = c.max(c.like(0.0));
            sigma / (a * (a * 2.0).sqrt()) * floored.sqrt()
        };
        let forward = self.discount(bond_maturity)?;
        let k = self.discount(bond_start)? * strike;
        // the standard deviation is the Black volatility over a time of 1
        let one = forward.like(1.0);
        forward_value(
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
pub fn jamshidian_swaption<T: Operand>(
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
    let root = brent(plain, 1e-12, 0.05, -10.0, 10.0, 10_000)?;

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
pub fn jamshidian_swaption_sensitivity(
    mean_reversion: f64,
    volatility: f64,
    curve: &YieldCurve,
    flows: &SwaptionFlows,
) -> Result<SwaptionSensitivity, Error> {
    let recorder = Recorder::new();
    let lifted = curve.lift(&recorder)?;
    let (a, sigma) = (recorder.input(mean_reversion), recorder.input(volatility));
    let model = HullWhite { mean_reversion: a, volatility: sigma, curve: &lifted.curve };
    let value = jamshidian_swaption(&model, flows)?;
    let g = recorder.gradient(value, &[a, sigma])?;
    Ok(SwaptionSensitivity {
        value: value.primal(),
        d_mean_reversion: g[0].mean(),
        d_volatility: g[1].mean(),
    })
}

/// The nodes and weights of Gauss-Hermite quadrature for the standard normal distribution, in
/// increasing order. They are the roots of the normalized Hermite polynomials.
pub fn normal_quadrature(n: usize) -> Result<(Vec<f64>, Vec<f64>), Error> {
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
        Quadrature::GaussHermite(n) => normal_quadrature(n),
        Quadrature::Trapezoid(spacing) => {
            if !(spacing > 0.0 && spacing <= 1.0) {
                return invalid(format!("the trapezoid spacing must be in (0, 1], not {spacing}"));
            }
            let n = (8.0 / spacing).round() as i64;
            let nodes: Vec<f64> = (-n..=n).map(|i| i as f64 * spacing).collect();
            let weights = nodes
                .iter()
                .map(|&z| spacing * crate::math::normal::normal_pdf(z))
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
pub fn callable_bond(
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
pub fn bermudan_swaption(
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
        let (x, w) = normal_quadrature(24).unwrap();
        assert_eq!(x.len(), 24);
        let moment = |p: i32| x.iter().zip(&w).map(|(x, w)| w * x.powi(p)).sum::<f64>();
        assert!((moment(0) - 1.0).abs() < 1e-13);
        assert!(moment(1).abs() < 1e-13);
        assert!((moment(2) - 1.0).abs() < 1e-12);
        assert!((moment(4) - 3.0).abs() < 1e-11);
        assert!((moment(6) - 15.0).abs() < 1e-10);
        assert!(normal_quadrature(1).is_err());
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
        let v = callable_bond(&flows, 0.15, 0.012, &curve, &GridSettings::default()).unwrap();
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
        let v = callable_bond(&flows, 0.15, 0.012, &curve, &GridSettings::default()).unwrap();
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
            let analytic = jamshidian_swaption(&model, &f).unwrap();
            let settings = GridSettings { points: 1201, limit: 0.2, quadrature: Quadrature::Trapezoid(0.02) };
            let grid = bermudan_swaption(&f, &[2.0], 0.1, 0.01, &curve, &settings).unwrap();
            assert!((analytic - grid).abs() < 1e-5, "{analytic} {grid}");
        }
    }

    #[test]
    fn payer_receiver_parity() {
        // payer - receiver = the forward swap's value on the curve
        let curve = flat(0.02);
        let model = HullWhite { mean_reversion: 0.1, volatility: 0.01, curve: &curve };
        let payer = jamshidian_swaption(&model, &swaption(true)).unwrap();
        let receiver = jamshidian_swaption(&model, &swaption(false)).unwrap();
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
        let european = jamshidian_swaption(&model, &f).unwrap();
        let bermudan = bermudan_swaption(&f, &[2.0, 3.0, 4.0, 5.0, 6.0], 0.1, 0.01, &curve, &settings).unwrap();
        assert!(bermudan >= european - 1e-5, "{bermudan} {european}");
    }

    #[test]
    fn swaption_derivatives_agree_with_numeric_differences() {
        let curve = flat(0.02);
        let f = swaption(true);
        let s = jamshidian_swaption_sensitivity(0.1, 0.01, &curve, &f).unwrap();
        let value = |a: f64, v: f64| {
            jamshidian_swaption(&HullWhite { mean_reversion: a, volatility: v, curve: &curve }, &f).unwrap()
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
        assert!(jamshidian_swaption(&model, &swaption(true)).is_err());
        assert!(callable_bond(&[], 0.1, 0.01, &curve, &GridSettings::default()).is_err());
    }
}
