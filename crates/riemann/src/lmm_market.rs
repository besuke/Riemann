//! The lognormal LIBOR (forward) market model with joint paths under the spot measure, the
//! Rebonato approximation of swaption volatilities, and the calibration of an exponential
//! correlation to swaptions (LagrangeFinance II-7.4 and II-7.6, extended).
//!
//! The tenor structure is `T_0 < T_1 < ... < T_n` (years from today); the forward `L_i` covers
//! `[T_i, T_{i+1}]` with accrual `tau_i = T_{i+1} - T_i`, fixes at `T_i` and pays at `T_{i+1}`. Each forward
//! has a constant volatility `sigma_i`, and the Brownian motions are correlated by `rho`. Under the
//! spot measure, whose numeraire rolls the money from one tenor date to the next,
//!
//! `dL_k / L_k = sigma_k sum_{j = eta(t)}^{k} tau_j rho_kj sigma_j L_j / (1 + tau_j L_j) dt + sigma_k dW_k`,
//!
//! where `eta(t)` is the first forward not yet fixed (Glasserman, *Monte Carlo Methods in Financial
//! Engineering*, 3.7). The paths take log-Euler steps with the drift at the start of each step; a few
//! steps per period make the discretization error small. The numeraire is `B(T_0) = 1 / P(0, T_0)` and
//! `B(T_{k+1}) = B(T_k) (1 + tau_k L_k(T_k))`.
//!
//! The rates are generic forward-looking term rates. This is a model of term forwards, not an IBOR
//! index: no fixings, no index curve (see `docs/quantlib_guide_inventory.md` on IBOR).

// `!(x > 0.0)` is intended: it also rejects NaN
#![allow(clippy::neg_cmp_op_on_partial_ord)]
// the forwards, accruals, volatilities and correlations share one index
#![allow(clippy::needless_range_loop)]

use crate::lmm::check_correlation;
use crate::math::random::generate_standard_normal;
use crate::options::{compute_forward_value, ForwardInputs, ForwardModel, OptionType};
use crate::Error;

fn invalid<T>(reason: String) -> Result<T, Error> {
    Err(Error::InvalidInput { reason })
}

/// The model: tenor dates, forwards today, volatilities and correlation.
#[derive(Debug, Clone, PartialEq)]
pub struct LmmSpec {
    /// `T_0, ..., T_n` in years from today, increasing, `T_0 >= 0`.
    pub times: Vec<f64>,
    /// `L_0(0), ..., L_{n-1}(0)`.
    pub forwards: Vec<f64>,
    /// Constant lognormal volatilities `sigma_0, ..., sigma_{n-1}`.
    pub volatilities: Vec<f64>,
    /// `n` by `n`, by rows.
    pub correlation: Vec<f64>,
    /// `P(0, T_0)`.
    pub discount_start: f64,
}

impl LmmSpec {
    fn count(&self) -> usize {
        self.forwards.len()
    }

    fn accrual(&self, i: usize) -> f64 {
        self.times[i + 1] - self.times[i]
    }

    fn check(&self) -> Result<Vec<Vec<f64>>, Error> {
        let n = self.count();
        if n == 0 || self.times.len() != n + 1 || self.volatilities.len() != n {
            return invalid(format!("give n + 1 times and n forwards and volatilities (n = {n})"));
        }
        if !(self.times[0] >= 0.0) || self.times.windows(2).any(|w| !(w[1] > w[0])) {
            return invalid("the times must start at or after 0 and increase".into());
        }
        if self.forwards.iter().any(|f| !(*f > 0.0)) || self.volatilities.iter().any(|v| !(*v >= 0.0) || !v.is_finite()) {
            return invalid("the forwards must be positive and the volatilities finite and non-negative".into());
        }
        if !(self.discount_start > 0.0) {
            return invalid("the discount factor to the first time must be positive".into());
        }
        check_correlation(&self.correlation, n)
    }

    /// `P(0, T_k)` from the forwards.
    pub fn discount(&self, k: usize) -> f64 {
        (0..k).fold(self.discount_start, |p, i| p / (1.0 + self.accrual(i) * self.forwards[i]))
    }
}

