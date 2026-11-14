//! Market VaR and expected shortfall of a loss series, by the variance-covariance (Gaussian), historical and
//! Monte Carlo methods, and their backtests (LagrangeFinance I-9; `docs/spec_market_risk.md`).
//!
//! Losses are positive: `L_t = -R_t`. Quantiles follow R's `quantile()`: type 7 (linear interpolation, R's
//! default and the manuscript's) and type 1 (the inverse of the empirical distribution). Expected shortfall is
//! given in two definitions: the mean of the losses at or above the type-7 VaR (the manuscript's I-9.8) and
//! Rockafellar-Uryasev's, the mean of the largest `(1 - alpha) T` losses with the last counted in part.
//!
//! Monte Carlo scenarios use counter-based random numbers (`math::random`), so the result depends on the seed
//! and not on the number of threads; its standard errors come from batch means.

// `!(x > 0.0)` is intended: it also rejects NaN
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::batch::map_parallel;
use crate::math::linear::compute_cholesky;
use crate::math::normal::{compute_normal_cdf, compute_normal_pdf, compute_normal_quantile};
use crate::math::random::generate_standard_normal;
use crate::Error;

fn invalid<T>(reason: impl Into<String>) -> Result<T, Error> {
    Err(Error::InvalidInput { reason: reason.into() })
}

fn check_confidence(alpha: f64) -> Result<(), Error> {
    if !(alpha > 0.0 && alpha < 1.0) {
        return invalid(format!("the confidence level must be strictly between 0 and 1, not {alpha}"));
    }
    Ok(())
}

fn check_losses(losses: &[f64], least: usize) -> Result<(), Error> {
    if losses.len() < least {
        return invalid(format!("at least {least} losses are needed"));
    }
    if losses.iter().any(|v| !v.is_finite()) {
        return invalid("the losses must be finite (remove the missing periods first)");
    }
    Ok(())
}

/// R's quantile types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantileType {
    /// The inverse of the empirical distribution function.
    Type1,
    /// Linear interpolation between order statistics (R's default).
    Type7,
}

/// The `p`-quantile of values already sorted in increasing order.
pub fn compute_sorted_quantile(sorted: &[f64], p: f64, kind: QuantileType) -> f64 {
    let n = sorted.len();
    match kind {
        QuantileType::Type1 => {
            let k = ((n as f64) * p).ceil().max(1.0) as usize;
            sorted[k.min(n) - 1]
        }
        QuantileType::Type7 => {
            let h = (n - 1) as f64 * p;
            let lo = h.floor() as usize;
            if lo + 1 >= n {
                sorted[n - 1]
            } else {
                sorted[lo] + (h - lo as f64) * (sorted[lo + 1] - sorted[lo])
            }
        }
    }
}

/// Rockafellar-Uryasev expected shortfall of losses sorted in increasing order.
fn ru_es_sorted(sorted: &[f64], alpha: f64) -> f64 {
    let n = sorted.len();
    let k = (1.0 - alpha) * n as f64;
    let whole = k.floor() as usize;
    let mut sum: f64 = sorted.iter().rev().take(whole).sum();
    if whole < n && k > whole as f64 {
        sum += (k - whole as f64) * sorted[n - 1 - whole];
    }
    sum / k
}

fn mean_sd(x: &[f64]) -> (f64, f64) {
    let n = x.len() as f64;
    let mean = x.iter().sum::<f64>() / n;
    let var = x.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0);
    (mean, var.sqrt())
}

/// VaR and ES of a loss series at one confidence level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LossRisk {
    pub confidence_level: f64,
    /// `mu_L + z_alpha sd_L` (the mean and sample standard deviation of the losses).
    pub gaussian_var: f64,
    /// `mu_L + phi(z_alpha) / (1 - alpha) sd_L`.
    pub gaussian_es: f64,
    /// Type 7 (the manuscript).
    pub historical_var: f64,
    /// Type 1.
    pub historical_var_type1: f64,
    /// The mean of the losses at or above `historical_var` (the manuscript).
    pub tail_mean_es: f64,
    /// Rockafellar-Uryasev.
    pub historical_es: f64,
}

