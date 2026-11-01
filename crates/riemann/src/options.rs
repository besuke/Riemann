//! Equity options under Black-Scholes-Merton, and options on a forward under Black and Bachelier.
//! Every price is a function generic over [`Operand`], so its sensitivities come from automatic
//! differentiation and need no separate formulas.
//!
//! The values follow QuantLib v1.42 (modified BSD license; see THIRD_PARTY_NOTICES):
//! `ql/pricingengines/vanilla/analyticeuropeanengine.cpp` and `ql/pricingengines/blackcalculator.cpp`
//! (European; the formula itself is standard), `ql/pricingengines/blackformula.cpp` (Black and
//! Bachelier), `ql/pricingengines/barrier/analyticbarrierengine.cpp` (Copyright (C) 2000, 2001, 2002,
//! 2003 RiskMap srl; (C) 2002, 2003 Ferdinando Ametrano; (C) 2002, 2003 Sadruddin Rejeb; (C) 2003
//! Neil Firth; (C) 2007 StatPro Italia srl), `ql/pricingengines/vanilla/binomialengine.hpp`
//! (Copyright (C) 2002, 2003, 2004 Ferdinando Ametrano; (C) 2002, 2003 RiskMap srl; (C) 2003, 2004,
//! 2005, 2007 StatPro Italia srl; (C) 2007 Affine Group Limited),
//! `ql/methods/lattices/binomialtree.cpp` (Copyright (C) 2001, 2002, 2003 Sadruddin Rejeb; (C) 2003
//! Ferdinando Ametrano; (C) 2005 StatPro Italia srl) and `ql/methods/lattices/bsmlattice.hpp`
//! (Copyright (C) 2001, 2002, 2003 Sadruddin Rejeb; (C) 2005 StatPro Italia srl).
//!
//! Differences from QuantLib: inputs are numbers (flat continuous rates, one volatility and a time
//! in years), not term structures and dates. A volatility or time of 0 is an error here.

// `!(x > 0.0)` is intended: it also rejects NaN
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::autodiff::{Operand, Recorder, Value, Var};
use crate::batch::map_parallel;
use crate::math::random::standard_normal;
use crate::math::solver1d::brent;
use crate::Error;
use rayon::prelude::*;

/// Call or put.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OptionType {
    Call,
    Put,
}

impl OptionType {
    fn sign(self) -> f64 {
        match self {
            OptionType::Call => 1.0,
            OptionType::Put => -1.0,
        }
    }
}

/// The inputs of an equity option: flat continuous rates and time in years.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OptionInputs<T = f64> {
    pub spot: T,
    pub strike: T,
    pub rate: T,
    pub dividend_yield: T,
    pub volatility: T,
    pub time: T,
}

/// A value and its sensitivities. `theta` is the derivative with respect to the passage of calendar
/// time, minus the derivative with respect to `time`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Greeks {
    pub value: f64,
    pub delta: f64,
    pub gamma: f64,
    pub vega: f64,
    pub rho: f64,
    pub dividend_rho: f64,
    pub theta: f64,
    pub strike_sensitivity: f64,
}

fn invalid<T>(reason: String) -> Result<T, Error> {
    Err(Error::InvalidInput { reason })
}

impl<T: Operand> OptionInputs<T> {
    fn check(&self) -> Result<(), Error> {
        for (name, x) in [
            ("spot", self.spot),
            ("strike", self.strike),
            ("volatility", self.volatility),
            ("time", self.time),
        ] {
            if !(x.primal() > 0.0) {
                return invalid(format!("{name} must be positive, not {}", x.primal()));
            }
        }
        Ok(())
    }
}

impl OptionInputs<f64> {
    fn lift<'t>(&self, recorder: &'t Recorder) -> OptionInputs<Var<'t>> {
        OptionInputs {
            spot: recorder.input(self.spot),
            strike: recorder.input(self.strike),
            rate: recorder.input(self.rate),
            dividend_yield: recorder.input(self.dividend_yield),
            volatility: recorder.input(self.volatility),
            time: recorder.input(self.time),
        }
    }
}

/// The price of a European option (Black-Scholes-Merton).
pub fn european_value<T: Operand>(kind: OptionType, inputs: &OptionInputs<T>) -> Result<T, Error> {
    inputs.check()?;
    let OptionInputs { spot, strike, rate, dividend_yield, volatility, time } = *inputs;
    let discount = (-rate * time).exp();
    let forward = spot * (-dividend_yield * time).exp() / discount;
    Ok(black_undiscounted(kind, forward, strike, volatility * time.sqrt()) * discount)
}

fn black_undiscounted<T: Operand>(kind: OptionType, forward: T, strike: T, std_dev: T) -> T {
    let d1 = (forward / strike).ln() / std_dev + std_dev * 0.5;
    let d2 = d1 - std_dev;
    let phi = kind.sign();
    ((forward * (d1 * phi).normal_cdf()) - (strike * (d2 * phi).normal_cdf())) * phi
}

