//! Credit tranches on a portfolio loss distribution, and CMO waterfalls on a mortgage pool (LagrangeFinance I-11;
//! `docs/spec_tranching.md`).
//!
//! A tranche with attachment `A` and detachment `D` (fractions of the portfolio) loses
//! `min(max(l - A, 0), D - A)` of the portfolio when the portfolio loses the fraction `l` (I-11.9). Its expected
//! loss, the probabilities that it is hit and wiped out, and its VaR come from the exact distribution of
//! `credit_risk`; its premium and loss legs from that distribution at each payment date (I-11.12).
//!
//! CMOs redistribute the principal and interest of a pass-through pool (`mbs`): sequential tranches with an
//! accrual (Z) tranche last, and a planned amortization class (PAC) with its support tranche.

// `!(x > 0.0)` is intended: it also rejects NaN
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::credit_risk::{compute_credit_loss_distribution, compute_gauss_legendre, CreditLossDistribution, Obligor};
use crate::math::normal::{compute_normal_cdf, compute_normal_pdf, compute_normal_quantile};
use crate::Error;

fn invalid<T>(reason: impl Into<String>) -> Result<T, Error> {
    Err(Error::InvalidInput { reason: reason.into() })
}

/// `min(max(l - A, 0), D - A)`: the tranche's loss as a fraction of the portfolio.
pub fn compute_tranche_loss(loss_rate: f64, attachment: f64, detachment: f64) -> f64 {
    (loss_rate - attachment).max(0.0).min(detachment - attachment)
}

fn check_tranche(attachment: f64, detachment: f64) -> Result<(), Error> {
    if !(0.0..1.0).contains(&attachment) || !(detachment > attachment && detachment <= 1.0) {
        return invalid(format!("a tranche needs 0 <= A < D <= 1, not A = {attachment}, D = {detachment}"));
    }
    Ok(())
}

/// A tranche's loss on the exact portfolio loss distribution. Amounts are fractions of the portfolio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrancheRisk {
    pub expected_tranche_loss: f64,
    /// `P(l > A)`.
    pub hit_probability: f64,
    /// `P(l >= D)`.
    pub wipeout_probability: f64,
    /// The tranche loss at the portfolio's VaR, `min{l : P(L <= l) >= alpha}` (the tranche loss is
    /// non-decreasing in the portfolio loss, so this is the tranche's VaR).
    pub var: f64,
}

/// `total` is the portfolio notional that turns losses into the fraction `l = L / total`.
pub fn compute_tranche_risk(distribution: &CreditLossDistribution, total: f64, attachment: f64, detachment: f64, confidence_level: f64) -> Result<TrancheRisk, Error> {
    check_tranche(attachment, detachment)?;
    if !(total > 0.0) || !total.is_finite() {
        return invalid("the portfolio notional must be positive");
    }
    if !(confidence_level > 0.0 && confidence_level < 1.0) {
        return invalid("the confidence level must be strictly between 0 and 1");
    }
    let rate = |k: usize| (k as f64 * distribution.unit) / total;
    let p = &distribution.probability;
    let mut expected = 0.0;
    let mut hit = 0.0;
    let mut wiped = 0.0;
    let mut cumulative = 0.0;
    let mut var = None;
    for (k, pk) in p.iter().enumerate() {
        let l = rate(k);
        expected += pk * compute_tranche_loss(l, attachment, detachment);
        if l > attachment {
            hit += pk;
        }
        if l >= detachment {
            wiped += pk;
        }
        cumulative += pk;
        if var.is_none() && cumulative >= confidence_level - 1e-12 {
            var = Some(compute_tranche_loss(l, attachment, detachment));
        }
    }
    Ok(TrancheRisk {
        expected_tranche_loss: expected,
        hit_probability: hit,
        wipeout_probability: wiped,
        var: var.unwrap_or(detachment - attachment),
    })
}