pub fn compute_loss_risk(losses: &[f64], confidence_level: f64) -> Result<LossRisk, Error> {
    check_losses(losses, 2)?;
    check_confidence(confidence_level)?;
    let alpha = confidence_level;
    let (mean, sd) = mean_sd(losses);
    let z = compute_normal_quantile(alpha);
    let mut sorted = losses.to_vec();
    sorted.sort_by(f64::total_cmp);
    let historical_var = compute_sorted_quantile(&sorted, alpha, QuantileType::Type7);
    let tail: Vec<f64> = sorted.iter().copied().filter(|l| *l >= historical_var).collect();
    Ok(LossRisk {
        confidence_level: alpha,
        gaussian_var: mean + z * sd,
        gaussian_es: mean + compute_normal_pdf(z) / (1.0 - alpha) * sd,
        historical_var,
        historical_var_type1: compute_sorted_quantile(&sorted, alpha, QuantileType::Type1),
        tail_mean_es: if tail.is_empty() { historical_var } else { tail.iter().sum::<f64>() / tail.len() as f64 },
        historical_es: ru_es_sorted(&sorted, alpha),
    })
}

/// Monte Carlo VaR and ES of a portfolio with normal returns `N(mean, covariance)`.
#[derive(Debug, Clone, PartialEq)]
pub struct MonteCarloSettings {
    pub scenarios: usize,
    pub seed: u64,
    /// Batches for the standard errors (batch means); the scenarios are split evenly.
    pub batches: usize,
    /// Worker threads; the results do not depend on them. 0 means one per logical processor.
    pub threads: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonteCarloRisk {
    pub confidence_level: f64,
    /// Type-7 quantile of the simulated losses.
    pub var: f64,
    /// Rockafellar-Uryasev ES of the simulated losses.
    pub es: f64,
    /// Standard deviations of the batches' VaR and ES over the square root of the number of batches.
    pub var_standard_error: f64,
    pub es_standard_error: f64,
    pub scenarios: usize,
}

/// Losses `-w'(mu + L z)` of the scenarios `first..first + count`, with `z_d` drawn for path and dimension `d`.
fn simulate_losses(w: &[f64], mean: &[f64], chol: &[Vec<f64>], seed: u64, first: usize, count: usize) -> Vec<f64> {
    let n = mean.len();
    // w'L, so that a loss is -(w'mu + (w'L) z)
    let wl: Vec<f64> = (0..n).map(|k| (k..n).map(|i| w[i] * chol[i][k]).sum()).collect();
    let wmu: f64 = w.iter().zip(mean).map(|(a, b)| a * b).sum();
    (first..first + count)
        .map(|p| {
            let s: f64 = (0..n).map(|d| wl[d] * generate_standard_normal(seed, p as u64, d as u64)).sum();
            -(wmu + s)
        })
        .collect()
}

pub fn compute_monte_carlo_risk(weights: &[f64], mean: &[f64], covariance: &[Vec<f64>], confidence_level: f64, settings: &MonteCarloSettings) -> Result<MonteCarloRisk, Error> {
    check_confidence(confidence_level)?;
    let n = mean.len();
    if n == 0 || weights.len() != n || covariance.len() != n || covariance.iter().any(|r| r.len() != n) {
        return invalid("the weights, mean and covariance must have one entry per asset");
    }
    if weights.iter().chain(mean).chain(covariance.iter().flatten()).any(|v| !v.is_finite()) {
        return invalid("the weights, mean and covariance must be finite");
    }
    let MonteCarloSettings { scenarios, seed, batches, threads } = *settings;
    if batches < 2 || scenarios < 2 * batches || scenarios % batches != 0 {
        return invalid("the scenarios must split evenly into at least two batches of at least two scenarios");
    }
    let chol = compute_cholesky(covariance).map_err(|_| Error::InvalidInput { reason: "the covariance matrix is not positive definite".into() })?;
    let per = scenarios / batches;
    let alpha = confidence_level;
    let ids: Vec<usize> = (0..batches).collect();
    let mut batch_losses = map_parallel(&ids, threads, |&b| {
        let mut l = simulate_losses(weights, mean, &chol, seed, b * per, per);
        l.sort_by(f64::total_cmp);
        l
    })?;
    let estimates: Vec<(f64, f64)> = batch_losses.iter().map(|l| (compute_sorted_quantile(l, alpha, QuantileType::Type7), ru_es_sorted(l, alpha))).collect();
    let mut all: Vec<f64> = batch_losses.drain(..).flatten().collect();
    all.sort_by(f64::total_cmp);
    let se = |xs: Vec<f64>| mean_sd(&xs).1 / (batches as f64).sqrt();
    Ok(MonteCarloRisk {
        confidence_level: alpha,
        var: compute_sorted_quantile(&all, alpha, QuantileType::Type7),
        es: ru_es_sorted(&all, alpha),
        var_standard_error: se(estimates.iter().map(|e| e.0).collect()),
        es_standard_error: se(estimates.iter().map(|e| e.1).collect()),
        scenarios,
    })
}

/// Exceptions `L_t > VaR_t` and the tests on them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VarBacktest {
    pub observations: usize,
    pub exceptions: usize,
    pub exception_rate: f64,
    pub expected_rate: f64,
    /// Kupiec's proportion of failures: likelihood ratio and p-value (chi-squared, 1 degree of freedom).
    pub kupiec_lr: f64,
    pub kupiec_p_value: f64,
    /// Transitions between days without (0) and with (1) an exception.
    pub n00: usize,
    pub n01: usize,
    pub n10: usize,
    pub n11: usize,
    /// Christoffersen's independence test (1 degree of freedom) and conditional coverage (2).
    pub independence_lr: f64,
    pub independence_p_value: f64,
    pub conditional_coverage_lr: f64,
    pub conditional_coverage_p_value: f64,
}