/// The inputs of an option on a forward. `volatility` is lognormal for Black and absolute for
/// Bachelier.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForwardInputs<T = f64> {
    pub forward: T,
    pub strike: T,
    pub volatility: T,
    pub time: T,
    pub discount: T,
}

/// The model of an option on a forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ForwardModel {
    /// Black 1976 (QuantLib `blackFormula`).
    Black,
    /// Normal model (QuantLib `bachelierBlackFormula`).
    Bachelier,
}

/// The price of an option on a forward.
pub fn forward_value<T: Operand>(
    model: ForwardModel,
    kind: OptionType,
    inputs: &ForwardInputs<T>,
) -> Result<T, Error> {
    let ForwardInputs { forward, strike, volatility, time, discount } = *inputs;
    if !(volatility.primal() > 0.0 && time.primal() > 0.0) {
        return invalid("volatility and time must be positive".into());
    }
    let std_dev = volatility * time.sqrt();
    match model {
        ForwardModel::Black => {
            if !(forward.primal() > 0.0 && strike.primal() > 0.0) {
                return invalid("Black needs a positive forward and strike".into());
            }
            Ok(black_undiscounted(kind, forward, strike, std_dev) * discount)
        }
        ForwardModel::Bachelier => {
            let d = (forward - strike) * kind.sign();
            let h = d / std_dev;
            Ok((std_dev * h.normal_pdf() + d * h.normal_cdf()) * discount)
        }
    }
}

/// The derivatives of an option on a forward.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForwardGreeks {
    pub value: f64,
    pub delta_forward: f64,
    pub gamma_forward: f64,
    pub vega: f64,
    pub d_time: f64,
    pub d_discount: f64,
    pub strike_sensitivity: f64,
}

/// The value and derivatives of an option on a forward.
pub fn forward_greeks(
    model: ForwardModel,
    kind: OptionType,
    inputs: &ForwardInputs,
) -> Result<ForwardGreeks, Error> {
    let recorder = Recorder::new();
    let lifted = ForwardInputs {
        forward: recorder.input(inputs.forward),
        strike: recorder.input(inputs.strike),
        volatility: recorder.input(inputs.volatility),
        time: recorder.input(inputs.time),
        discount: recorder.input(inputs.discount),
    };
    let value = forward_value(model, kind, &lifted)?;
    let wrt = [lifted.forward, lifted.strike, lifted.volatility, lifted.time, lifted.discount];
    let first = recorder.gradient_vars(value, &wrt)?;
    let gamma = recorder.gradient(first[0], &[lifted.forward])?[0].mean();
    Ok(ForwardGreeks {
        value: value.primal(),
        delta_forward: first[0].primal(),
        gamma_forward: gamma,
        vega: first[2].primal(),
        d_time: first[3].primal(),
        d_discount: first[4].primal(),
        strike_sensitivity: first[1].primal(),
    })
}

/// A function of recorded inputs, for the Greeks below.
fn greeks_of<F>(inputs: &OptionInputs, f: F) -> Result<Greeks, Error>
where
    F: for<'t> Fn(&OptionInputs<Var<'t>>) -> Result<Var<'t>, Error>,
{
    let recorder = Recorder::new();
    let lifted = inputs.lift(&recorder);
    let value = f(&lifted)?;
    let wrt = [
        lifted.spot,
        lifted.strike,
        lifted.rate,
        lifted.dividend_yield,
        lifted.volatility,
        lifted.time,
    ];
    let first = recorder.gradient_vars(value, &wrt)?;
    let gamma = recorder.gradient(first[0], &[lifted.spot])?[0].mean();
    Ok(Greeks {
        value: value.primal(),
        delta: first[0].primal(),
        gamma,
        vega: first[4].primal(),
        rho: first[2].primal(),
        dividend_rho: first[3].primal(),
        theta: -first[5].primal(),
        strike_sensitivity: first[1].primal(),
    })
}

/// The value and Greeks of a European option.
pub fn european_greeks(kind: OptionType, inputs: &OptionInputs) -> Result<Greeks, Error> {
    greeks_of(inputs, |i| european_value(kind, i))
}

/// The volatility that gives `price`, by Brent's method between 1e-7 and 4, as QuantLib's
/// `impliedVolatility` searches by default.
pub fn implied_volatility(
    kind: OptionType,
    price: f64,
    inputs: &OptionInputs,
    accuracy: f64,
    max_evaluations: usize,
) -> Result<f64, Error> {
    let phi = kind.sign();
    let forward_value = inputs.spot * (-inputs.dividend_yield * inputs.time).exp();
    let strike_value = inputs.strike * (-inputs.rate * inputs.time).exp();
    let intrinsic = (phi * (forward_value - strike_value)).max(0.0);
    if !(price >= intrinsic) {
        return invalid(format!("price {price} is below the intrinsic value {intrinsic}"));
    }
    let objective = |v: f64| {
        let trial = OptionInputs { volatility: v, ..*inputs };
        Ok(european_value(kind, &trial)? - price)
    };
    brent(objective, accuracy, 0.2, 1e-7, 4.0, max_evaluations)
}

