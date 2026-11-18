//! Credit portfolio losses in the one-factor Gaussian model, the asymptotic single risk factor (ASRF) and the
//! Basel IRB capital of corporates (LagrangeFinance I-10; `docs/spec_credit_risk.md`).
//!
//! Obligor `i` defaults when `sqrt(rho_i) Z + sqrt(1 - rho_i) eps_i < Phi^-1(PD_i)`, and then loses
//! `EAD_i LGD_i`. Given `Z = z` the defaults are independent with the conditional PD
//! `Phi((Phi^-1(PD_i) - sqrt(rho_i) z) / sqrt(1 - rho_i))`. When every loss is a whole number of a unit, the
//! distribution of the portfolio loss given `z` follows by recursion over the obligors, and the unconditional
//! distribution by composite Gauss-Legendre quadrature of `phi(z)` over `[-10, 10]`: VaR and ES without sampling
//! error. (With many obligors, the probability of one loss given `z` is a narrow peak in `z`, which Gauss-Hermite
//! quadrature resolves only with hundreds of nodes; short panels resolve it.)
//!
//! The Monte Carlo draws `Z` as dimension 0 and `eps_i` as dimension `i + 1` of the counter-based random
//! numbers (`math::random`), so the losses depend on the seed and not on the number of threads.

// `!(x > 0.0)` is intended: it also rejects NaN
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::batch::map_parallel;
use crate::math::linear::compute_symmetric_eigen;
use crate::math::normal::{compute_normal_cdf, compute_normal_pdf, compute_normal_quantile};
use crate::math::random::generate_standard_normal;
use crate::Error;

fn invalid<T>(reason: impl Into<String>) -> Result<T, Error> {
    Err(Error::InvalidInput { reason: reason.into() })
}

/// One obligor: default probability, exposure, loss given default and loading on the common factor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Obligor {
    pub pd: f64,
    pub ead: f64,
    pub lgd: f64,
    pub rho: f64,
}

fn check_obligors(obligors: &[Obligor]) -> Result<(), Error> {
    if obligors.is_empty() {
        return invalid("the portfolio needs at least one obligor");
    }
    for (i, o) in obligors.iter().enumerate() {
        if !(o.pd > 0.0 && o.pd < 1.0) || !(o.ead >= 0.0) || !o.ead.is_finite() || !(0.0..=1.0).contains(&o.lgd) || !(0.0..1.0).contains(&o.rho) {
            return invalid(format!(
                "obligor {}: PD must be in (0, 1), EAD finite and non-negative, LGD in [0, 1] and rho in [0, 1)",
                i + 1
            ));
        }
    }
    Ok(())
}

fn check_confidence(alpha: f64) -> Result<(), Error> {
    if !(alpha > 0.0 && alpha < 1.0) {
        return invalid(format!("the confidence level must be strictly between 0 and 1, not {alpha}"));
    }
    Ok(())
}

/// `Phi((Phi^-1(pd) - sqrt(rho) z) / sqrt(1 - rho))`.
pub fn compute_conditional_pd(pd: f64, rho: f64, z: f64) -> f64 {
    compute_normal_cdf((compute_normal_quantile(pd) - rho.sqrt() * z) / (1.0 - rho).sqrt())
}

/// `sum PD_i EAD_i LGD_i`.
pub fn compute_expected_credit_loss(obligors: &[Obligor]) -> f64 {
    obligors.iter().map(|o| o.pd * o.ead * o.lgd).sum()
}

/// Nodes and weights of `n`-point Gauss-Legendre quadrature on `[-1, 1]`, by Golub and Welsch: the eigenvalues
/// of the Jacobi matrix of the Legendre polynomials (off-diagonal `k / sqrt(4 k^2 - 1)`), and twice the squared
/// first components of its unit eigenvectors.
pub fn compute_gauss_legendre(n: usize) -> Result<(Vec<f64>, Vec<f64>), Error> {
    if !(2..=64).contains(&n) {
        return invalid("Gauss-Legendre quadrature needs 2 to 64 nodes");
    }
    let mut j = vec![vec![0.0; n]; n];
    for k in 1..n {
        let kf = k as f64;
        let v = kf / (4.0 * kf * kf - 1.0).sqrt();
        j[k - 1][k] = v;
        j[k][k - 1] = v;
    }
    let (values, vectors) = compute_symmetric_eigen(&j)?;
    let mut pairs: Vec<(f64, f64)> = (0..n).map(|k| (values[k], 2.0 * vectors[0][k] * vectors[0][k])).collect();
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok((pairs.iter().map(|p| p.0).collect(), pairs.iter().map(|p| p.1).collect()))
}

