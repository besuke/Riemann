//! Constrained portfolio optimization and risk contributions (LagrangeFinance I-8; `docs/spec_portfolio.md`).
//!
//! The inputs are a matrix of returns, one row per period and one column per asset, and constraints on
//! the weights: their sum and a lower and upper bound per asset. Every method is solved exactly:
//! quadratic programs by the dual active-set method (`math::quadratic`), the maximum return in closed
//! form, the maximum Sharpe ratio as a quadratic program in `y = kappa w`, the minimum expected shortfall
//! as the linear program of Rockafellar and Uryasev (`math::simplex`), the risk budget by Newton's method
//! on Spinu's convex problem, and the target volatility by bisection on the frontier.
//!
//! Risk measures are written once over [`Operand`], so that their Euler contributions `w_i dR/dw_i`
//! come from automatic differentiation; they add up to the measure, which is homogeneous of degree one.
//!
//! Mean-variance maximizes `mu'w - (gamma / 2) w'Sigma w` (LagrangeFinance I-8.4.2). GaussQuant's
//! `risk_aversion` is `mu'w - lambda w'Sigma w`, that is `gamma = 2 lambda`.

// `!(x > 0.0)` is intended: it also rejects NaN
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::autodiff::{compute_value_and_gradient, Operand};
use crate::math::normal::{compute_inverse_normal_cdf, compute_normal_cdf, compute_normal_pdf};
use crate::math::quadratic::{solve_quadratic_program, LinearConstraints};
use crate::math::simplex::solve_linear_program;
use crate::Error;

fn invalid<T>(reason: impl Into<String>) -> Result<T, Error> {
    Err(Error::InvalidInput { reason: reason.into() })
}

/// Returns by rows: period 1's assets, then period 2's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReturnMatrix<'a> {
    pub values: &'a [f64],
    pub assets: usize,
}

impl ReturnMatrix<'_> {
    pub fn periods(&self) -> usize {
        self.values.len() / self.assets.max(1)
    }
    fn row(&self, t: usize) -> &[f64] {
        &self.values[t * self.assets..(t + 1) * self.assets]
    }
    fn check(&self) -> Result<(), Error> {
        if self.assets == 0 || self.values.len() % self.assets != 0 {
            return invalid("the returns must have one column per asset and whole rows");
        }
        if self.periods() < 2 {
            return invalid("at least two periods of returns are needed");
        }
        if self.values.iter().any(|v| !v.is_finite()) {
            return invalid("the returns must be finite (remove the periods with missing returns first)");
        }
        Ok(())
    }
}

/// The sample mean and covariance (divisor `T - 1`) of the returns.
#[derive(Debug, Clone, PartialEq)]
pub struct ReturnMoments {
    pub mean: Vec<f64>,
    pub covariance: Vec<Vec<f64>>,
    pub periods: usize,
}

#[allow(clippy::needless_range_loop)]
pub fn compute_return_moments(returns: &ReturnMatrix) -> Result<ReturnMoments, Error> {
    returns.check()?;
    let (nt, n) = (returns.periods(), returns.assets);
    let mut mean = vec![0.0; n];
    for t in 0..nt {
        for (m, r) in mean.iter_mut().zip(returns.row(t)) {
            *m += r;
        }
    }
    for m in &mut mean {
        *m /= nt as f64;
    }
    let mut covariance = vec![vec![0.0; n]; n];
    for t in 0..nt {
        let row = returns.row(t);
        for i in 0..n {
            let di = row[i] - mean[i];
            for j in 0..=i {
                covariance[i][j] += di * (row[j] - mean[j]);
            }
        }
    }
    for i in 0..n {
        for j in 0..=i {
            covariance[i][j] /= (nt - 1) as f64;
            covariance[j][i] = covariance[i][j];
        }
    }
    Ok(ReturnMoments { mean, covariance, periods: nt })
}

/// The weights sum to `weight_sum`, and `lower_i <= w_i <= upper_i` (infinite bounds allowed).
#[derive(Debug, Clone, PartialEq)]
pub struct PortfolioConstraints {
    pub weight_sum: f64,
    pub lower: Vec<f64>,
    pub upper: Vec<f64>,
}