/// Knock-in or knock-out above or below the spot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BarrierType {
    DownIn,
    UpIn,
    DownOut,
    UpOut,
}

/// The price of a European barrier option with continuous monitoring (Reiner and Rubinstein;
/// QuantLib `AnalyticBarrierEngine`). The barrier and rebate are numbers, not differentiated.
pub fn barrier_value<T: Operand>(
    kind: OptionType,
    barrier_type: BarrierType,
    barrier: f64,
    rebate: f64,
    inputs: &OptionInputs<T>,
) -> Result<T, Error> {
    inputs.check()?;
    if !(barrier > 0.0) {
        return invalid(format!("barrier must be positive, not {barrier}"));
    }
    let spot = inputs.spot.primal();
    let touched = match barrier_type {
        BarrierType::DownIn | BarrierType::DownOut => spot < barrier,
        BarrierType::UpIn | BarrierType::UpOut => spot > barrier,
    };
    if touched {
        return invalid(format!("barrier {barrier} touched at spot {spot}"));
    }
    let b = BarrierTerms::new(inputs, barrier, rebate);
    let (a, c, d, e, f) = (
        |phi| b.a(phi),
        |eta, phi| b.c(eta, phi),
        |eta, phi| b.d(eta, phi),
        |eta| b.e(eta),
        |eta| b.f(eta),
    );
    let big_b = |phi| b.b(phi);
    let at_or_above = inputs.strike.primal() >= barrier;
    Ok(match (kind, barrier_type) {
        (OptionType::Call, BarrierType::DownIn) => {
            if at_or_above {
                c(1.0, 1.0) + e(1.0)
            } else {
                a(1.0) - big_b(1.0) + d(1.0, 1.0) + e(1.0)
            }
        }
        (OptionType::Call, BarrierType::UpIn) => {
            if at_or_above {
                a(1.0) + e(-1.0)
            } else {
                big_b(1.0) - c(-1.0, 1.0) + d(-1.0, 1.0) + e(-1.0)
            }
        }
        (OptionType::Call, BarrierType::DownOut) => {
            if at_or_above {
                a(1.0) - c(1.0, 1.0) + f(1.0)
            } else {
                big_b(1.0) - d(1.0, 1.0) + f(1.0)
            }
        }
        (OptionType::Call, BarrierType::UpOut) => {
            if at_or_above {
                f(-1.0)
            } else {
                a(1.0) - big_b(1.0) + c(-1.0, 1.0) - d(-1.0, 1.0) + f(-1.0)
            }
        }
        (OptionType::Put, BarrierType::DownIn) => {
            if at_or_above {
                big_b(-1.0) - c(1.0, -1.0) + d(1.0, -1.0) + e(1.0)
            } else {
                a(-1.0) + e(1.0)
            }
        }
        (OptionType::Put, BarrierType::UpIn) => {
            if at_or_above {
                a(-1.0) - big_b(-1.0) + d(-1.0, -1.0) + e(-1.0)
            } else {
                c(-1.0, -1.0) + e(-1.0)
            }
        }
        (OptionType::Put, BarrierType::DownOut) => {
            if at_or_above {
                a(-1.0) - big_b(-1.0) + c(1.0, -1.0) - d(1.0, -1.0) + f(1.0)
            } else {
                f(1.0)
            }
        }
        (OptionType::Put, BarrierType::UpOut) => {
            if at_or_above {
                big_b(-1.0) - d(-1.0, -1.0) + f(-1.0)
            } else {
                a(-1.0) - c(-1.0, -1.0) + f(-1.0)
            }
        }
    })
}

/// The terms A to F of the barrier formulas, named as in QuantLib.
struct BarrierTerms<T> {
    spot: T,
    strike: T,
    barrier: T,
    rebate: f64,
    std_dev: T,
    rate: T,
    volatility: T,
    dividend_discount: T,
    risk_free_discount: T,
    mu: T,
    mu_sigma: T,
}

impl<T: Operand> BarrierTerms<T> {
    fn new(inputs: &OptionInputs<T>, barrier: f64, rebate: f64) -> BarrierTerms<T> {
        let OptionInputs { spot, strike, rate, dividend_yield, volatility, time } = *inputs;
        let std_dev = volatility * time.sqrt();
        let mu = (rate - dividend_yield) / (volatility * volatility) - 0.5;
        BarrierTerms {
            spot,
            strike,
            barrier: spot.like(barrier),
            rebate,
            std_dev,
            rate,
            volatility,
            dividend_discount: (-dividend_yield * time).exp(),
            risk_free_discount: (-rate * time).exp(),
            mu,
            mu_sigma: (mu + 1.0) * std_dev,
        }
    }