/// The expected tranche loss of a large homogeneous pool: the portfolio loses `LGD p(Z)` with `p(z)` the
/// conditional PD. The tranche loss has kinks where `LGD p(z)` crosses `A` and `D`, at
/// `z = (Phi^-1(PD) - sqrt(1 - rho) Phi^-1(K / LGD)) / sqrt(rho)`; the integral over `Z` is split there, and each
/// piece of `[-10, 10]` gets `panels` panels of 16-point Gauss-Legendre quadrature.
pub fn compute_lhp_expected_tranche_loss(pd: f64, lgd: f64, rho: f64, attachment: f64, detachment: f64, panels: usize) -> Result<f64, Error> {
    check_tranche(attachment, detachment)?;
    if !(pd > 0.0 && pd < 1.0) || !(lgd > 0.0 && lgd <= 1.0) || !(0.0..1.0).contains(&rho) {
        return invalid("PD must be in (0, 1), LGD in (0, 1] and rho in [0, 1)");
    }
    let c = compute_normal_quantile(pd);
    let f = |z: f64| compute_tranche_loss(lgd * compute_normal_cdf((c - rho.sqrt() * z) / (1.0 - rho).sqrt()), attachment, detachment);
    let mut cuts = vec![-10.0, 10.0];
    if rho > 0.0 {
        for k in [attachment, detachment] {
            if k > 0.0 && k < lgd {
                let z = (c - (1.0 - rho).sqrt() * compute_normal_quantile(k / lgd)) / rho.sqrt();
                if z > -10.0 && z < 10.0 {
                    cuts.push(z);
                }
            }
        }
    }
    cuts.sort_by(f64::total_cmp);
    let (x, w) = compute_gauss_legendre(16)?;
    let mut total = 0.0;
    for piece in cuts.windows(2) {
        let width = (piece[1] - piece[0]) / panels.max(1) as f64;
        for p in 0..panels.max(1) {
            let mid = piece[0] + (p as f64 + 0.5) * width;
            for (xi, wi) in x.iter().zip(&w) {
                let z = mid + 0.5 * width * xi;
                total += 0.5 * width * wi * compute_normal_pdf(z) * f(z);
            }
        }
    }
    Ok(total)
}

/// The legs of a tranche (I-11.12) on obligors with constant hazard rates, so that `PD_i(t) = 1 - exp(-h_i t)`.
#[derive(Debug, Clone, PartialEq)]
pub struct TrancheLegs {
    /// `E[L_{A,D}(t)]` at each payment date, a fraction of the portfolio.
    pub expected_tranche_loss: Vec<f64>,
    pub discount: Vec<f64>,
    /// `sum DF(t) dt E[outstanding_t]` with `outstanding_t = (D - A) - L_{A,D}(t)`: the premium leg per unit spread.
    pub premium_per_unit_spread: f64,
    /// `sum DF(t) (E[L_{A,D}(t)] - E[L_{A,D}(t_prev)])`.
    pub loss_leg: f64,
    /// `loss_leg / premium_per_unit_spread`.
    pub fair_spread: f64,
}

/// One obligor of a portfolio priced over time: a constant hazard rate instead of a one-period PD.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HazardObligor {
    pub hazard: f64,
    pub ead: f64,
    pub lgd: f64,
    pub rho: f64,
}

#[allow(clippy::too_many_arguments)]
pub fn compute_tranche_legs(
    obligors: &[HazardObligor],
    unit: f64,
    times: &[f64],
    discount_rate: f64,
    attachment: f64,
    detachment: f64,
    panels: usize,
) -> Result<TrancheLegs, Error> {
    check_tranche(attachment, detachment)?;
    if times.is_empty() || times.iter().any(|t| !(*t > 0.0)) || times.windows(2).any(|w| !(w[1] > w[0])) {
        return invalid("the payment times must be positive and increasing");
    }
    if obligors.iter().any(|o| !(o.hazard > 0.0) || !o.hazard.is_finite()) || !discount_rate.is_finite() {
        return invalid("the hazard rates must be positive and the discount rate finite");
    }
    let total: f64 = obligors.iter().map(|o| o.ead).sum();
    let mut expected = Vec::with_capacity(times.len());
    for &t in times {
        let at_t: Vec<Obligor> = obligors.iter().map(|o| Obligor { pd: -(-o.hazard * t).exp_m1(), ead: o.ead, lgd: o.lgd, rho: o.rho }).collect();
        let d = compute_credit_loss_distribution(&at_t, unit, panels)?;
        expected.push(compute_tranche_risk(&d, total, attachment, detachment, 0.5)?.expected_tranche_loss);
    }
    let discount: Vec<f64> = times.iter().map(|t| (-discount_rate * t).exp()).collect();
    let width = detachment - attachment;
    let mut premium = 0.0;
    let mut loss = 0.0;
    let mut previous_time = 0.0;
    let mut previous_loss = 0.0;
    for ((t, df), e) in times.iter().zip(&discount).zip(&expected) {
        premium += df * (t - previous_time) * (width - e);
        loss += df * (e - previous_loss);
        previous_time = *t;
        previous_loss = *e;
    }
    Ok(TrancheLegs { expected_tranche_loss: expected, discount, premium_per_unit_spread: premium, loss_leg: loss, fair_spread: loss / premium })
}