impl PortfolioConstraints {
    pub fn check(&self, assets: usize) -> Result<(), Error> {
        if self.lower.len() != assets || self.upper.len() != assets {
            return invalid(format!("the constraints need one lower and one upper bound per asset ({assets})"));
        }
        if !self.weight_sum.is_finite() {
            return invalid("the sum of the weights must be finite");
        }
        if self.lower.iter().chain(&self.upper).any(|v| v.is_nan()) {
            return invalid("the bounds must be numbers (infinite bounds are allowed)");
        }
        if let Some(i) = (0..assets).find(|&i| self.lower[i] > self.upper[i] || self.lower[i] == f64::INFINITY || self.upper[i] == f64::NEG_INFINITY) {
            return invalid(format!("the lower bound of asset {} is above its upper bound", i + 1));
        }
        let (lo, hi): (f64, f64) = (self.lower.iter().sum(), self.upper.iter().sum());
        let tol = 1e-12 * (1.0 + self.weight_sum.abs());
        if lo > self.weight_sum + tol || hi < self.weight_sum - tol {
            return invalid(format!("no weights within the bounds sum to {} (the bounds sum to {lo} and {hi})", self.weight_sum));
        }
        Ok(())
    }
    fn bounded(&self) -> bool {
        self.lower.iter().chain(&self.upper).any(|v| v.is_finite())
    }
}

/// What the optimization looks for.
#[derive(Debug, Clone, PartialEq)]
pub enum PortfolioMethod {
    MinimumVariance,
    /// `max mu'w - (risk_aversion / 2) w'Sigma w`.
    MeanVariance { risk_aversion: f64 },
    MaximumReturn,
    /// `max (mu'w - r_f) / sqrt(w'Sigma w)`; `risk_free_rate` per period.
    MaximumSharpe { risk_free_rate: f64 },
    /// Rockafellar-Uryasev expected shortfall of the losses `-r_t'w` at `confidence_level`.
    MinimumExpectedShortfall { confidence_level: f64 },
    /// Risk contributions `w_i (Sigma w)_i / w'Sigma w` equal to `budget` (normalized to sum 1).
    RiskBudget { budget: Vec<f64> },
    /// Minimum variance with `mu'w >= target`.
    TargetReturn { target: f64 },
    /// Maximum return with `sqrt(w'Sigma w) <= target`.
    TargetVolatility { target: f64 },
}

/// An optimal portfolio and how it was found.
#[derive(Debug, Clone, PartialEq)]
pub struct OptimalPortfolio {
    pub weights: Vec<f64>,
    pub expected_return: f64,
    pub volatility: f64,
    /// The quantity optimized, in its own sign: variance, utility, return, Sharpe ratio, expected
    /// shortfall, sum of squared deviations of the risk shares, or return.
    pub objective: f64,
    /// Assets at their lower and upper bounds.
    pub at_lower: Vec<bool>,
    pub at_upper: Vec<bool>,
    pub solver: &'static str,
    pub iterations: usize,
    /// For the quadratic programs, `max |G x + a - sum u_i n_i|` at the solution; otherwise 0.
    pub stationarity_residual: f64,
}