    fn vanilla_like(&self, x: T, phi: f64) -> T {
        let n1 = (x * phi).normal_cdf();
        let n2 = ((x - self.std_dev) * phi).normal_cdf();
        (self.spot * self.dividend_discount * n1 - self.strike * self.risk_free_discount * n2) * phi
    }

    fn a(&self, phi: f64) -> T {
        self.vanilla_like((self.spot / self.strike).ln() / self.std_dev + self.mu_sigma, phi)
    }

    fn b(&self, phi: f64) -> T {
        self.vanilla_like((self.spot / self.barrier).ln() / self.std_dev + self.mu_sigma, phi)
    }

    /// `(H / S)^power`, where `power` carries the derivatives.
    fn pow_hs(&self, power: T) -> T {
        ((self.barrier / self.spot).ln() * power).exp()
    }

    /// `factor * n`, with a limit of 0 where the normal term vanishes and the power can be infinite.
    fn guarded(factor: T, n: T) -> T {
        if n.primal() == 0.0 {
            n
        } else {
            factor * n
        }
    }

    fn reflected(&self, y: T, eta: f64, phi: f64) -> T {
        let hs = self.barrier / self.spot;
        let pow0 = self.pow_hs(self.mu * 2.0);
        let pow1 = pow0 * hs * hs;
        let n1 = (y * eta).normal_cdf();
        let n2 = ((y - self.std_dev) * eta).normal_cdf();
        (self.spot * self.dividend_discount * Self::guarded(pow1, n1)
            - self.strike * self.risk_free_discount * Self::guarded(pow0, n2))
            * phi
    }

    fn c(&self, eta: f64, phi: f64) -> T {
        let hs = self.barrier / self.spot;
        let y1 = (self.barrier * hs / self.strike).ln() / self.std_dev + self.mu_sigma;
        self.reflected(y1, eta, phi)
    }

    fn d(&self, eta: f64, phi: f64) -> T {
        let y2 = (self.barrier / self.spot).ln() / self.std_dev + self.mu_sigma;
        self.reflected(y2, eta, phi)
    }

    fn e(&self, eta: f64) -> T {
        if self.rebate <= 0.0 {
            return self.spot.like(0.0);
        }
        let pow0 = self.pow_hs(self.mu * 2.0);
        let x2 = (self.spot / self.barrier).ln() / self.std_dev + self.mu_sigma;
        let y2 = (self.barrier / self.spot).ln() / self.std_dev + self.mu_sigma;
        let n1 = ((x2 - self.std_dev) * eta).normal_cdf();
        let n2 = ((y2 - self.std_dev) * eta).normal_cdf();
        self.risk_free_discount * (n1 - Self::guarded(pow0, n2)) * self.rebate
    }

    fn f(&self, eta: f64) -> T {
        if self.rebate <= 0.0 {
            return self.spot.like(0.0);
        }
        let v2 = self.volatility * self.volatility;
        let lambda = (self.mu * self.mu + self.rate * 2.0 / v2).sqrt();
        let plus = self.pow_hs(self.mu + lambda);
        let minus = self.pow_hs(self.mu - lambda);
        let z = (self.barrier / self.spot).ln() / self.std_dev + lambda * self.std_dev;
        let n1 = (z * eta).normal_cdf();
        let n2 = ((z - lambda * self.std_dev * 2.0) * eta).normal_cdf();
        (Self::guarded(plus, n1) + Self::guarded(minus, n2)) * self.rebate
    }
}

/// The value and Greeks of a barrier option.
pub fn barrier_greeks(
    kind: OptionType,
    barrier_type: BarrierType,
    barrier: f64,
    rebate: f64,
    inputs: &OptionInputs,
) -> Result<Greeks, Error> {
    greeks_of(inputs, |i| barrier_value(kind, barrier_type, barrier, rebate, i))
}

/// When an option can be exercised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Exercise {
    European,
    American,
}

/// The value at the root of a Cox-Ross-Rubinstein tree, and the first values used for the tree's
/// delta and gamma (Hull's formulas, as in QuantLib's `BinomialVanillaEngine`).
struct TreeResult<T> {
    value: T,
    at_step_1: [T; 2],
    at_step_2: [T; 3],
    spot_step_1: [f64; 2],
    spot_step_2: [f64; 3],
}