/// Nodes `z` and weights `phi(z) dz` of composite 16-point Gauss-Legendre quadrature on `[-10, 10]` with
/// `panels` equal panels; outside, the standard normal has mass below 1.6e-23.
pub fn compute_normal_quadrature(panels: usize) -> Result<(Vec<f64>, Vec<f64>), Error> {
    if !(1..=100_000).contains(&panels) {
        return invalid("the quadrature needs 1 to 100000 panels");
    }
    let (x, w) = compute_gauss_legendre(16)?;
    let width = 20.0 / panels as f64;
    let mut z = Vec::with_capacity(16 * panels);
    let mut weight = Vec::with_capacity(16 * panels);
    for p in 0..panels {
        let mid = -10.0 + (p as f64 + 0.5) * width;
        for (xi, wi) in x.iter().zip(&w) {
            let zi = mid + 0.5 * width * xi;
            z.push(zi);
            weight.push(0.5 * width * wi * compute_normal_pdf(zi));
        }
    }
    Ok((z, weight))
}

/// The distribution of the portfolio loss on the lattice `0, unit, 2 unit, ...`.
#[derive(Debug, Clone, PartialEq)]
pub struct CreditLossDistribution {
    pub unit: f64,
    /// `probability[k] = P(L = k unit)`.
    pub probability: Vec<f64>,
}

impl CreditLossDistribution {
    pub fn loss(&self, k: usize) -> f64 {
        k as f64 * self.unit
    }
    pub fn mean(&self) -> f64 {
        self.probability.iter().enumerate().map(|(k, p)| self.loss(k) * p).sum()
    }
    pub fn standard_deviation(&self) -> f64 {
        let m = self.mean();
        self.probability.iter().enumerate().map(|(k, p)| (self.loss(k) - m).powi(2) * p).sum::<f64>().sqrt()
    }
}

/// The exact distribution: recursion over the obligors given `z`, composite Gauss-Legendre quadrature over `z`
/// with `panels` panels on `[-10, 10]` (400 resolves the manuscript's portfolio to 1e-15).
/// Every `EAD_i LGD_i` must be a whole number of `unit` (to 1e-9 relative).
pub fn compute_credit_loss_distribution(obligors: &[Obligor], unit: f64, panels: usize) -> Result<CreditLossDistribution, Error> {
    check_obligors(obligors)?;
    if !(unit > 0.0) || !unit.is_finite() {
        return invalid("the loss unit must be positive");
    }
    let units: Vec<usize> = obligors
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let m = o.ead * o.lgd / unit;
            let r = m.round();
            if (m - r).abs() > 1e-9 * m.max(1.0) {
                invalid(format!("the loss of obligor {} ({}) is not a whole number of the unit {unit}", i + 1, o.ead * o.lgd))
            } else {
                Ok(r as usize)
            }
        })
        .collect::<Result<_, _>>()?;
    let states = units.iter().sum::<usize>() + 1;
    if states > 1_000_000 {
        return invalid("the loss lattice has more than a million points: choose a larger unit");
    }
    let thresholds: Vec<f64> = obligors.iter().map(|o| compute_normal_quantile(o.pd)).collect();
    let (z, w) = compute_normal_quadrature(panels)?;
    let mut probability = vec![0.0; states];
    let mut dist = vec![0.0; states];
    for (zk, wk) in z.iter().zip(&w) {
        dist.iter_mut().for_each(|v| *v = 0.0);
        dist[0] = 1.0;
        let mut top = 0;
        for (o, (&m, &c)) in obligors.iter().zip(units.iter().zip(&thresholds)) {
            let p = compute_normal_cdf((c - o.rho.sqrt() * zk) / (1.0 - o.rho).sqrt());
            if m == 0 {
                continue;
            }
            for s in (0..=top).rev() {
                let v = dist[s];
                dist[s + m] += p * v;
                dist[s] = (1.0 - p) * v;
            }
            top += m;
        }
        for (acc, v) in probability.iter_mut().zip(&dist) {
            *acc += wk * v;
        }
    }
    Ok(CreditLossDistribution { unit, probability })
}

/// VaR and ES of a loss distribution on a lattice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DistributionRisk {
    pub confidence_level: f64,
    /// `min { l : P(L <= l) >= alpha }`.
    pub var: f64,
    /// Rockafellar-Uryasev: `(E[L 1{L > VaR}] + VaR (P(L <= VaR) - alpha)) / (1 - alpha)`.
    pub es: f64,
    /// `E[L | L >= VaR]` (the manuscript's tail mean, I-10.11).
    pub tail_mean_es: f64,
}

pub fn compute_distribution_risk(distribution: &CreditLossDistribution, confidence_level: f64) -> Result<DistributionRisk, Error> {
    check_confidence(confidence_level)?;
    let alpha = confidence_level;
    let p = &distribution.probability;
    let mut cumulative = 0.0;
    let mut k_var = p.len() - 1;
    for (k, pk) in p.iter().enumerate() {
        cumulative += pk;
        if cumulative >= alpha - 1e-12 {
            k_var = k;
            break;
        }
    }
    let var = distribution.loss(k_var);
    let above: f64 = (k_var + 1..p.len()).map(|k| distribution.loss(k) * p[k]).sum();
    let mass: f64 = (k_var..p.len()).map(|k| p[k]).sum();
    let at_or_above: f64 = (k_var..p.len()).map(|k| distribution.loss(k) * p[k]).sum();
    Ok(DistributionRisk { confidence_level: alpha, var, es: (above + var * (cumulative - alpha)) / (1.0 - alpha), tail_mean_es: at_or_above / mass })
}