/// `x ln y`, zero when `x` is zero.
fn xlogy(x: f64, y: f64) -> f64 {
    if x == 0.0 {
        0.0
    } else {
        x * y.ln()
    }
}

/// Upper tail of the chi-squared distribution with 1 or 2 degrees of freedom.
fn chi_squared_upper(x: f64, df: u32) -> f64 {
    let x = x.max(0.0);
    match df {
        1 => 2.0 * compute_normal_cdf(-x.sqrt()),
        2 => (-0.5 * x).exp(),
        _ => unreachable!("only 1 and 2 degrees of freedom are used"),
    }
}

/// `var` has one value per loss, or one value for all.
pub fn compute_var_backtest(losses: &[f64], var: &[f64], confidence_level: f64) -> Result<VarBacktest, Error> {
    check_losses(losses, 2)?;
    check_confidence(confidence_level)?;
    if !(var.len() == 1 || var.len() == losses.len()) || var.iter().any(|v| !v.is_finite()) {
        return invalid("the VaR must be one finite number, or one per loss");
    }
    let hit: Vec<bool> = losses.iter().enumerate().map(|(t, l)| *l > var[if var.len() == 1 { 0 } else { t }]).collect();
    let n = hit.len();
    let x = hit.iter().filter(|h| **h).count();
    let p = 1.0 - confidence_level;
    let (nf, xf) = (n as f64, x as f64);
    let kupiec_lr = -2.0 * (xlogy(nf - xf, 1.0 - p) + xlogy(xf, p)) + 2.0 * (xlogy(nf - xf, 1.0 - xf / nf) + xlogy(xf, xf / nf));
    let (mut n00, mut n01, mut n10, mut n11) = (0usize, 0usize, 0usize, 0usize);
    for pair in hit.windows(2) {
        match (pair[0], pair[1]) {
            (false, false) => n00 += 1,
            (false, true) => n01 += 1,
            (true, false) => n10 += 1,
            (true, true) => n11 += 1,
        }
    }
    let f = |v: usize| v as f64;
    let pi0 = if n00 + n01 > 0 { f(n01) / f(n00 + n01) } else { 0.0 };
    let pi1 = if n10 + n11 > 0 { f(n11) / f(n10 + n11) } else { 0.0 };
    let pi = f(n01 + n11) / f(n - 1);
    let independence_lr = -2.0 * (xlogy(f(n00 + n10), 1.0 - pi) + xlogy(f(n01 + n11), pi))
        + 2.0 * (xlogy(f(n00), 1.0 - pi0) + xlogy(f(n01), pi0) + xlogy(f(n10), 1.0 - pi1) + xlogy(f(n11), pi1));
    let conditional_coverage_lr = kupiec_lr + independence_lr;
    Ok(VarBacktest {
        observations: n,
        exceptions: x,
        exception_rate: xf / nf,
        expected_rate: p,
        kupiec_lr,
        kupiec_p_value: chi_squared_upper(kupiec_lr, 1),
        n00,
        n01,
        n10,
        n11,
        independence_lr,
        independence_p_value: chi_squared_upper(independence_lr, 1),
        conditional_coverage_lr,
        conditional_coverage_p_value: chi_squared_upper(conditional_coverage_lr, 2),
    })
}