/// An exponential correlation `rho_ij = exp(-beta |T_i - T_j|)` on the forwards' fixing times.
pub fn create_exponential_correlation(times: &[f64], beta: f64) -> Result<Vec<f64>, Error> {
    if !(beta >= 0.0) || !beta.is_finite() {
        return invalid(format!("beta must be finite and non-negative, not {beta}"));
    }
    Ok(times.iter().flat_map(|&a| times.iter().map(move |&b| (-beta * (a - b).abs()).exp())).collect())
}

/// Joint paths: `state[p][k][i]` is `L_i(T_k)` on path `p` for `i >= k` (NaN for fixed forwards),
/// and `numeraire[p][k]` is `B(T_k)`.
#[derive(Debug, Clone, PartialEq)]
pub struct LmmPaths {
    pub state: Vec<Vec<Vec<f64>>>,
    pub numeraire: Vec<Vec<f64>>,
}

/// Simulates `paths` paths with `steps_per_period` log-Euler steps in each period (and from 0 to
/// `T_0`). The normal variate of path `p`, step `s` and forward `i` is the library's counter-based
/// draw for `(seed, p, s n + i)`, correlated by the Cholesky factor of `rho`.
pub fn generate_lmm_paths(spec: &LmmSpec, paths: usize, steps_per_period: usize, seed: u64) -> Result<LmmPaths, Error> {
    let lower = spec.check()?;
    if paths == 0 || steps_per_period == 0 {
        return invalid("give at least one path and one step per period".into());
    }
    let n = spec.count();
    if paths.saturating_mul(n).saturating_mul(n + 1) > 200_000_000 {
        return invalid(format!("{paths} paths of {n} forwards are too many to keep"));
    }
    let mut state = Vec::with_capacity(paths);
    let mut numeraire = Vec::with_capacity(paths);
    for p in 0..paths {
        let mut l = spec.forwards.clone();
        let mut path_state = vec![vec![f64::NAN; n]; n + 1];
        let mut path_numeraire = vec![0.0; n + 1];
        path_numeraire[0] = 1.0 / spec.discount_start;
        let mut step_count: u64 = 0;
        let mut evolve = |l: &mut Vec<f64>, from: f64, to: f64, alive: usize| {
            let dt = (to - from) / steps_per_period as f64;
            for _ in 0..steps_per_period {
                let z: Vec<f64> = (0..n).map(|i| generate_standard_normal(seed, p as u64, step_count * n as u64 + i as u64)).collect();
                step_count += 1;
                let w: Vec<f64> = (0..n).map(|i| lower[i][..=i].iter().zip(&z).map(|(a, b)| a * b).sum()).collect();
                let snapshot = l.clone();
                let drift = drift_of(spec, alive, &snapshot);
                for k in alive..n {
                    let sigma = spec.volatilities[k];
                    l[k] = snapshot[k] * ((drift[k] - 0.5 * sigma * sigma) * dt + sigma * dt.sqrt() * w[k]).exp();
                }
            }
        };
        if spec.times[0] > 0.0 {
            evolve(&mut l, 0.0, spec.times[0], 0);
        }
        path_state[0].clone_from(&l);
        for k in 0..n {
            path_numeraire[k + 1] = path_numeraire[k] * (1.0 + spec.accrual(k) * l[k]);
            if k + 1 < n {
                evolve(&mut l, spec.times[k], spec.times[k + 1], k + 1);
                path_state[k + 1][(k + 1)..n].copy_from_slice(&l[(k + 1)..n]);
            }
        }
        state.push(path_state);
        numeraire.push(path_numeraire);
    }
    Ok(LmmPaths { state, numeraire })
}

/// A Monte Carlo value with its standard error.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LmmEstimate {
    pub value: f64,
    pub standard_error: f64,
}

fn estimate(values: &[f64]) -> LmmEstimate {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / (n - 1.0).max(1.0);
    LmmEstimate { value: mean, standard_error: (variance / n).sqrt() }
}