fn crr_tree<T: Operand>(
    kind: OptionType,
    exercise: Exercise,
    inputs: &OptionInputs<T>,
    steps: usize,
) -> Result<TreeResult<T>, Error> {
    inputs.check()?;
    if steps < 2 {
        return invalid(format!("at least 2 time steps required, {steps} provided"));
    }
    let OptionInputs { spot, strike, rate, dividend_yield, volatility, time } = *inputs;
    let dt = time / steps as f64;
    let drift = (rate - dividend_yield - volatility * volatility * 0.5) * dt;
    let dx = volatility * dt.sqrt();
    let pu = drift / dx * 0.5 + 0.5;
    if !(0.0..=1.0).contains(&pu.primal()) {
        return invalid(format!("negative probability in the tree (up {})", pu.primal()));
    }
    let pd = -pu + 1.0;
    let discount = (-rate * dt).exp();
    let phi = kind.sign();
    let zero = spot.like(0.0);
    let exercise_value = |i: usize, j: usize| {
        let s = spot * (dx * (2.0 * j as f64 - i as f64)).exp();
        ((s - strike) * phi).max(zero)
    };
    let mut values: Vec<T> = (0..=steps).map(|j| exercise_value(steps, j)).collect();
    let mut at_step_1 = [zero; 2];
    let mut at_step_2 = [zero; 3];
    for i in (0..steps).rev() {
        let next: Vec<T> = (0..=i)
            .map(|j| {
                let held = (pd * values[j] + pu * values[j + 1]) * discount;
                match exercise {
                    Exercise::European => held,
                    Exercise::American => held.max(exercise_value(i, j)),
                }
            })
            .collect();
        if i == 2 {
            at_step_2 = [next[0], next[1], next[2]];
        } else if i == 1 {
            at_step_1 = [next[0], next[1]];
        }
        values = next;
    }
    let at = |i: usize, j: usize| {
        spot.primal() * (dx.primal() * (2.0 * j as f64 - i as f64)).exp()
    };
    Ok(TreeResult {
        value: values[0],
        at_step_1,
        at_step_2,
        spot_step_1: [at(1, 0), at(1, 1)],
        spot_step_2: [at(2, 0), at(2, 1), at(2, 2)],
    })
}

/// The value of a vanilla option on a Cox-Ross-Rubinstein tree (QuantLib `BinomialVanillaEngine`).
pub fn binomial_value<T: Operand>(
    kind: OptionType,
    exercise: Exercise,
    inputs: &OptionInputs<T>,
    steps: usize,
) -> Result<T, Error> {
    Ok(crr_tree(kind, exercise, inputs, steps)?.value)
}

/// The value and Greeks on a tree. The value of a tree is piecewise linear in the spot, so the delta
/// and gamma are read from the first steps of the tree, as in QuantLib. The other sensitivities are
/// the derivatives of the tree by automatic differentiation; QuantLib computes none of them on a
/// tree except theta, from the Black-Scholes equation, so `theta` here can differ from QuantLib's.
pub fn binomial_greeks(
    kind: OptionType,
    exercise: Exercise,
    inputs: &OptionInputs,
    steps: usize,
) -> Result<Greeks, Error> {
    let recorder = Recorder::new();
    let lifted = inputs.lift(&recorder);
    let tree = crr_tree(kind, exercise, &lifted, steps)?;
    let wrt = [lifted.strike, lifted.rate, lifted.dividend_yield, lifted.volatility, lifted.time];
    let g = recorder.gradient(tree.value, &wrt)?;
    let p2 = tree.at_step_2.map(|v| v.primal());
    let s2 = tree.spot_step_2;
    let delta_up = (p2[2] - p2[1]) / (s2[2] - s2[1]);
    let delta_down = (p2[1] - p2[0]) / (s2[1] - s2[0]);
    let p1 = tree.at_step_1.map(|v| v.primal());
    let s1 = tree.spot_step_1;
    Ok(Greeks {
        value: tree.value.primal(),
        delta: (p1[1] - p1[0]) / (s1[1] - s1[0]),
        gamma: (delta_up - delta_down) / ((s2[2] - s2[0]) / 2.0),
        vega: g[3].mean(),
        rho: g[1].mean(),
        dividend_rho: g[2].mean(),
        theta: -g[4].mean(),
        strike_sensitivity: g[0].mean(),
    })
}

/// Settings of a Monte Carlo evaluation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonteCarloSettings {
    pub paths: u64,
    pub seed: u64,
    /// Each normal number is also used with the opposite sign; `paths` must be even.
    pub antithetic: bool,
    /// Width of the smoothed payoff `(x + sqrt(x^2 + w^2)) / 2`. Without it the payoff has a kink,
    /// whose pathwise gamma is 0, and `gamma` is not available.
    pub smoothing: Option<f64>,
    /// Worker threads; the results do not depend on them. 0 means one per logical processor.
    pub threads: usize,
}

/// The result of a Monte Carlo evaluation. `gamma` is `None` without smoothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonteCarloResult {
    pub value: f64,
    pub standard_error: f64,
    pub delta: f64,
    pub gamma: Option<f64>,
    pub vega: f64,
    pub rho: f64,
    pub dividend_rho: f64,
    pub theta: f64,
}