// ---- CMO waterfalls ---------------------------------------------------------------------------------------------

/// A tranche of a CMO: its original balance and its annual coupon (paid monthly).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CmoTranche {
    pub balance: f64,
    pub coupon: f64,
}

/// Monthly cash flows of each tranche, by tranche then month.
#[derive(Debug, Clone, PartialEq)]
pub struct CmoCashflows {
    pub interest: Vec<Vec<f64>>,
    pub principal: Vec<Vec<f64>>,
    /// Interest added to a Z tranche's balance instead of being paid.
    pub accrued: Vec<Vec<f64>>,
    pub balance: Vec<Vec<f64>>,
    /// Collateral cash left after the tranches: excess interest, and principal once every tranche is retired.
    pub residual: Vec<f64>,
}

impl CmoCashflows {
    /// Weighted average life in years of each tranche's principal.
    pub fn weighted_average_life(&self) -> Vec<f64> {
        self.principal
            .iter()
            .map(|p| {
                let total: f64 = p.iter().sum();
                p.iter().enumerate().map(|(t, x)| (t + 1) as f64 * x).sum::<f64>() / total / 12.0
            })
            .collect()
    }
}

fn check_collateral(principal: &[f64], interest: &[f64], tranches: &[CmoTranche]) -> Result<(), Error> {
    if principal.is_empty() || principal.len() != interest.len() {
        return invalid("the collateral needs one principal and one interest amount per month");
    }
    if principal.iter().chain(interest).any(|v| !(*v >= 0.0) || !v.is_finite()) {
        return invalid("the collateral cash flows must be finite and non-negative");
    }
    if tranches.is_empty() || tranches.iter().any(|t| !(t.balance >= 0.0) || !t.balance.is_finite() || !(t.coupon >= 0.0) || !t.coupon.is_finite()) {
        return invalid("each tranche needs a non-negative balance and coupon");
    }
    Ok(())
}

/// Sequential pay: principal goes to the tranches in order. With `z_last`, the last tranche accrues: while any
/// earlier tranche is outstanding its interest is added to its balance, and that amount of collateral interest is
/// paid as principal to the earlier tranches (I-11.4, I-11.6).
#[allow(clippy::needless_range_loop)]
pub fn compute_sequential_cmo(principal: &[f64], interest: &[f64], tranches: &[CmoTranche], z_last: bool) -> Result<CmoCashflows, Error> {
    check_collateral(principal, interest, tranches)?;
    let (n, months) = (tranches.len(), principal.len());
    if z_last && n < 2 {
        return invalid("an accrual tranche needs at least one tranche before it");
    }
    let mut bal: Vec<f64> = tranches.iter().map(|t| t.balance).collect();
    let mut out = CmoCashflows {
        interest: vec![vec![0.0; months]; n],
        principal: vec![vec![0.0; months]; n],
        accrued: vec![vec![0.0; months]; n],
        balance: vec![vec![0.0; months]; n],
        residual: vec![0.0; months],
    };
    let regular = if z_last { n - 1 } else { n };
    for t in 0..months {
        let mut cash_interest = interest[t];
        let mut cash_principal = principal[t];
        for k in 0..regular {
            let i = tranches[k].coupon / 12.0 * bal[k];
            out.interest[k][t] = i;
            cash_interest -= i;
        }
        if z_last {
            let z = n - 1;
            let i = tranches[z].coupon / 12.0 * bal[z];
            if bal[..regular].iter().any(|b| *b > 0.0) {
                out.accrued[z][t] = i;
                bal[z] += i;
                cash_principal += i;
            } else {
                out.interest[z][t] = i;
            }
            cash_interest -= i;
        }
        for k in 0..n {
            let pay = cash_principal.min(bal[k]);
            out.principal[k][t] = pay;
            bal[k] -= pay;
            cash_principal -= pay;
        }
        for k in 0..n {
            out.balance[k][t] = bal[k];
        }
        out.residual[t] = cash_interest + cash_principal;
    }
    Ok(out)
}