fn quad_form(sigma: &[Vec<f64>], w: &[f64]) -> f64 {
    sigma.iter().zip(w).map(|(row, wi)| wi * row.iter().zip(w).map(|(s, wj)| s * wj).sum::<f64>()).sum()
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// The sum, the bounds and any extra rows `(normal, bound)` as `>=`, for the weights.
fn weight_constraints(c: &PortfolioConstraints, extra: Vec<(Vec<f64>, f64)>) -> LinearConstraints {
    let n = c.lower.len();
    let mut normals = vec![vec![1.0; n]];
    let mut bounds = vec![c.weight_sum];
    for (row, b) in extra {
        normals.push(row);
        bounds.push(b);
    }
    for i in 0..n {
        if c.lower[i].is_finite() {
            let mut e = vec![0.0; n];
            e[i] = 1.0;
            normals.push(e);
            bounds.push(c.lower[i]);
        }
        if c.upper[i].is_finite() {
            let mut e = vec![0.0; n];
            e[i] = -1.0;
            normals.push(e);
            bounds.push(-c.upper[i]);
        }
    }
    LinearConstraints { normals, bounds, equalities: 1 }
}

struct QpResult {
    weights: Vec<f64>,
    iterations: usize,
    residual: f64,
}

fn solve_weights(g: &[Vec<f64>], a: &[f64], c: &PortfolioConstraints, extra: Vec<(Vec<f64>, f64)>) -> Result<QpResult, Error> {
    let s = solve_quadratic_program(g, a, &weight_constraints(c, extra))?;
    Ok(QpResult { weights: s.x, iterations: s.iterations, residual: s.stationarity_residual })
}

fn minimum_variance(m: &ReturnMoments, c: &PortfolioConstraints) -> Result<QpResult, Error> {
    solve_weights(&m.covariance, &vec![0.0; m.mean.len()], c, vec![])
}

fn target_return(m: &ReturnMoments, c: &PortfolioConstraints, target: f64) -> Result<QpResult, Error> {
    solve_weights(&m.covariance, &vec![0.0; m.mean.len()], c, vec![(m.mean.clone(), target)]).map_err(|e| match e {
        Error::InvalidInput { reason } if reason.contains("no feasible point") => Error::InvalidInput {
            reason: format!("no weights within the constraints reach the target return {target}"),
        },
        other => other,
    })
}

/// The largest return: fill the assets in decreasing order of mean return up to their upper bounds.
fn maximum_return(m: &ReturnMoments, c: &PortfolioConstraints) -> Result<Vec<f64>, Error> {
    let n = m.mean.len();
    if c.lower.iter().any(|v| !v.is_finite()) || c.upper.iter().any(|v| !v.is_finite()) {
        return invalid("the maximum return needs finite lower and upper bounds on every weight");
    }
    let mut w = c.lower.clone();
    let mut left = c.weight_sum - w.iter().sum::<f64>();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| m.mean[j].total_cmp(&m.mean[i]).then(i.cmp(&j)));
    for i in order {
        let add = left.min(c.upper[i] - c.lower[i]);
        w[i] += add;
        left -= add;
        if left <= 0.0 {
            break;
        }
    }
    Ok(w)
}

/// max Sharpe as `min y'Sigma y`, `(mu - r_f)'y = 1`, `y_i - (l_i / s) 1'y >= 0`, `(u_i / s) 1'y - y_i >= 0`,
/// `1'y >= 0`, and `w = s y / 1'y` for the sum `s > 0`.
fn maximum_sharpe(m: &ReturnMoments, c: &PortfolioConstraints, rf: f64) -> Result<QpResult, Error> {
    let n = m.mean.len();
    let s = c.weight_sum;
    if !(s > 0.0) {
        return invalid("the maximum Sharpe ratio needs a positive sum of the weights");
    }
    let mut normals = vec![m.mean.iter().map(|v| v - rf).collect::<Vec<f64>>()];
    let mut bounds = vec![1.0];
    for i in 0..n {
        if c.lower[i].is_finite() {
            normals.push((0..n).map(|k| if k == i { 1.0 } else { 0.0 } - c.lower[i] / s).collect());
            bounds.push(0.0);
        }
        if c.upper[i].is_finite() {
            normals.push((0..n).map(|k| c.upper[i] / s - if k == i { 1.0 } else { 0.0 }).collect());
            bounds.push(0.0);
        }
    }
    normals.push(vec![1.0; n]);
    bounds.push(0.0);
    let sol = solve_quadratic_program(&m.covariance, &vec![0.0; n], &LinearConstraints { normals, bounds, equalities: 1 }).map_err(|e| match e {
        Error::InvalidInput { reason } if reason.contains("no feasible point") || reason.contains("linearly dependent") => Error::InvalidInput {
            reason: "no weights within the constraints have a return above the risk-free rate".into(),
        },
        other => other,
    })?;
    let total: f64 = sol.x.iter().sum();
    if !(total > 0.0) {
        return invalid("the maximum Sharpe ratio portfolio is not defined for these constraints");
    }
    Ok(QpResult { weights: sol.x.iter().map(|y| s * y / total).collect(), iterations: sol.iterations, residual: sol.stationarity_residual })
}