/// Monte Carlo of a European option over one step to maturity, with pathwise sensitivities by
/// automatic differentiation on path vectors.
pub fn monte_carlo_european(
    kind: OptionType,
    inputs: &OptionInputs,
    settings: &MonteCarloSettings,
) -> Result<MonteCarloResult, Error> {
    inputs.check()?;
    if settings.paths < 2 || (settings.antithetic && settings.paths % 2 != 0) {
        return invalid(format!(
            "paths must be at least 2, and even with antithetic numbers (not {})",
            settings.paths
        ));
    }
    if let Some(w) = settings.smoothing {
        if !(w > 0.0) {
            return invalid(format!("smoothing must be positive, not {w}"));
        }
    }
    // always on the workers: see `map_parallel`
    map_parallel(&[()], settings.threads, |_| monte_carlo_on_workers(kind, inputs, settings))?
        .pop()
        .expect("one unit")
}

fn monte_carlo_on_workers(
    kind: OptionType,
    inputs: &OptionInputs,
    settings: &MonteCarloSettings,
) -> Result<MonteCarloResult, Error> {
    let MonteCarloSettings { paths, seed, antithetic, smoothing, .. } = *settings;
    let draws = if antithetic { paths / 2 } else { paths };
    let mut normals: Vec<f64> =
        (0..draws).into_par_iter().map(|p| standard_normal(seed, p, 0)).collect();
    if antithetic {
        let opposite: Vec<f64> = normals.iter().map(|z| -z).collect();
        normals.extend(opposite);
    }

    let recorder = Recorder::new();
    let lifted = inputs.lift(&recorder);
    let z = recorder.input_paths(normals);
    let OptionInputs { spot, strike, rate, dividend_yield, volatility, time } = lifted;
    let terminal = spot
        * ((rate - dividend_yield - volatility * volatility * 0.5) * time
            + volatility * time.sqrt() * z)
            .exp();
    let moneyness = (terminal - strike) * kind.sign();
    let payoff = match smoothing {
        None => moneyness.max(recorder.constant(0.0)),
        Some(w) => (moneyness + (moneyness * moneyness + w * w).sqrt()) * 0.5,
    };
    let discount = (-rate * time).exp();
    let value = discount * payoff.average();

    let wrt = [spot, rate, dividend_yield, volatility, time];
    let first = recorder.gradient_vars(value, &wrt)?;
    let gamma = match smoothing {
        Some(_) => Some(recorder.gradient(first[0], &[spot])?[0].mean()),
        None => None,
    };
    let standard_error =
        discount.primal() * standard_error_of(&payoff.value(), antithetic.then_some(draws));
    Ok(MonteCarloResult {
        value: value.primal(),
        standard_error,
        delta: first[0].primal(),
        gamma,
        vega: first[3].primal(),
        rho: first[1].primal(),
        dividend_rho: first[2].primal(),
        theta: -first[4].primal(),
    })
}

