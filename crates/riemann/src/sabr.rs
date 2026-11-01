//! The SABR volatility formulas of Hagan et al. and their calibration.
//!
//! The formulas follow QuantLib v1.42 `ql/termstructures/volatility/sabr.cpp` (Copyright (C) 2006
//! Ferdinando Ametrano; (C) 2006 Mario Pucci; (C) 2006 StatPro Italia srl; (C) 2015 Peter Caspers;
//! (C) 2019 Klaus Spanderen); modified BSD license. See THIRD_PARTY_NOTICES. Forward and strike must
//! be positive; the shifted form is not implemented.
//!
//! The formulas are generic over [`Operand`], so calibration uses exact derivatives.

// `!(x > 0.0)` is intended: it also rejects NaN
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::autodiff::{Operand, Recorder};
use crate::math::linear::solve;
use crate::math::solver1d::close;
use crate::Error;

/// The parameters of SABR.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SabrParameters<T = f64> {
    pub alpha: T,
    pub beta: T,
    pub nu: T,
    pub rho: T,
}

/// Which volatility a formula returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SabrVolatilityType {
    Lognormal,
    Normal,
}

fn invalid<T>(reason: String) -> Result<T, Error> {
    Err(Error::InvalidInput { reason })
}

fn check<T: Operand>(p: &SabrParameters<T>, strike: f64, forward: f64, expiry: f64) -> Result<(), Error> {
    let (alpha, beta, nu, rho) = (p.alpha.primal(), p.beta.primal(), p.nu.primal(), p.rho.primal());
    if !(alpha > 0.0) {
        return invalid(format!("alpha must be positive, not {alpha}"));
    }
    if !(0.0..=1.0).contains(&beta) {
        return invalid(format!("beta must be in [0, 1], not {beta}"));
    }
    if !(nu >= 0.0) {
        return invalid(format!("nu must not be negative, not {nu}"));
    }
    if !(rho * rho < 1.0) {
        return invalid(format!("rho must be inside (-1, 1), not {rho}"));
    }
    if !(strike > 0.0 && forward > 0.0) {
        return invalid(format!("strike and forward must be positive ({strike}, {forward})"));
    }
    if !(expiry >= 0.0) {
        return invalid(format!("expiry must not be negative, not {expiry}"));
    }
    Ok(())
}

/// The implied volatility of SABR at `strike`.
pub fn sabr_volatility<T: Operand>(
    kind: SabrVolatilityType,
    strike: f64,
    forward: f64,
    expiry: f64,
    p: &SabrParameters<T>,
) -> Result<T, Error> {
    check(p, strike, forward, expiry)?;
    let SabrParameters { alpha, beta, nu, rho } = *p;
    let one = alpha.like(1.0);
    let one_minus_beta = one - beta;
    let log_fk = (forward * strike).ln();
    let a = (one_minus_beta * log_fk).exp();
    let sqrt_a = a.sqrt();
    let log_m = if close(forward, strike) {
        let epsilon = (forward - strike) / strike;
        epsilon - 0.5 * epsilon * epsilon
    } else {
        (forward / strike).ln()
    };
    let z = nu / alpha * sqrt_a * log_m;
    let b = one - rho * z * 2.0 + z * z;
    let c = one_minus_beta * one_minus_beta * (log_m * log_m);
    let xx = ((b.sqrt() + z - rho) / (one - rho)).ln();
    let e2 = c / 24.0 + c * c / 1920.0 + 1.0;
    let common = rho * beta * nu * alpha / sqrt_a * 0.25 + (one * 2.0 - rho * rho * 3.0) * (nu * nu / 24.0);
    let multiplier = if (z.primal() * z.primal()).abs() > f64::EPSILON * 10.0 {
        z / xx
    } else {
        one - rho * z * 0.5 - (rho * rho * 3.0 - 2.0) * z * z / 12.0
    };
    Ok(match kind {
        SabrVolatilityType::Lognormal => {
            let d = sqrt_a * e2;
            let t = (one_minus_beta * one_minus_beta * alpha * alpha / (a * 24.0) + common) * expiry + 1.0;
            alpha / d * multiplier * t
        }
        SabrVolatilityType::Normal => {
            let lm2 = log_m * log_m;
            let e1 = 1.0 + lm2 / 24.0 + lm2 * lm2 / 1920.0;
            let e = one * e1 / e2;
            let minus_beta = -beta;
            let t = (minus_beta * (one * 2.0 - beta) * alpha * alpha / (a * 24.0) + common) * expiry + 1.0;
            let f = alpha * (beta * log_fk * 0.5).exp();
            f * e * multiplier * t
        }
    })
}

/// A calibrated smile and how well it fits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SabrCalibration {
    pub parameters: SabrParameters,
    /// The square root of the mean squared difference from the quotes.
    pub root_mean_square_error: f64,
    pub iterations: usize,
}