/// Portfolio losses of `scenarios` Monte Carlo scenarios.
pub fn simulate_credit_losses(obligors: &[Obligor], scenarios: usize, seed: u64, threads: usize) -> Result<Vec<f64>, Error> {
    check_obligors(obligors)?;
    if scenarios == 0 {
        return invalid("at least one scenario is needed");
    }
    let thresholds: Vec<f64> = obligors.iter().map(|o| compute_normal_quantile(o.pd)).collect();
    let chunk = 4096;
    let starts: Vec<usize> = (0..scenarios).step_by(chunk).collect();
    let parts = map_parallel(&starts, threads, |&first| {
        (first..(first + chunk).min(scenarios))
            .map(|s| {
                let z = generate_standard_normal(seed, s as u64, 0);
                obligors
                    .iter()
                    .zip(&thresholds)
                    .enumerate()
                    .filter(|(i, (o, c))| o.rho.sqrt() * z + (1.0 - o.rho).sqrt() * generate_standard_normal(seed, s as u64, *i as u64 + 1) < **c)
                    .map(|(_, (o, _))| o.ead * o.lgd)
                    .sum::<f64>()
            })
            .collect::<Vec<f64>>()
    })?;
    Ok(parts.into_iter().flatten().collect())
}

/// The ASRF loss quantile: `sum EAD_i LGD_i Phi((Phi^-1(PD_i) + sqrt(rho_i) Phi^-1(alpha)) / sqrt(1 - rho_i))`,
/// the limit of the VaR when no obligor carries a noticeable share of the portfolio.
pub fn compute_asrf_var(obligors: &[Obligor], confidence_level: f64) -> Result<f64, Error> {
    check_obligors(obligors)?;
    check_confidence(confidence_level)?;
    let q = compute_normal_quantile(confidence_level);
    Ok(obligors.iter().map(|o| o.ead * o.lgd * compute_normal_cdf((compute_normal_quantile(o.pd) + o.rho.sqrt() * q) / (1.0 - o.rho).sqrt())).sum())
}

/// Basel IRB capital of a corporate exposure (CRE31): correlation, maturity adjustment, capital per unit of
/// EAD and risk-weighted assets `12.5 K EAD`. The PD is used as given (no floor is applied here).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IrbCapital {
    pub correlation: f64,
    pub maturity_adjustment: f64,
    pub capital: f64,
    pub risk_weighted_assets: f64,
}

pub fn compute_irb_capital(pd: f64, lgd: f64, maturity: f64, ead: f64) -> Result<IrbCapital, Error> {
    if !(pd > 0.0 && pd < 1.0) || !(0.0..=1.0).contains(&lgd) || !(maturity > 0.0) || !maturity.is_finite() || !(ead >= 0.0) || !ead.is_finite() {
        return invalid("IRB capital needs PD in (0, 1), LGD in [0, 1], a positive maturity and a non-negative EAD");
    }
    let f = (1.0 - (-50.0 * pd).exp()) / (1.0 - (-50.0_f64).exp());
    let r = 0.12 * f + 0.24 * (1.0 - f);
    let b = (0.11852 - 0.05478 * pd.ln()).powi(2);
    let conditional = compute_normal_cdf((compute_normal_quantile(pd) + r.sqrt() * compute_normal_quantile(0.999)) / (1.0 - r).sqrt());
    let k = (lgd * conditional - pd * lgd) * (1.0 + (maturity - 2.5) * b) / (1.0 - 1.5 * b);
    Ok(IrbCapital { correlation: r, maturity_adjustment: b, capital: k, risk_weighted_assets: 12.5 * k * ead })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_quadrature_integrates_normal_moments() {
        let (x, w) = compute_normal_quadrature(400).unwrap();
        let moment = |p: i32| x.iter().zip(&w).map(|(xi, wi)| wi * xi.powi(p)).sum::<f64>();
        assert!((moment(0) - 1.0).abs() < 1e-14);
        assert!(moment(1).abs() < 1e-13);
        assert!((moment(2) - 1.0).abs() < 1e-12);
        assert!((moment(4) - 3.0).abs() < 1e-11);
    }

    #[test]
    fn simulation_does_not_depend_on_the_threads() {
        let o = vec![Obligor { pd: 0.02, ead: 100.0, lgd: 0.6, rho: 0.2 }; 30];
        assert_eq!(simulate_credit_losses(&o, 10_000, 5, 1).unwrap(), simulate_credit_losses(&o, 10_000, 5, 4).unwrap());
    }
}