/// The standard error of the mean. With `pairs`, the first half and the second half of the paths
/// are antithetic pairs, and each pair counts as one sample.
pub(crate) fn standard_error_of(payoff: &Value, pairs: Option<u64>) -> f64 {
    let Value::Paths(paths) = payoff else {
        return 0.0;
    };
    let samples: Vec<f64> = match pairs {
        Some(n) => {
            let n = n as usize;
            (0..n).map(|i| 0.5 * (paths[i] + paths[n + i])).collect()
        }
        None => paths.to_vec(),
    };
    let n = samples.len() as f64;
    let mean = samples.iter().sum::<f64>() / n;
    let variance = samples.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / (n - 1.0);
    (variance / n).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs() -> OptionInputs {
        OptionInputs {
            spot: 100.0,
            strike: 105.0,
            rate: 0.03,
            dividend_yield: 0.01,
            volatility: 0.25,
            time: 1.5,
        }
    }

    #[test]
    fn put_call_parity() {
        let i = inputs();
        let call = european_value(OptionType::Call, &i).unwrap();
        let put = european_value(OptionType::Put, &i).unwrap();
        let parity = i.spot * (-i.dividend_yield * i.time).exp() - i.strike * (-i.rate * i.time).exp();
        assert!((call - put - parity).abs() < 1e-12);
    }

    #[test]
    fn greeks_agree_with_numeric_differences() {
        let i = inputs();
        for kind in [OptionType::Call, OptionType::Put] {
            let g = european_greeks(kind, &i).unwrap();
            let v = |f: &dyn Fn(&mut OptionInputs, f64), h: f64| {
                let (mut up, mut down) = (i, i);
                f(&mut up, h);
                f(&mut down, -h);
                (european_value(kind, &up).unwrap() - european_value(kind, &down).unwrap()) / (2.0 * h)
            };
            assert!((g.delta - v(&|x, h| x.spot += h, 1e-4)).abs() < 1e-8);
            assert!((g.vega - v(&|x, h| x.volatility += h, 1e-5)).abs() < 1e-7);
            assert!((g.rho - v(&|x, h| x.rate += h, 1e-5)).abs() < 1e-7);
            assert!((g.dividend_rho - v(&|x, h| x.dividend_yield += h, 1e-5)).abs() < 1e-7);
            assert!((g.theta + v(&|x, h| x.time += h, 1e-5)).abs() < 1e-7);
            let h = 1e-2;
            let bumped = |s: f64| european_value(kind, &OptionInputs { spot: s, ..i }).unwrap();
            let gamma = (bumped(i.spot + h) - 2.0 * g.value + bumped(i.spot - h)) / (h * h);
            assert!((g.gamma - gamma).abs() < 1e-6, "{} {gamma}", g.gamma);
        }
    }

    #[test]
    fn the_tree_converges_to_the_analytic_value() {
        let i = inputs();
        for kind in [OptionType::Call, OptionType::Put] {
            let analytic = european_value(kind, &i).unwrap();
            let tree = binomial_value(kind, Exercise::European, &i, 2000).unwrap();
            assert!((tree - analytic).abs() < 0.01, "{tree} {analytic}");
        }
    }

    #[test]
    fn american_puts_are_worth_at_least_european_puts_and_exercise_value() {
        let i = OptionInputs { strike: 130.0, ..inputs() };
        let american = binomial_value(OptionType::Put, Exercise::American, &i, 200).unwrap();
        let european = binomial_value(OptionType::Put, Exercise::European, &i, 200).unwrap();
        assert!(american > european);
        assert!(american >= 130.0 - 100.0);
    }

    #[test]
    fn tree_sensitivities_agree_with_numeric_differences() {
        let i = OptionInputs { strike: 110.0, ..inputs() };
        let g = binomial_greeks(OptionType::Put, Exercise::American, &i, 100).unwrap();
        let value = |x: OptionInputs| {
            binomial_value(OptionType::Put, Exercise::American, &x, 100).unwrap()
        };
        let h = 1e-6;
        let vega = (value(OptionInputs { volatility: i.volatility + h, ..i })
            - value(OptionInputs { volatility: i.volatility - h, ..i }))
            / (2.0 * h);
        let rho = (value(OptionInputs { rate: i.rate + h, ..i })
            - value(OptionInputs { rate: i.rate - h, ..i }))
            / (2.0 * h);
        assert!((g.vega - vega).abs() < 1e-5, "{} {vega}", g.vega);
        assert!((g.rho - rho).abs() < 1e-5, "{} {rho}", g.rho);
    }

    #[test]
    fn knock_in_plus_knock_out_is_the_vanilla_option() {
        let i = inputs();
        for kind in [OptionType::Call, OptionType::Put] {
            let vanilla = european_value(kind, &i).unwrap();
            for (out, into, barrier) in [
                (BarrierType::DownOut, BarrierType::DownIn, 80.0),
                (BarrierType::DownOut, BarrierType::DownIn, 95.0),
                (BarrierType::UpOut, BarrierType::UpIn, 120.0),
                (BarrierType::UpOut, BarrierType::UpIn, 150.0),
            ] {
                let sum = barrier_value(kind, out, barrier, 0.0, &i).unwrap()
                    + barrier_value(kind, into, barrier, 0.0, &i).unwrap();
                assert!((sum - vanilla).abs() < 1e-10, "{kind:?} {barrier}: {sum} {vanilla}");
            }
        }
    }

    #[test]
    fn a_touched_barrier_is_an_error() {
        assert!(barrier_value(OptionType::Call, BarrierType::DownOut, 110.0, 0.0, &inputs()).is_err());
    }

    #[test]
    fn barrier_greeks_agree_with_numeric_differences() {
        let i = inputs();
        for (barrier_type, barrier, rebate) in [
            (BarrierType::DownOut, 80.0, 0.0),
            (BarrierType::UpOut, 150.0, 3.0),
            (BarrierType::DownIn, 90.0, 2.0),
            (BarrierType::UpIn, 120.0, 0.0),
        ] {
            let g = barrier_greeks(OptionType::Call, barrier_type, barrier, rebate, &i).unwrap();
            let value = |x: OptionInputs| {
                barrier_value(OptionType::Call, barrier_type, barrier, rebate, &x).unwrap()
            };
            let h = 1e-4;
            let delta = (value(OptionInputs { spot: i.spot + h, ..i })
                - value(OptionInputs { spot: i.spot - h, ..i }))
                / (2.0 * h);
            let vega = (value(OptionInputs { volatility: i.volatility + 1e-5, ..i })
                - value(OptionInputs { volatility: i.volatility - 1e-5, ..i }))
                / 2e-5;
            assert!((g.delta - delta).abs() < 1e-7, "{barrier_type:?} {} {delta}", g.delta);
            assert!((g.vega - vega).abs() < 1e-6, "{barrier_type:?} {} {vega}", g.vega);
        }
    }

    #[test]
    fn forward_formulas_are_consistent() {
        let i = inputs();
        let discount = (-i.rate * i.time).exp();
        let forward = i.spot * (-i.dividend_yield * i.time).exp() / discount;
        let inputs = ForwardInputs { forward, strike: i.strike, volatility: i.volatility, time: i.time, discount };
        let black = forward_value(ForwardModel::Black, OptionType::Call, &inputs).unwrap();
        assert!((black - european_value(OptionType::Call, &i).unwrap()).abs() < 1e-11);
        // Bachelier: put-call parity
        let n = ForwardInputs { volatility: 20.0, ..inputs };
        let call = forward_value(ForwardModel::Bachelier, OptionType::Call, &n).unwrap();
        let put = forward_value(ForwardModel::Bachelier, OptionType::Put, &n).unwrap();
        assert!((call - put - discount * (forward - i.strike)).abs() < 1e-11);
        // derivatives
        let g = forward_greeks(ForwardModel::Bachelier, OptionType::Call, &n).unwrap();
        let h = 1e-4;
        let up = forward_value(ForwardModel::Bachelier, OptionType::Call, &ForwardInputs { forward: forward + h, ..n }).unwrap();
        let down = forward_value(ForwardModel::Bachelier, OptionType::Call, &ForwardInputs { forward: forward - h, ..n }).unwrap();
        assert!((g.delta_forward - (up - down) / (2.0 * h)).abs() < 1e-8);
    }

    #[test]
    fn implied_volatility_recovers_the_input() {
        let i = inputs();
        for kind in [OptionType::Call, OptionType::Put] {
            let price = european_value(kind, &i).unwrap();
            let v = implied_volatility(kind, price, &i, 1e-12, 200).unwrap();
            assert!((v - i.volatility).abs() < 1e-10, "{v}");
        }
        assert!(implied_volatility(OptionType::Call, -1.0, &i, 1e-8, 100).is_err());
    }

    fn settings(threads: usize) -> MonteCarloSettings {
        MonteCarloSettings { paths: 200_000, seed: 20261101, antithetic: true, smoothing: None, threads }
    }

    #[test]
    fn monte_carlo_agrees_with_the_analytic_value_and_greeks() {
        let i = inputs();
        for kind in [OptionType::Call, OptionType::Put] {
            let g = european_greeks(kind, &i).unwrap();
            let mc = monte_carlo_european(kind, &i, &settings(2)).unwrap();
            let within = |a: f64, b: f64, name: &str| {
                assert!((a - b).abs() < 0.015 * (1.0 + b.abs()), "{name}: {a} {b}");
            };
            assert!((mc.value - g.value).abs() < 5.0 * mc.standard_error, "{} {}", mc.value, g.value);
            within(mc.delta, g.delta, "delta");
            within(mc.vega, g.vega, "vega");
            within(mc.rho, g.rho, "rho");
            within(mc.dividend_rho, g.dividend_rho, "dividend rho");
            within(mc.theta, g.theta, "theta");
            assert_eq!(mc.gamma, None);
        }
    }

    #[test]
    fn smoothed_monte_carlo_has_a_gamma() {
        let i = inputs();
        let g = european_greeks(OptionType::Call, &i).unwrap();
        let s = MonteCarloSettings { smoothing: Some(0.25), ..settings(2) };
        let mc = monte_carlo_european(OptionType::Call, &i, &s).unwrap();
        let gamma = mc.gamma.unwrap();
        assert!((gamma - g.gamma).abs() < 0.03 * g.gamma, "{gamma} {}", g.gamma);
    }

    #[test]
    fn monte_carlo_does_not_depend_on_the_number_of_threads() {
        let i = inputs();
        let one = monte_carlo_european(OptionType::Call, &i, &settings(1)).unwrap();
        for threads in [2, 4] {
            let many = monte_carlo_european(OptionType::Call, &i, &settings(threads)).unwrap();
            assert_eq!(one, many);
        }
    }

    #[test]
    fn invalid_inputs_are_errors() {
        let i = inputs();
        assert!(european_value(OptionType::Call, &OptionInputs { volatility: 0.0, ..i }).is_err());
        assert!(european_value(OptionType::Call, &OptionInputs { spot: f64::NAN, ..i }).is_err());
        assert!(binomial_value(OptionType::Call, Exercise::American, &i, 1).is_err());
        let odd = MonteCarloSettings { paths: 3, ..settings(1) };
        assert!(monte_carlo_european(OptionType::Call, &i, &odd).is_err());
    }
}