/// How the rolling VaR is estimated from each window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollingMethod {
    /// Type-7 quantile of the window's losses.
    Historical,
    /// `mean + z_alpha sd` of the window's losses.
    Gaussian,
}

/// `VaR_t` for `t = window..T - 1` from the `window` losses before `t`.
pub fn compute_rolling_var(losses: &[f64], window: usize, confidence_level: f64, method: RollingMethod) -> Result<Vec<f64>, Error> {
    check_confidence(confidence_level)?;
    if window < 2 {
        return invalid("the window needs at least two losses");
    }
    check_losses(losses, window + 1)?;
    let z = compute_normal_quantile(confidence_level);
    Ok((window..losses.len())
        .map(|t| {
            let past = &losses[t - window..t];
            match method {
                RollingMethod::Historical => {
                    let mut s = past.to_vec();
                    s.sort_by(f64::total_cmp);
                    compute_sorted_quantile(&s, confidence_level, QuantileType::Type7)
                }
                RollingMethod::Gaussian => {
                    let (m, sd) = mean_sd(past);
                    m + z * sd
                }
            }
        })
        .collect())
}

/// The Basel traffic light: green while the binomial probability of at most `exceptions` exceptions is below
/// 95%, yellow while below 99.99%, red otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrafficLight {
    Green,
    Yellow,
    Red,
}

/// The zone and `P(X <= exceptions)` for `X ~ Binomial(observations, 1 - confidence_level)`.
pub fn compute_traffic_light(exceptions: usize, observations: usize, confidence_level: f64) -> Result<(TrafficLight, f64), Error> {
    check_confidence(confidence_level)?;
    if observations == 0 || exceptions > observations {
        return invalid("the exceptions must be between 0 and the number of observations, which must be positive");
    }
    let p = 1.0 - confidence_level;
    let n = observations as f64;
    // pmf(k + 1) = pmf(k) (n - k) / (k + 1) p / (1 - p), from pmf(0) = (1 - p)^n
    let mut pmf = (n * (1.0 - p).ln()).exp();
    let mut cumulative = pmf;
    for k in 0..exceptions {
        pmf *= (n - k as f64) / (k as f64 + 1.0) * p / (1.0 - p);
        cumulative += pmf;
    }
    let cumulative = cumulative.min(1.0);
    let zone = if cumulative < 0.95 {
        TrafficLight::Green
    } else if cumulative < 0.9999 {
        TrafficLight::Yellow
    } else {
        TrafficLight::Red
    };
    Ok((zone, cumulative))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantile_types_match_r() {
        let s = [1.0, 2.0, 3.0, 4.0, 5.0];
        // quantile(1:5, 0.9, type = 7) = 4.6; type = 1 = 5
        assert!((compute_sorted_quantile(&s, 0.9, QuantileType::Type7) - 4.6).abs() < 1e-15);
        assert_eq!(compute_sorted_quantile(&s, 0.9, QuantileType::Type1), 5.0);
        assert_eq!(compute_sorted_quantile(&s, 0.4, QuantileType::Type1), 2.0);
    }

    #[test]
    fn monte_carlo_does_not_depend_on_the_threads() {
        let cov = vec![vec![0.04, 0.01], vec![0.01, 0.09]];
        let run = |threads| {
            compute_monte_carlo_risk(&[0.5, 0.5], &[0.01, 0.02], &cov, 0.99, &MonteCarloSettings { scenarios: 20_000, seed: 7, batches: 20, threads })
                .unwrap()
        };
        assert_eq!(run(1), run(4));
    }
}