/// The Rockafellar-Uryasev program in standard form. Variables: `w - l` (or `w+`, `w-` where the lower
/// bound is infinite), `zeta+`, `zeta-`, the excess losses `u_t`, their slacks, and the slacks of the
/// finite upper bounds.
fn minimum_expected_shortfall(returns: &ReturnMatrix, c: &PortfolioConstraints, alpha: f64) -> Result<(Vec<f64>, f64, usize), Error> {
    let (nt, n) = (returns.periods(), returns.assets);
    let k = (1.0 - alpha) * nt as f64;
    // columns of w: for each asset either one (w - l >= 0) or two (w+ - w-)
    let mut w_cols: Vec<Vec<(usize, f64)>> = Vec::with_capacity(n);
    let mut col = 0usize;
    for i in 0..n {
        if c.lower[i].is_finite() {
            w_cols.push(vec![(col, 1.0)]);
            col += 1;
        } else {
            w_cols.push(vec![(col, 1.0), (col + 1, -1.0)]);
            col += 2;
        }
    }
    let zeta = col;
    let u0 = zeta + 2;
    let e0 = u0 + nt;
    let uppers: Vec<usize> = (0..n).filter(|&i| c.upper[i].is_finite()).collect();
    let s0 = e0 + nt;
    let width = s0 + uppers.len();
    let shift: Vec<f64> = c.lower.iter().map(|l| if l.is_finite() { *l } else { 0.0 }).collect();

    let mut a = Vec::with_capacity(nt + 1 + uppers.len());
    let mut b = Vec::with_capacity(nt + 1 + uppers.len());
    for t in 0..nt {
        // r_t'w + zeta + u_t - e_t = 0, with w = shift + columns
        let r = returns.row(t);
        let mut row = vec![0.0; width];
        for i in 0..n {
            for &(cidx, sgn) in &w_cols[i] {
                row[cidx] += sgn * r[i];
            }
        }
        row[zeta] = 1.0;
        row[zeta + 1] = -1.0;
        row[u0 + t] = 1.0;
        row[e0 + t] = -1.0;
        a.push(row);
        b.push(-dot(r, &shift));
    }
    let mut row = vec![0.0; width];
    for cols in &w_cols {
        for &(cidx, sgn) in cols {
            row[cidx] = sgn;
        }
    }
    a.push(row);
    b.push(c.weight_sum - shift.iter().sum::<f64>());
    for (k_up, &i) in uppers.iter().enumerate() {
        let mut row = vec![0.0; width];
        for &(cidx, sgn) in &w_cols[i] {
            row[cidx] = sgn;
        }
        row[s0 + k_up] = 1.0;
        a.push(row);
        b.push(c.upper[i] - shift[i]);
    }
    let mut cost = vec![0.0; width];
    cost[zeta] = 1.0;
    cost[zeta + 1] = -1.0;
    for t in 0..nt {
        cost[u0 + t] = 1.0 / k;
    }
    let sol = solve_linear_program(&cost, &a, &b).map_err(|e| match e {
        Error::InvalidInput { reason } if reason.contains("unbounded") => Error::InvalidInput {
            reason: "the minimum expected shortfall is unbounded: bound the weights".into(),
        },
        other => other,
    })?;
    let w: Vec<f64> = (0..n).map(|i| shift[i] + w_cols[i].iter().map(|&(cidx, sgn)| sgn * sol.x[cidx]).sum::<f64>()).collect();
    Ok((w, sol.objective, sol.pivots))
}

