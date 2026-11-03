//! Building blocks of the BGM/LMM (LagrangeFinance II-7.4): a caplet by Black's or Bachelier's
//! formula, a caplet from the marginal distribution of its forward rate on given normal variates, and
//! correlated normal shocks.
//!
//! This is not a full LIBOR market model. There is no drift under a common measure and no joint path
//! of several forward rates: each caplet is valued under its own forward measure, where its rate is a
//! martingale. The extension to a full model is a separate decision.
//!
//! The random numbers are inputs, so that the same draws give the same values in any language; the
//! generators of this library are separate functions.

// `!(x > 0.0)` is intended: it also rejects NaN
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::math::linear::compute_cholesky;
use crate::math::random::generate_standard_normal;
use crate::options::{compute_forward_value, ForwardInputs, ForwardModel, OptionType};
use crate::Error;

fn invalid<T>(reason: String) -> Result<T, Error> {
    Err(Error::InvalidInput { reason })
}

/// A caplet on the forward rate of one period, paid at the end of the period.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapletTerms {
    pub forward: f64,
    pub strike: f64,
    /// Annual volatility: lognormal for Black, in rate units for Bachelier.
    pub volatility: f64,
    /// Years to the fixing.
    pub expiry: f64,
    /// Year fraction of the period.
    pub accrual: f64,
    /// Discount factor to the payment.
    pub discount_factor: f64,
    pub notional: f64,
}

impl CapletTerms {
    fn check(&self, model: ForwardModel) -> Result<(), Error> {
        let values = [self.forward, self.strike, self.volatility, self.expiry, self.accrual, self.discount_factor, self.notional];
        if values.iter().any(|v| !v.is_finite()) {
            return invalid("the terms of a caplet must be finite".into());
        }
        if self.volatility < 0.0 || self.expiry < 0.0 || !(self.accrual > 0.0) || !(self.discount_factor > 0.0) || !(self.notional > 0.0) {
            return invalid("a caplet needs a non-negative volatility and expiry, and a positive accrual, discount and notional".into());
        }
        if model == ForwardModel::Black && (!(self.forward > 0.0) || !(self.strike > 0.0)) {
            return invalid("the lognormal caplet needs a positive forward and strike".into());
        }
        Ok(())
    }

    fn scale(&self) -> f64 {
        self.notional * self.accrual * self.discount_factor
    }
}

/// The value of a caplet: `notional * accrual * discount * formula(forward, strike, volatility, expiry)`.
/// With no volatility or no time, it is the discounted intrinsic value.
pub fn compute_caplet_value(model: ForwardModel, terms: &CapletTerms) -> Result<f64, Error> {
    terms.check(model)?;
    if terms.volatility * terms.expiry.sqrt() == 0.0 {
        return Ok(terms.scale() * (terms.forward - terms.strike).max(0.0));
    }
    let inputs = ForwardInputs { forward: terms.forward, strike: terms.strike, volatility: terms.volatility, time: terms.expiry, discount: 1.0 };
    Ok(terms.scale() * compute_forward_value(model, OptionType::Call, &inputs)?)
}

/// A caplet from simulated forward rates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapletSimulation {
    pub value: f64,
    /// `sd / sqrt(paths)` of the discounted payoffs.
    pub standard_error: f64,
    pub paths: usize,
}

/// The caplet from the lognormal marginal of its forward under its own forward measure:
/// `L(T) = L(0) exp(-v^2 T / 2 + v sqrt(T) z)` for each standard normal `z` in `normals`.
pub fn compute_caplet_from_normals(terms: &CapletTerms, normals: &[f64]) -> Result<CapletSimulation, Error> {
    terms.check(ForwardModel::Black)?;
    if normals.len() < 2 || normals.iter().any(|z| !z.is_finite()) {
        return invalid("give at least 2 finite normal variates".into());
    }
    let total = terms.volatility * terms.expiry.sqrt();
    let drift = -0.5 * total * total;
    let scale = terms.scale();
    let (mut sum, mut sum_squares) = (0.0, 0.0);
    for z in normals {
        let payoff = scale * (terms.forward * (drift + total * z).exp() - terms.strike).max(0.0);
        sum += payoff;
        sum_squares += payoff * payoff;
    }
    let n = normals.len() as f64;
    let variance = ((sum_squares - sum * sum / n) / (n - 1.0)).max(0.0);
    Ok(CapletSimulation { value: sum / n, standard_error: (variance / n).sqrt(), paths: normals.len() })
}