/// The swap rate from `T_a` to `T_b` and its annuity, at `T_a` on a path or today (`k` the index of
/// the time, `forwards` the forwards then, from index `a`).
/// The drift of `ln L_k` under the spot measure without the `-sigma_k^2 / 2` term, per year, for the
/// forwards from `alive` (the first not yet fixed) on: `sigma_k sum_{j = alive}^{k} tau_j rho_kj sigma_j L_j
/// / (1 + tau_j L_j)`. Entries before `alive` are 0 (QuantLib `LMMDriftCalculator::computePlain` with the
/// numeraire at `alive`).
fn drift_of(spec: &LmmSpec, alive: usize, forwards: &[f64]) -> Vec<f64> {
    let n = spec.count();
    let mut drift = vec![0.0; n];
    for k in alive..n {
        let mut sum = 0.0;
        for j in alive..=k {
            let tau = spec.accrual(j);
            sum += tau * spec.correlation[k * n + j] * spec.volatilities[j] * forwards[j] / (1.0 + tau * forwards[j]);
        }
        drift[k] = spec.volatilities[k] * sum;
    }
    drift
}

/// The spot-measure drift for the forwards `forwards` (see the module documentation).
pub fn compute_lmm_drift(spec: &LmmSpec, alive: usize, forwards: &[f64]) -> Result<Vec<f64>, Error> {
    spec.check()?;
    if forwards.len() != spec.count() || alive >= spec.count() {
        return invalid(format!("give {} forwards and a first alive index below it", spec.count()));
    }
    Ok(drift_of(spec, alive, forwards))
}

/// The swap rate from `T_a` to `T_b` and its annuity per unit discount at `T_a`, for given forwards.
pub fn compute_lmm_swap_rate(spec: &LmmSpec, a: usize, b: usize, forwards: &[f64]) -> Result<(f64, f64), Error> {
    spec.check()?;
    if !(a < b && b <= spec.count()) || forwards.len() != spec.count() {
        return invalid(format!("the swap must run from T_a to T_b with a < b <= {}", spec.count()));
    }
    Ok(swap_rate_and_annuity(spec, a, b, forwards))
}

fn swap_rate_and_annuity(spec: &LmmSpec, a: usize, b: usize, forwards: &[f64]) -> (f64, f64) {
    let (mut discount, mut annuity) = (1.0, 0.0);
    for i in a..b {
        discount /= 1.0 + spec.accrual(i) * forwards[i];
        annuity += spec.accrual(i) * discount;
    }
    ((1.0 - discount) / annuity, annuity)
}

/// A payer swaption exercised at `T_a` into the swap paying `strike` and receiving the forwards from
/// `T_a` to `T_b`, per unit notional, on the paths.
pub fn compute_lmm_swaption_value(spec: &LmmSpec, paths: &LmmPaths, a: usize, b: usize, strike: f64) -> Result<LmmEstimate, Error> {
    spec.check()?;
    let n = spec.count();
    if !(a < b && b <= n) {
        return invalid(format!("the swap must run from T_a to T_b with a < b <= {n}"));
    }
    let values: Vec<f64> = paths
        .state
        .iter()
        .zip(&paths.numeraire)
        .map(|(state, numeraire)| {
            let (rate, annuity) = swap_rate_and_annuity(spec, a, b, &state[a]);
            annuity * (rate - strike).max(0.0) / numeraire[a]
        })
        .collect();
    Ok(estimate(&values))
}

/// A caplet on `L_i` paid at `T_{i+1}`, per unit notional, on the paths.
pub fn compute_lmm_caplet_value(spec: &LmmSpec, paths: &LmmPaths, i: usize, strike: f64) -> Result<LmmEstimate, Error> {
    spec.check()?;
    if i >= spec.count() {
        return invalid(format!("no forward {i}"));
    }
    let tau = spec.accrual(i);
    let values: Vec<f64> = paths
        .state
        .iter()
        .zip(&paths.numeraire)
        .map(|(state, numeraire)| tau * (state[i][i] - strike).max(0.0) / numeraire[i + 1])
        .collect();
    Ok(estimate(&values))
}