/// Spinu (2013): `min 1/2 y'Sigma y - sum b_i log y_i` over `y > 0`; then `w = s y / 1'y`, whose risk
/// contributions are `b`.
fn risk_budget(m: &ReturnMoments, c: &PortfolioConstraints, budget: &[f64]) -> Result<(Vec<f64>, usize), Error> {
    let n = m.mean.len();
    if budget.len() != n || budget.iter().any(|b| !(b > &0.0) || !b.is_finite()) {
        return invalid(format!("the risk budget needs one positive number per asset ({n})"));
    }
    if !(c.weight_sum > 0.0) {
        return invalid("the risk budget needs a positive sum of the weights");
    }
    let total: f64 = budget.iter().sum();
    let b: Vec<f64> = budget.iter().map(|v| v / total).collect();
    let sigma = &m.covariance;
    let mut y: Vec<f64> = (0..n).map(|i| b[i] / sigma[i][i].sqrt()).collect();
    let objective = |y: &[f64]| 0.5 * quad_form(sigma, y) - b.iter().zip(y).map(|(bi, yi)| bi * yi.ln()).sum::<f64>();
    let mut iterations = 0;
    for _ in 0..200 {
        iterations += 1;
        let sy: Vec<f64> = sigma.iter().map(|row| dot(row, &y)).collect();
        let grad: Vec<f64> = (0..n).map(|i| sy[i] - b[i] / y[i]).collect();
        let mut h = sigma.clone();
        for i in 0..n {
            h[i][i] += b[i] / (y[i] * y[i]);
        }
        let step = crate::math::linear::solve(&h, &grad.iter().map(|g| vec![*g]).collect::<Vec<_>>())?;
        // the Newton decrement g'H^-1 g; near the optimum the full step makes it the square of the error
        let decrement: f64 = grad.iter().zip(&step).map(|(g, s)| g * s[0]).sum();
        let f0 = objective(&y);
        let mut t = 1.0;
        loop {
            let trial: Vec<f64> = (0..n).map(|i| y[i] - t * step[i][0]).collect();
            // near the optimum the change of the objective is below its rounding, so only positivity is asked
            if trial.iter().all(|v| *v > 0.0) && (decrement <= 1e-10 || objective(&trial) <= f0) {
                y = trial;
                break;
            }
            t /= 2.0;
            if t < 1e-20 {
                if decrement <= 1e-24 {
                    break; // already at the optimum to rounding
                }
                return Err(Error::NoConvergence { reason: "the risk budget's Newton steps stalled".into() });
            }
        }
        if t == 1.0 && decrement <= 1e-28 {
            break;
        }
    }
    let sum_y: f64 = y.iter().sum();
    let w: Vec<f64> = y.iter().map(|v| c.weight_sum * v / sum_y).collect();
    let tol = 1e-12;
    if let Some(i) = (0..n).find(|&i| w[i] < c.lower[i] - tol || w[i] > c.upper[i] + tol) {
        return invalid(format!(
            "the risk-budget portfolio puts {} on asset {}, outside its bounds; risk budgets with binding bounds are not supported",
            w[i],
            i + 1
        ));
    }
    Ok((w, iterations))
}

fn statistics(m: &ReturnMoments, w: &[f64]) -> (f64, f64) {
    (dot(&m.mean, w), quad_form(&m.covariance, w).max(0.0).sqrt())
}