// the indices compare an entry with its transpose
#[allow(clippy::needless_range_loop)]
fn check_correlation(correlation: &[f64], dimension: usize) -> Result<Vec<Vec<f64>>, Error> {
    if dimension == 0 || correlation.len() != dimension * dimension {
        return invalid(format!("the correlation must be {dimension} by {dimension}"));
    }
    let rows: Vec<Vec<f64>> = correlation.chunks(dimension).map(<[f64]>::to_vec).collect();
    for i in 0..dimension {
        if (rows[i][i] - 1.0).abs() > 1e-12 {
            return invalid("a correlation matrix has ones on its diagonal".into());
        }
        for j in 0..i {
            if !rows[i][j].is_finite() || (rows[i][j] - rows[j][i]).abs() > 1e-12 || rows[i][j].abs() > 1.0 {
                return invalid("a correlation matrix is symmetric, with entries in [-1, 1]".into());
            }
        }
    }
    compute_cholesky(&rows)
}

/// Correlates rows of independent standard normals: each row `z` of `independent` (rows of
/// `dimension` values) becomes `L z`, where `L L^T` is the correlation. This is R's `z %*% chol(C)`.
pub fn compute_correlated_normals(independent: &[f64], correlation: &[f64], dimension: usize) -> Result<Vec<f64>, Error> {
    let lower = check_correlation(correlation, dimension)?;
    if independent.is_empty() || independent.len() % dimension != 0 || independent.iter().any(|z| !z.is_finite()) {
        return invalid(format!("give finite normal variates in rows of {dimension}"));
    }
    let mut out = vec![0.0; independent.len()];
    for (row, z) in out.chunks_mut(dimension).zip(independent.chunks(dimension)) {
        for (i, value) in row.iter_mut().enumerate() {
            *value = lower[i][..=i].iter().zip(z).map(|(l, x)| l * x).sum();
        }
    }
    Ok(out)
}

/// Correlated standard normals from this library's random numbers: `count` rows of `dimension`.
pub fn generate_correlated_normals(correlation: &[f64], dimension: usize, count: usize, seed: u64) -> Result<Vec<f64>, Error> {
    if count == 0 || count.saturating_mul(dimension) > 500_000_000 {
        return invalid(format!("give from 1 row to fewer than 5e8 values, not {count} rows"));
    }
    let independent: Vec<f64> = (0..count)
        .flat_map(|r| (0..dimension).map(move |d| generate_standard_normal(seed, r as u64, d as u64)))
        .collect();
    compute_correlated_normals(&independent, correlation, dimension)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms() -> CapletTerms {
        CapletTerms { forward: 0.025, strike: 0.0275, volatility: 0.24, expiry: 2.0, accrual: 0.5, discount_factor: (-0.05f64).exp(), notional: 1.0 }
    }

    #[test]
    fn the_simulation_converges_to_the_formula() {
        let t = terms();
        let exact = compute_caplet_value(ForwardModel::Black, &t).unwrap();
        let normals: Vec<f64> = (0..200_000).map(|p| generate_standard_normal(5, p, 0)).collect();
        let s = compute_caplet_from_normals(&t, &normals).unwrap();
        assert!((s.value - exact).abs() < 3.0 * s.standard_error, "{} {exact} {}", s.value, s.standard_error);
    }

    #[test]
    fn no_volatility_is_the_intrinsic_value() {
        let t = CapletTerms { volatility: 0.0, forward: 0.03, ..terms() };
        let v = compute_caplet_value(ForwardModel::Black, &t).unwrap();
        assert!((v - 0.5 * (-0.05f64).exp() * 0.0025).abs() < 1e-16);
        // Bachelier allows a negative forward
        let n = CapletTerms { forward: -0.001, strike: 0.0, volatility: 0.006, ..terms() };
        assert!(compute_caplet_value(ForwardModel::Bachelier, &n).unwrap() > 0.0);
        assert!(compute_caplet_value(ForwardModel::Black, &n).is_err());
    }

    #[test]
    fn correlated_normals_have_the_target_correlation() {
        let c = [1.0, 0.8, 0.5, 0.8, 1.0, 0.7, 0.5, 0.7, 1.0];
        let z = generate_correlated_normals(&c, 3, 200_000, 9).unwrap();
        for i in 0..3 {
            for j in 0..3 {
                let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
                for row in z.chunks(3) {
                    sxy += row[i] * row[j];
                    sxx += row[i] * row[i];
                    syy += row[j] * row[j];
                }
                let r = sxy / (sxx * syy).sqrt();
                assert!((r - c[i * 3 + j]).abs() < 0.01, "{i} {j} {r}");
            }
        }
    }

    #[test]
    fn invalid_correlations_are_errors() {
        assert!(compute_correlated_normals(&[0.1, 0.2], &[1.0, 0.5, 0.4, 1.0], 2).is_err());
        assert!(compute_correlated_normals(&[0.1, 0.2], &[1.0, 1.5, 1.5, 1.0], 2).is_err());
        // not positive definite
        let c = [1.0, 0.9, -0.9, 0.9, 1.0, 0.9, -0.9, 0.9, 1.0];
        assert!(compute_correlated_normals(&[0.1, 0.2, 0.3], &c, 3).is_err());
        assert!(compute_caplet_from_normals(&terms(), &[0.1]).is_err());
    }
}