/// Rebonato's approximation of the Black volatility of the swaption from `T_a` to `T_b`, with the
/// weights frozen today: `v^2 T_a S^2 = sum_ij w_i w_j L_i L_j rho_ij sigma_i sigma_j T_a`.
pub fn compute_rebonato_swaption_volatility(spec: &LmmSpec, a: usize, b: usize) -> Result<f64, Error> {
    spec.check()?;
    let n = spec.count();
    if !(a < b && b <= n) {
        return invalid(format!("the swap must run from T_a to T_b with a < b <= {n}"));
    }
    let (rate, annuity) = swap_rate_and_annuity(spec, a, b, &spec.forwards);
    let mut discount = 1.0;
    let weights: Vec<f64> = (a..b)
        .map(|i| {
            discount /= 1.0 + spec.accrual(i) * spec.forwards[i];
            spec.accrual(i) * discount / annuity
        })
        .collect();
    let mut sum = 0.0;
    for i in a..b {
        for j in a..b {
            sum += weights[i - a] * weights[j - a] * spec.forwards[i] * spec.forwards[j] * spec.correlation[i * n + j] * spec.volatilities[i] * spec.volatilities[j];
        }
    }
    Ok(sum.sqrt() / rate)
}

/// The Black value of the swaption at a volatility, per unit notional, today.
pub fn compute_black_swaption_value(spec: &LmmSpec, a: usize, b: usize, strike: f64, volatility: f64) -> Result<f64, Error> {
    spec.check()?;
    let (rate, annuity) = swap_rate_and_annuity(spec, a, b, &spec.forwards);
    let value = compute_forward_value(
        ForwardModel::Black,
        OptionType::Call,
        &ForwardInputs { forward: rate, strike, volatility, time: spec.times[a], discount: 1.0 },
    )?;
    Ok(spec.discount(a) * annuity * value)
}

/// A swaption volatility to fit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwaptionQuote {
    pub start: usize,
    pub end: usize,
    pub volatility: f64,
}

/// The fit of the correlation.
#[derive(Debug, Clone, PartialEq)]
pub struct CorrelationFit {
    pub beta: f64,
    /// Rebonato volatilities at the fitted `beta`, in the order of the quotes.
    pub fitted: Vec<f64>,
    pub root_mean_square_error: f64,
}