/// Solves one method under the constraints.
pub fn compute_optimal_portfolio(returns: &ReturnMatrix, constraints: &PortfolioConstraints, method: &PortfolioMethod) -> Result<OptimalPortfolio, Error> {
    let m = compute_return_moments(returns)?;
    constraints.check(returns.assets)?;
    let n = returns.assets;
    let (weights, solver, iterations, residual) = match method {
        PortfolioMethod::MinimumVariance => {
            let r = minimum_variance(&m, constraints)?;
            (r.weights, "dual active set (Goldfarb-Idnani)", r.iterations, r.residual)
        }
        PortfolioMethod::MeanVariance { risk_aversion } => {
            if !(*risk_aversion > 0.0) || !risk_aversion.is_finite() {
                return invalid("the risk aversion must be positive and finite");
            }
            let g: Vec<Vec<f64>> = m.covariance.iter().map(|row| row.iter().map(|v| v * risk_aversion).collect()).collect();
            let a: Vec<f64> = m.mean.iter().map(|v| -v).collect();
            let r = solve_weights(&g, &a, constraints, vec![])?;
            (r.weights, "dual active set (Goldfarb-Idnani)", r.iterations, r.residual)
        }
        PortfolioMethod::MaximumReturn => (maximum_return(&m, constraints)?, "closed form (assets in order of mean return)", 0, 0.0),
        PortfolioMethod::MaximumSharpe { risk_free_rate } => {
            if !risk_free_rate.is_finite() {
                return invalid("the risk-free rate must be finite");
            }
            let r = maximum_sharpe(&m, constraints, *risk_free_rate)?;
            (r.weights, "dual active set on y = kappa w (Goldfarb-Idnani)", r.iterations, r.residual)
        }
        PortfolioMethod::MinimumExpectedShortfall { confidence_level } => {
            check_confidence(*confidence_level)?;
            let (w, _, pivots) = minimum_expected_shortfall(returns, constraints, *confidence_level)?;
            (w, "simplex (Rockafellar-Uryasev linear program)", pivots, 0.0)
        }
        PortfolioMethod::RiskBudget { budget } => {
            let (w, it) = risk_budget(&m, constraints, budget)?;
            (w, "Newton (Spinu's convex problem)", it, 0.0)
        }
        PortfolioMethod::TargetReturn { target } => {
            if !target.is_finite() {
                return invalid("the target return must be finite");
            }
            let r = target_return(&m, constraints, *target)?;
            (r.weights, "dual active set (Goldfarb-Idnani)", r.iterations, r.residual)
        }
        PortfolioMethod::TargetVolatility { target } => {
            if !(*target > 0.0) || !target.is_finite() {
                return invalid("the target volatility must be positive and finite");
            }
            let (w, it, res) = target_volatility(&m, constraints, *target)?;
            (w, "bisection on the frontier, dual active set (Goldfarb-Idnani)", it, res)
        }
    };
    let (expected_return, volatility) = statistics(&m, &weights);
    let objective = match method {
        PortfolioMethod::MinimumVariance | PortfolioMethod::TargetReturn { .. } => volatility * volatility,
        PortfolioMethod::MeanVariance { risk_aversion } => expected_return - 0.5 * risk_aversion * volatility * volatility,
        PortfolioMethod::MaximumReturn | PortfolioMethod::TargetVolatility { .. } => expected_return,
        PortfolioMethod::MaximumSharpe { risk_free_rate } => (expected_return - risk_free_rate) / volatility,
        PortfolioMethod::MinimumExpectedShortfall { confidence_level } => compute_historical_es(returns, &weights, *confidence_level),
        PortfolioMethod::RiskBudget { budget } => {
            let total: f64 = budget.iter().sum();
            let sw: Vec<f64> = m.covariance.iter().map(|row| dot(row, &weights)).collect();
            let var = volatility * volatility;
            (0..n).map(|i| (weights[i] * sw[i] / var - budget[i] / total).powi(2)).sum()
        }
    };
    let tol = 1e-10;
    Ok(OptimalPortfolio {
        at_lower: (0..n).map(|i| constraints.lower[i].is_finite() && (weights[i] - constraints.lower[i]).abs() <= tol).collect(),
        at_upper: (0..n).map(|i| constraints.upper[i].is_finite() && (weights[i] - constraints.upper[i]).abs() <= tol).collect(),
        weights,
        expected_return,
        volatility,
        objective,
        solver,
        iterations,
        stationarity_residual: residual,
    })
}

/// The maximum return with volatility at most `target`: the frontier's return is found by bisection,
/// each point being a target-return quadratic program.
fn target_volatility(m: &ReturnMoments, c: &PortfolioConstraints, target: f64) -> Result<(Vec<f64>, usize, f64), Error> {
    let low = minimum_variance(m, c)?;
    let (r_low, v_low) = statistics(m, &low.weights);
    if v_low > target * (1.0 + 1e-12) {
        return invalid(format!("the target volatility {target} is below the minimum volatility {v_low}"));
    }
    if !c.bounded() {
        // without bounds the frontier is unbounded above; walk it in closed steps of the target return
        let mut hi = r_low + 1e-3;
        let mut iterations = 0;
        while statistics(m, &target_return(m, c, hi)?.weights).1 <= target {
            hi = r_low + 2.0 * (hi - r_low);
            iterations += 1;
            if iterations > 200 {
                return Err(Error::NoConvergence { reason: "the target volatility could not be bracketed".into() });
            }
        }
        return bisect_frontier(m, c, target, r_low, hi);
    }
    let top = maximum_return(m, c)?;
    let (r_top, v_top) = statistics(m, &top);
    if v_top <= target {
        return Ok((top, 0, 0.0));
    }
    bisect_frontier(m, c, target, r_low, r_top)
}

fn bisect_frontier(m: &ReturnMoments, c: &PortfolioConstraints, target: f64, mut lo: f64, mut hi: f64) -> Result<(Vec<f64>, usize, f64), Error> {
    let mut iterations = 0;
    while hi - lo > 4.0 * f64::EPSILON * (1.0 + hi.abs()) && iterations < 200 {
        let mid = 0.5 * (lo + hi);
        if statistics(m, &target_return(m, c, mid)?.weights).1 > target {
            hi = mid;
        } else {
            lo = mid;
        }
        iterations += 1;
    }
    let r = target_return(m, c, lo)?;
    Ok((r.weights, iterations, r.residual))
}