/// The PAC schedule: each month the smaller principal of the pool at the band's two prepayment speeds.
pub fn compute_pac_schedule(principal_low: &[f64], principal_high: &[f64]) -> Result<Vec<f64>, Error> {
    if principal_low.len() != principal_high.len() || principal_low.is_empty() {
        return invalid("the two principal streams must have the same, positive length");
    }
    Ok(principal_low.iter().zip(principal_high).map(|(a, b)| a.min(*b)).collect())
}

/// A PAC and its support tranche (I-11.5). Each month the PAC is due its scheduled principal plus any earlier
/// shortfall; the support takes what is left, and once the support is retired the PAC takes the rest.
#[allow(clippy::needless_range_loop)]
pub fn compute_pac_support_cmo(principal: &[f64], interest: &[f64], schedule: &[f64], pac: CmoTranche, support: CmoTranche) -> Result<CmoCashflows, Error> {
    let tranches = [pac, support];
    check_collateral(principal, interest, &tranches)?;
    if schedule.len() != principal.len() || schedule.iter().any(|v| !(*v >= 0.0) || !v.is_finite()) {
        return invalid("the PAC schedule needs one non-negative amount per month");
    }
    let months = principal.len();
    let mut bal = [pac.balance, support.balance];
    let mut out = CmoCashflows {
        interest: vec![vec![0.0; months]; 2],
        principal: vec![vec![0.0; months]; 2],
        accrued: vec![vec![0.0; months]; 2],
        balance: vec![vec![0.0; months]; 2],
        residual: vec![0.0; months],
    };
    let mut shortfall = 0.0;
    for t in 0..months {
        let mut cash_interest = interest[t];
        let mut cash = principal[t];
        for k in 0..2 {
            let i = tranches[k].coupon / 12.0 * bal[k];
            out.interest[k][t] = i;
            cash_interest -= i;
        }
        let due = bal[0].min(schedule[t] + shortfall);
        let pay = cash.min(due);
        shortfall = due - pay;
        out.principal[0][t] = pay;
        bal[0] -= pay;
        cash -= pay;
        let pay = cash.min(bal[1]);
        out.principal[1][t] = pay;
        bal[1] -= pay;
        cash -= pay;
        let pay = cash.min(bal[0]);
        out.principal[0][t] += pay;
        bal[0] -= pay;
        cash -= pay;
        if bal[0] <= 0.0 {
            shortfall = 0.0;
        }
        out.balance[0][t] = bal[0];
        out.balance[1][t] = bal[1];
        out.residual[t] = cash_interest + cash;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tranche_loss_follows_the_manuscript() {
        assert_eq!(compute_tranche_loss(0.02, 0.03, 0.07), 0.0);
        assert!((compute_tranche_loss(0.05, 0.03, 0.07) - 0.02).abs() < 1e-17);
        assert!((compute_tranche_loss(0.12, 0.03, 0.07) - 0.04).abs() < 1e-17);
    }

    #[test]
    fn waterfalls_conserve_cash() {
        let principal = vec![10.0; 12];
        let interest = vec![1.0; 12];
        let t = [CmoTranche { balance: 50.0, coupon: 0.06 }, CmoTranche { balance: 40.0, coupon: 0.06 }, CmoTranche { balance: 30.0, coupon: 0.06 }];
        let c = compute_sequential_cmo(&principal, &interest, &t, true).unwrap();
        for m in 0..12 {
            let paid: f64 = (0..3).map(|k| c.interest[k][m] + c.principal[k][m]).sum::<f64>() + c.residual[m];
            assert!((paid - 11.0).abs() < 1e-12, "month {m}: {paid}");
        }
    }
}