/// Fits `beta` of the exponential correlation to swaption volatilities by least squares on the
/// Rebonato volatilities, with the forwards' volatilities fixed (from the caplets: with a constant
/// volatility per forward, `sigma_i` is the Black volatility of the caplet on `L_i`). The search is a
/// golden section on `[0, beta_max]`.
pub fn calibrate_lmm_correlation(spec: &LmmSpec, quotes: &[SwaptionQuote], beta_max: f64) -> Result<CorrelationFit, Error> {
    if quotes.is_empty() {
        return invalid("no swaption volatilities given".into());
    }
    if !(beta_max > 0.0) || !beta_max.is_finite() {
        return invalid("beta_max must be positive and finite".into());
    }
    let fixing_times = &spec.times[..spec.count()];
    let fitted_at = |beta: f64| -> Result<Vec<f64>, Error> {
        let trial = LmmSpec { correlation: create_exponential_correlation(fixing_times, beta)?, ..spec.clone() };
        quotes.iter().map(|q| compute_rebonato_swaption_volatility(&trial, q.start, q.end)).collect()
    };
    let error_at = |beta: f64| -> Result<f64, Error> {
        Ok(fitted_at(beta)?.iter().zip(quotes).map(|(f, q)| (f - q.volatility) * (f - q.volatility)).sum())
    };
    let ratio = (5f64.sqrt() - 1.0) / 2.0;
    let (mut lo, mut hi) = (0.0, beta_max);
    let mut x1 = hi - ratio * (hi - lo);
    let mut x2 = lo + ratio * (hi - lo);
    let (mut f1, mut f2) = (error_at(x1)?, error_at(x2)?);
    for _ in 0..200 {
        if hi - lo < 1e-12 * beta_max.max(1.0) {
            break;
        }
        if f1 <= f2 {
            hi = x2;
            x2 = x1;
            f2 = f1;
            x1 = hi - ratio * (hi - lo);
            f1 = error_at(x1)?;
        } else {
            lo = x1;
            x1 = x2;
            f1 = f2;
            x2 = lo + ratio * (hi - lo);
            f2 = error_at(x2)?;
        }
    }
    let beta = (lo + hi) / 2.0;
    let fitted = fitted_at(beta)?;
    let root_mean_square_error = (error_at(beta)? / quotes.len() as f64).sqrt();
    Ok(CorrelationFit { beta, fitted, root_mean_square_error })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lmm::{compute_caplet_value, CapletTerms};

    fn spec(beta: f64) -> LmmSpec {
        let times: Vec<f64> = (0..=10).map(|k| 1.0 + 0.5 * k as f64).collect();
        let forwards: Vec<f64> = (0..10).map(|k| 0.02 + 0.001 * k as f64).collect();
        let volatilities: Vec<f64> = (0..10).map(|k| 0.25 - 0.005 * k as f64).collect();
        let correlation = create_exponential_correlation(&times[..10], beta).unwrap();
        LmmSpec { times, forwards, volatilities, correlation, discount_start: (-0.02f64).exp() }
    }

    const PATHS: usize = 2000;

    #[test]
    fn discounted_zero_bonds_are_martingales_and_caplets_follow_black() {
        let s = spec(0.2);
        let paths = generate_lmm_paths(&s, PATHS, 4, 11).unwrap();
        // E[1 / B(T_k)] = P(0, T_k): no arbitrage under the spot measure
        for k in [1usize, 5, 10] {
            let values: Vec<f64> = paths.numeraire.iter().map(|b| 1.0 / b[k]).collect();
            let e = estimate(&values);
            assert!((e.value - s.discount(k)).abs() < 4.0 * e.standard_error + 1e-12, "P(0,T_{k}): {} against {} ± {}", s.discount(k), e.value, e.standard_error);
        }
        // each caplet is Black's formula under its own forward measure
        for i in [0usize, 4, 9] {
            let strike = s.forwards[i];
            let mc = compute_lmm_caplet_value(&s, &paths, i, strike).unwrap();
            let black = compute_caplet_value(
                ForwardModel::Black,
                &CapletTerms {
                    forward: s.forwards[i],
                    strike,
                    volatility: s.volatilities[i],
                    expiry: s.times[i],
                    accrual: s.accrual(i),
                    discount_factor: s.discount(i + 1),
                    notional: 1.0,
                },
            )
            .unwrap();
            assert!((mc.value - black).abs() < 4.0 * mc.standard_error, "caplet {i}: {black} against {} ± {}", mc.value, mc.standard_error);
        }
    }

    #[test]
    fn rebonato_is_close_to_the_simulated_swaption_and_the_calibration_recovers_beta() {
        let s = spec(0.3);
        let paths = generate_lmm_paths(&s, PATHS, 4, 13).unwrap();
        for (a, b) in [(2usize, 6usize), (4, 10)] {
            let (rate, _) = swap_rate_and_annuity(&s, a, b, &s.forwards);
            let mc = compute_lmm_swaption_value(&s, &paths, a, b, rate).unwrap();
            let approx = compute_black_swaption_value(&s, a, b, rate, compute_rebonato_swaption_volatility(&s, a, b).unwrap()).unwrap();
            // Rebonato is an approximation: allow the simulation error and 2% of the value
            assert!((mc.value - approx).abs() < 4.0 * mc.standard_error + 0.02 * approx, "{a}x{b}: {approx} against {} ± {}", mc.value, mc.standard_error);
        }
        // quotes made at beta = 0.3 give back 0.3
        let quotes: Vec<SwaptionQuote> = [(2usize, 6usize), (4, 10), (1, 8), (6, 10)]
            .iter()
            .map(|&(a, b)| SwaptionQuote { start: a, end: b, volatility: compute_rebonato_swaption_volatility(&s, a, b).unwrap() })
            .collect();
        let fit = calibrate_lmm_correlation(&spec(0.9), &quotes, 5.0).unwrap();
        assert!((fit.beta - 0.3).abs() < 1e-6, "{}", fit.beta);
        assert!(fit.root_mean_square_error < 1e-9);
    }

    #[test]
    fn invalid_inputs_are_errors() {
        let mut s = spec(0.2);
        assert!(generate_lmm_paths(&s, 0, 4, 1).is_err());
        assert!(compute_rebonato_swaption_volatility(&s, 5, 5).is_err());
        assert!(create_exponential_correlation(&s.times, -1.0).is_err());
        s.forwards[3] = -0.01;
        assert!(generate_lmm_paths(&s, 10, 4, 1).is_err());
    }
}