/// Minimum-variance portfolios at each target return.
pub fn compute_efficient_frontier(returns: &ReturnMatrix, constraints: &PortfolioConstraints, targets: &[f64]) -> Result<Vec<OptimalPortfolio>, Error> {
    targets.iter().map(|&target| compute_optimal_portfolio(returns, constraints, &PortfolioMethod::TargetReturn { target })).collect()
}

/// The normal quantile to full precision: QuantLib's approximation (relative error about 1e-9) and
/// two Newton steps on the distribution function.
fn compute_normal_quantile(alpha: f64) -> f64 {
    let mut z = compute_inverse_normal_cdf(alpha);
    for _ in 0..2 {
        z -= (compute_normal_cdf(z) - alpha) / compute_normal_pdf(z);
    }
    z
}

fn check_confidence(alpha: f64) -> Result<(), Error> {
    if !(alpha > 0.0 && alpha < 1.0) {
        return invalid(format!("the confidence level must be strictly between 0 and 1, not {alpha}"));
    }
    Ok(())
}

// ---- risk measures, generic for automatic differentiation -------------------------------------------------

fn portfolio_volatility<T: Operand>(w: &[T], sigma: &[Vec<f64>]) -> T {
    let mut v = w[0].like(0.0);
    for (i, row) in sigma.iter().enumerate() {
        let mut s = w[0].like(0.0);
        for (j, sij) in row.iter().enumerate() {
            s = s + w[j] * *sij;
        }
        v = v + w[i] * s;
    }
    v.sqrt()
}

fn portfolio_mean<T: Operand>(w: &[T], mu: &[f64]) -> T {
    w.iter().zip(mu).fold(w[0].like(0.0), |s, (wi, mi)| s + *wi * *mi)
}

/// Rockafellar-Uryasev expected shortfall: the mean of the largest `(1 - alpha) T` losses, the last
/// one counted in part. The order of the losses is taken from the values.
fn historical_es<T: Operand>(w: &[T], returns: &ReturnMatrix, alpha: f64) -> T {
    let nt = returns.periods();
    let losses: Vec<T> = (0..nt).map(|t| -portfolio_mean(w, returns.row(t))).collect();
    let mut order: Vec<usize> = (0..nt).collect();
    order.sort_by(|&a, &b| losses[b].primal().total_cmp(&losses[a].primal()).then(a.cmp(&b)));
    let k = (1.0 - alpha) * nt as f64;
    let whole = k.floor() as usize;
    let mut sum = w[0].like(0.0);
    for &t in order.iter().take(whole) {
        sum = sum + losses[t];
    }
    if whole < nt && k > whole as f64 {
        sum = sum + losses[order[whole]] * (k - whole as f64);
    }
    sum / k
}

pub fn compute_historical_es(returns: &ReturnMatrix, weights: &[f64], alpha: f64) -> f64 {
    historical_es(weights, returns, alpha)
}

/// Risk measures of a portfolio and their Euler contributions (`w_i dR/dw_i`, by automatic
/// differentiation; they add up to the measure).
#[derive(Debug, Clone, PartialEq)]
pub struct PortfolioRisk {
    pub expected_return: f64,
    pub volatility: f64,
    /// `-mu_p + z_alpha sigma_p`.
    pub normal_var: f64,
    /// `-mu_p + phi(z_alpha) / (1 - alpha) sigma_p`.
    pub normal_es: f64,
    /// R's `quantile(type = 7)` of the losses.
    pub historical_var: f64,
    /// Rockafellar-Uryasev: the mean of the largest `(1 - alpha) T` losses, the last in part.
    pub historical_es: f64,
    /// The mean of the losses at or above `historical_var` (GaussQuant's definition).
    pub tail_mean_es: f64,
    pub volatility_contribution: Vec<f64>,
    pub normal_var_contribution: Vec<f64>,
    pub normal_es_contribution: Vec<f64>,
    pub historical_es_contribution: Vec<f64>,
}