/// Calibrates alpha, nu and rho to volatilities at one expiry, with beta fixed, by Levenberg-Marquardt
/// on `(ln alpha, ln nu, atanh rho)`, so that the parameters stay in range. The Jacobian comes from
/// automatic differentiation. The result depends only on the inputs.
pub fn calibrate_sabr(
    kind: SabrVolatilityType,
    strikes: &[f64],
    volatilities: &[f64],
    forward: f64,
    expiry: f64,
    beta: f64,
) -> Result<SabrCalibration, Error> {
    if strikes.len() != volatilities.len() || strikes.len() < 3 {
        return invalid("calibration needs at least 3 strikes, with one volatility each".into());
    }
    let n = strikes.len();
    let mid = volatilities[n / 2];
    // the at-the-money level of the volatility fixes alpha to first order
    let alpha0 = match kind {
        SabrVolatilityType::Lognormal => mid * forward.powf(1.0 - beta),
        SabrVolatilityType::Normal => mid * forward.powf(-beta),
    };
    let mut x = [alpha0.max(1e-8).ln(), 0.3f64.ln(), 0.0];
    let residuals_jacobian = |x: &[f64; 3]| -> Result<(Vec<f64>, Vec<[f64; 3]>), Error> {
        let (mut r, mut j) = (Vec::with_capacity(n), Vec::with_capacity(n));
        for (&strike, &volatility) in strikes.iter().zip(volatilities) {
            let rec = Recorder::new();
            let (u, v, w) = (rec.input(x[0]), rec.input(x[1]), rec.input(x[2]));
            let we = w.exp();
            let rho = (we - we.recip()) / (we + we.recip());
            let p = SabrParameters { alpha: u.exp(), beta: rec.constant(beta), nu: v.exp(), rho };
            let value = sabr_volatility(kind, strike, forward, expiry, &p)?;
            let g = rec.gradient(value, &[u, v, w])?;
            r.push(value.primal() - volatility);
            j.push([g[0].mean(), g[1].mean(), g[2].mean()]);
        }
        Ok((r, j))
    };
    let cost = |r: &[f64]| r.iter().map(|e| e * e).sum::<f64>();
    let (mut r, mut j) = residuals_jacobian(&x)?;
    let mut lambda = 1e-3;
    let mut iterations = 0;
    for _ in 0..200 {
        iterations += 1;
        let mut jtj = vec![vec![0.0; 3]; 3];
        let mut jtr = vec![vec![0.0]; 3];
        for (row, e) in j.iter().zip(&r) {
            for a in 0..3 {
                jtr[a][0] -= row[a] * e;
                for b in 0..3 {
                    jtj[a][b] += row[a] * row[b];
                }
            }
        }
        let mut improved = false;
        for _ in 0..30 {
            let mut damped = jtj.clone();
            for (a, row) in damped.iter_mut().enumerate() {
                row[a] += lambda * row[a].abs().max(1e-12);
            }
            let step = solve(&damped, &jtr)?;
            let trial = [x[0] + step[0][0], x[1] + step[1][0], (x[2] + step[2][0]).clamp(-10.0, 10.0)];
            if let Ok((rt, jt)) = residuals_jacobian(&trial) {
                if cost(&rt) < cost(&r) {
                    let change = (cost(&r) - cost(&rt)) / cost(&r).max(1e-300);
                    x = trial;
                    r = rt;
                    j = jt;
                    lambda = (lambda * 0.3).max(1e-12);
                    improved = change > 1e-14;
                    break;
                }
            }
            lambda *= 4.0;
        }
        if !improved {
            break;
        }
    }
    let we = x[2].exp();
    Ok(SabrCalibration {
        parameters: SabrParameters {
            alpha: x[0].exp(),
            beta,
            nu: x[1].exp(),
            rho: (we - 1.0 / we) / (we + 1.0 / we),
        },
        root_mean_square_error: (cost(&r) / n as f64).sqrt(),
        iterations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: SabrParameters = SabrParameters { alpha: 0.03, beta: 0.5, nu: 0.4, rho: -0.3 };

    #[test]
    fn at_the_money_is_continuous() {
        for kind in [SabrVolatilityType::Lognormal, SabrVolatilityType::Normal] {
            let at = sabr_volatility(kind, 0.03, 0.03, 2.0, &P).unwrap();
            let near = sabr_volatility(kind, 0.03 * (1.0 + 1e-6), 0.03, 2.0, &P).unwrap();
            assert!((at - near).abs() < 1e-6, "{at} {near}");
        }
    }

    #[test]
    fn calibration_recovers_the_parameters_from_its_own_smile() {
        let forward = 0.03;
        let strikes: Vec<f64> = [0.6, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5].iter().map(|m| m * forward).collect();
        for kind in [SabrVolatilityType::Lognormal, SabrVolatilityType::Normal] {
            let vols: Vec<f64> = strikes.iter().map(|&k| sabr_volatility(kind, k, forward, 2.0, &P).unwrap()).collect();
            let c = calibrate_sabr(kind, &strikes, &vols, forward, 2.0, P.beta).unwrap();
            assert!(c.root_mean_square_error < 1e-9, "{kind:?} {}", c.root_mean_square_error);
            assert!((c.parameters.alpha - P.alpha).abs() < 1e-5, "{:?}", c.parameters);
            assert!((c.parameters.nu - P.nu).abs() < 1e-4, "{:?}", c.parameters);
            assert!((c.parameters.rho - P.rho).abs() < 1e-4, "{:?}", c.parameters);
        }
    }

    #[test]
    fn invalid_inputs_are_errors() {
        let bad = SabrParameters { rho: 1.0, ..P };
        assert!(sabr_volatility(SabrVolatilityType::Normal, 0.03, 0.03, 1.0, &bad).is_err());
        assert!(sabr_volatility(SabrVolatilityType::Normal, -0.01, 0.03, 1.0, &P).is_err());
        assert!(calibrate_sabr(SabrVolatilityType::Normal, &[0.03], &[0.01], 0.03, 1.0, 0.5).is_err());
    }
}