pub fn compute_portfolio_risk(returns: &ReturnMatrix, weights: &[f64], confidence_level: f64) -> Result<PortfolioRisk, Error> {
    let m = compute_return_moments(returns)?;
    check_confidence(confidence_level)?;
    let n = returns.assets;
    if weights.len() != n || weights.iter().any(|w| !w.is_finite()) {
        return invalid(format!("the weights must be {n} finite numbers"));
    }
    let alpha = confidence_level;
    let z = compute_normal_quantile(alpha);
    let k_es = compute_normal_pdf(z) / (1.0 - alpha);
    let euler = |gradient: Vec<f64>| -> Vec<f64> { gradient.iter().zip(weights).map(|(g, w)| g * w).collect() };
    let sigma = &m.covariance;
    let mu = &m.mean;
    let (volatility, g_vol) = compute_value_and_gradient(weights, |w| Ok(portfolio_volatility(w, sigma)))?;
    let (normal_var, g_var) = compute_value_and_gradient(weights, |w| Ok(-portfolio_mean(w, mu) + portfolio_volatility(w, sigma) * z))?;
    let (normal_es, g_es) = compute_value_and_gradient(weights, |w| Ok(-portfolio_mean(w, mu) + portfolio_volatility(w, sigma) * k_es))?;
    let (historical_es, g_hes) = compute_value_and_gradient(weights, |w| Ok(historical_es(w, returns, alpha)))?;

    let nt = returns.periods();
    let mut losses: Vec<f64> = (0..nt).map(|t| -dot(returns.row(t), weights)).collect();
    losses.sort_by(f64::total_cmp);
    let h = (nt - 1) as f64 * alpha;
    let lo = h.floor() as usize;
    let historical_var = if lo + 1 < nt { losses[lo] + (h - lo as f64) * (losses[lo + 1] - losses[lo]) } else { losses[nt - 1] };
    let tail: Vec<f64> = losses.iter().copied().filter(|l| *l >= historical_var).collect();
    let tail_mean_es = if tail.is_empty() { historical_var } else { tail.iter().sum::<f64>() / tail.len() as f64 };

    if volatility == 0.0 {
        return invalid("the portfolio has no variance, so its contributions are not defined");
    }
    Ok(PortfolioRisk {
        expected_return: dot(mu, weights),
        volatility,
        normal_var,
        normal_es,
        historical_var,
        historical_es,
        tail_mean_es,
        volatility_contribution: euler(g_vol),
        normal_var_contribution: euler(g_var),
        normal_es_contribution: euler(g_es),
        historical_es_contribution: euler(g_hes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn returns() -> Vec<f64> {
        // three assets, eight periods
        vec![
            0.01, 0.02, -0.01, 0.03, -0.01, 0.02, -0.02, 0.01, 0.00, 0.02, 0.03, -0.01, 0.00, -0.02, 0.01, 0.01, 0.01, 0.02,
            -0.03, 0.00, -0.02, 0.02, 0.01, 0.00,
        ]
    }

    #[test]
    fn contributions_add_up_to_the_measures() {
        let r = returns();
        let rm = ReturnMatrix { values: &r, assets: 3 };
        let risk = compute_portfolio_risk(&rm, &[0.5, 0.3, 0.2], 0.9).unwrap();
        for (total, parts) in [
            (risk.volatility, &risk.volatility_contribution),
            (risk.normal_var, &risk.normal_var_contribution),
            (risk.normal_es, &risk.normal_es_contribution),
            (risk.historical_es, &risk.historical_es_contribution),
        ] {
            assert!((parts.iter().sum::<f64>() - total).abs() < 1e-15, "{total} {parts:?}");
        }
    }

    #[test]
    fn the_long_only_minimum_variance_meets_its_constraints() {
        let r = returns();
        let rm = ReturnMatrix { values: &r, assets: 3 };
        let c = PortfolioConstraints { weight_sum: 1.0, lower: vec![0.0; 3], upper: vec![0.6; 3] };
        let p = compute_optimal_portfolio(&rm, &c, &PortfolioMethod::MinimumVariance).unwrap();
        assert!((p.weights.iter().sum::<f64>() - 1.0).abs() < 1e-14);
        assert!(p.weights.iter().all(|w| *w >= -1e-14 && *w <= 0.6 + 1e-14));
        assert!(p.stationarity_residual < 1e-14);
    }
}
