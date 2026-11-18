//! Pass-through mortgage-backed securities: monthly cash flows with prepayment, values on short-rate
//! paths, and the option-adjusted spread (LagrangeFinance II-8.8).
//!
//! Prepayment here is a behavioural rule of the borrowers, driven by the age of the loans and the
//! refinancing incentive. It is not an optimal exercise, so nothing is regressed or maximized, unlike
//! Longstaff-Schwartz. The rule and its parameters are teaching assumptions, not estimates.
//!
//! The inputs are checked once at the entry; the loops work on contiguous arrays (`docs/design_distributed.md`).

// `!(x > 0.0)` is intended: it also rejects NaN
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::math::random::generate_standard_normal;
use crate::math::solver1d::solve_brent_with_step;
use crate::Error;

fn invalid<T>(reason: String) -> Result<T, Error> {
    Err(Error::InvalidInput { reason })
}

/// How the scheduled payment follows prepayments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Amortization {
    /// The level payment of the original loan is kept; a prepayment shortens the loan
    /// (LagrangeFinance II-8.8).
    FixedPayment,
    /// The payment is recomputed each month on the remaining balance and term, as for a pool of
    /// level-payment mortgages.
    Reamortized,
}

/// A pass-through pool. Rates are annual; payments are monthly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PassThroughTerms {
    pub original_balance: f64,
    /// The rate the borrowers pay.
    pub mortgage_rate: f64,
    /// The rate the investors receive, at most the mortgage rate.
    pub pass_through_rate: f64,
    pub months: usize,
    /// 1 is 100% PSA; 0 is no prepayment.
    pub psa_multiplier: f64,
    /// beta in `exp(beta (mortgage rate - refinancing rate - cost))`.
    pub incentive_sensitivity: f64,
    /// The cost of refinancing, as a rate.
    pub refinancing_cost: f64,
    /// The highest annual prepayment rate.
    pub cpr_cap: f64,
    pub amortization: Amortization,
}

impl PassThroughTerms {
    pub fn check(&self) -> Result<(), Error> {
        let finite = [self.original_balance, self.mortgage_rate, self.pass_through_rate, self.psa_multiplier,
            self.incentive_sensitivity, self.refinancing_cost, self.cpr_cap];
        if finite.iter().any(|v| !v.is_finite()) {
            return invalid("the terms of a pass-through must be finite".into());
        }
        if !(self.original_balance > 0.0) || self.months == 0 || self.months > 1200 {
            return invalid("a pass-through needs a positive balance and 1 to 1200 months".into());
        }
        if self.mortgage_rate < 0.0 || self.pass_through_rate < 0.0 || self.pass_through_rate > self.mortgage_rate {
            return invalid("rates must be non-negative, and the pass-through rate at most the mortgage rate".into());
        }
        if self.psa_multiplier < 0.0 || self.incentive_sensitivity < 0.0 || self.refinancing_cost < 0.0 {
            return invalid("the PSA multiplier, the sensitivity and the cost must be non-negative".into());
        }
        if !(0.0..=1.0).contains(&self.cpr_cap) {
            return invalid(format!("the CPR cap must be in [0, 1], not {}", self.cpr_cap));
        }
        Ok(())
    }
}

/// The cash flows of a pool, by month, as columns.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MbsCashflows {
    pub begin_balance: Vec<f64>,
    pub refinance_rate: Vec<f64>,
    pub cpr: Vec<f64>,
    pub smm: Vec<f64>,
    pub interest: Vec<f64>,
    pub scheduled_principal: Vec<f64>,
    pub prepaid_principal: Vec<f64>,
    pub end_balance: Vec<f64>,
    pub cashflow: Vec<f64>,
}

fn level_payment(balance: f64, monthly_rate: f64, months: usize) -> f64 {
    if monthly_rate == 0.0 {
        balance / months as f64
    } else {
        balance * monthly_rate / (1.0 - (1.0 + monthly_rate).powi(-(months as i32)))
    }
}

/// The core: cash flows for one path of refinancing rates, written into `out`. No checks.
fn fill_cashflows(terms: &PassThroughTerms, refinance: &[f64], out: &mut MbsCashflows) {
    let n = terms.months;
    for column in [&mut out.begin_balance, &mut out.refinance_rate, &mut out.cpr, &mut out.smm, &mut out.interest,
        &mut out.scheduled_principal, &mut out.prepaid_principal, &mut out.end_balance, &mut out.cashflow]
    {
        column.clear();
        column.reserve(n);
    }
    let monthly_rate = terms.mortgage_rate / 12.0;
    let mut payment = level_payment(terms.original_balance, monthly_rate, n);
    let mut balance = terms.original_balance;
    for m in 1..=n {
        let begin = balance;
        if terms.amortization == Amortization::Reamortized && begin > 0.0 {
            payment = level_payment(begin, monthly_rate, n - m + 1);
        }
        let loan_interest = begin * monthly_rate;
        let mut scheduled = (payment - loan_interest).max(0.0).min(begin);
        if m == n {
            scheduled = begin;
        }
        let f = refinance[m - 1];
        let base_cpr = (0.002 * m as f64).min(0.06);
        let incentive = (terms.incentive_sensitivity * (terms.mortgage_rate - f - terms.refinancing_cost)).exp();
        let cpr = (terms.psa_multiplier * base_cpr * incentive).min(terms.cpr_cap);
        let smm = 1.0 - (1.0 - cpr).powf(1.0 / 12.0);
        let prepaid = (begin - scheduled) * smm;
        balance = (begin - scheduled - prepaid).max(0.0);
        let interest = begin * terms.pass_through_rate / 12.0;
        out.begin_balance.push(begin);
        out.refinance_rate.push(f);
        out.cpr.push(cpr);
        out.smm.push(smm);
        out.interest.push(interest);
        out.scheduled_principal.push(scheduled);
        out.prepaid_principal.push(prepaid);
        out.end_balance.push(balance);
        out.cashflow.push(interest + scheduled + prepaid);
    }
}

/// The monthly cash flows for refinancing rates by month (one value is used for every month).
pub fn compute_mbs_cashflows(terms: &PassThroughTerms, refinance_rates: &[f64]) -> Result<MbsCashflows, Error> {
    terms.check()?;
    if refinance_rates.iter().any(|r| !r.is_finite() || *r < 0.0) {
        return invalid("refinancing rates must be finite and non-negative".into());
    }
    let path: Vec<f64> = match refinance_rates.len() {
        1 => vec![refinance_rates[0]; terms.months],
        n if n == terms.months => refinance_rates.to_vec(),
        n => return invalid(format!("give 1 or {} refinancing rates, not {n}", terms.months)),
    };
    let mut out = MbsCashflows::default();
    fill_cashflows(terms, &path, &mut out);
    Ok(out)
}

/// The value and the weighted average life on each path.
#[derive(Debug, Clone, PartialEq)]
pub struct MbsPathValues {
    pub prices: Vec<f64>,
    /// Years, weighted by principal, over the original balance.
    pub weighted_average_lives: Vec<f64>,
    pub mean_price: f64,
}

/// The rates of all paths, by rows of `months` values: the short rates that discount, and the
/// refinancing rates that drive prepayment.
#[derive(Debug, Clone, Copy)]
pub struct RatePaths<'a> {
    pub short_rates: &'a [f64],
    pub refinance_rates: &'a [f64],
    pub paths: usize,
}

fn check_paths(terms: &PassThroughTerms, rates: &RatePaths) -> Result<(), Error> {
    terms.check()?;
    let size = rates.paths * terms.months;
    if rates.paths == 0 || rates.short_rates.len() != size || rates.refinance_rates.len() != size {
        return invalid(format!("give {} paths of {} months, for both the short and the refinancing rates", rates.paths, terms.months));
    }
    if rates.short_rates.iter().any(|r| !r.is_finite()) || rates.refinance_rates.iter().any(|r| !r.is_finite() || *r < 0.0) {
        return invalid("rates must be finite, and refinancing rates non-negative".into());
    }
    Ok(())
}

/// The core: path values at the spread `z`, discounting month m by exp(-sum_{i <= m} (r_i + z) / 12).
fn path_values(terms: &PassThroughTerms, rates: &RatePaths, z: f64) -> MbsPathValues {
    let n = terms.months;
    let mut flows = MbsCashflows::default();
    let mut prices = Vec::with_capacity(rates.paths);
    let mut lives = Vec::with_capacity(rates.paths);
    for p in 0..rates.paths {
        let short = &rates.short_rates[p * n..(p + 1) * n];
        fill_cashflows(terms, &rates.refinance_rates[p * n..(p + 1) * n], &mut flows);
        let (mut accumulated, mut price, mut life) = (0.0, 0.0, 0.0);
        let principal = flows.scheduled_principal.iter().zip(&flows.prepaid_principal);
        for (m, ((rate, cashflow), (scheduled, prepaid))) in short.iter().zip(&flows.cashflow).zip(principal).enumerate() {
            accumulated += (rate + z) / 12.0;
            price += cashflow * (-accumulated).exp();
            life += (m + 1) as f64 / 12.0 * (scheduled + prepaid);
        }
        prices.push(price);
        lives.push(life / terms.original_balance);
    }
    let mean_price = prices.iter().sum::<f64>() / rates.paths as f64;
    MbsPathValues { prices, weighted_average_lives: lives, mean_price }
}

/// The value on each path, with the spread `oas` added to every short rate. The price is the mean of
/// the path values, because the cash flows and the discount factors move together.
pub fn compute_mbs_path_values(terms: &PassThroughTerms, rates: &RatePaths, oas: f64) -> Result<MbsPathValues, Error> {
    check_paths(terms, rates)?;
    if !oas.is_finite() {
        return invalid(format!("the spread must be finite, not {oas}"));
    }
    Ok(path_values(terms, rates, oas))
}

/// The option-adjusted spread: the spread over the short rates of every path at which the mean path
/// value equals `price`. With one flat path, it is the static spread of LagrangeFinance II-8.8.1.
pub fn compute_mbs_oas(terms: &PassThroughTerms, rates: &RatePaths, price: f64) -> Result<f64, Error> {
    check_paths(terms, rates)?;
    if !(price > 0.0) || !price.is_finite() {
        return invalid(format!("the price must be positive, not {price}"));
    }
    solve_brent_with_step(|z| Ok(path_values(terms, rates, z).mean_price - price), 1e-12, 0.0, 0.01, 1000)
}

/// Monthly Hull-White short rates on a flat continuous curve, as in LagrangeFinance II-8.8: the state
/// is stepped exactly, and the rate is the state plus `flat + sigma^2 / (2 a^2) (1 - exp(-a t))^2`.
/// With `antithetic`, the second half of the paths uses the negated numbers of the first half. The
/// numbers come from [`generate_standard_normal`], so they are not R's.
pub fn generate_hull_white_monthly_rates(
    paths: usize,
    months: usize,
    flat_rate: f64,
    mean_reversion: f64,
    volatility: f64,
    seed: u64,
    antithetic: bool,
) -> Result<Vec<f64>, Error> {
    if paths == 0 || months == 0 || (antithetic && paths % 2 != 0) || paths.saturating_mul(months) > 500_000_000 {
        return invalid("give a positive number of paths (even with antithetic numbers) and months".into());
    }
    if !(mean_reversion > 0.0) || !(volatility >= 0.0) || !flat_rate.is_finite() || !mean_reversion.is_finite() || !volatility.is_finite() {
        return invalid("the mean reversion must be positive and the volatility non-negative".into());
    }
    let dt = 1.0 / 12.0;
    let b = (-mean_reversion * dt).exp();
    let step_sd = volatility * ((1.0 - b * b) / (2.0 * mean_reversion)).sqrt();
    let drift: Vec<f64> = (1..=months)
        .map(|m| {
            let t = m as f64 * dt;
            flat_rate + volatility * volatility / (2.0 * mean_reversion * mean_reversion) * (1.0 - (-mean_reversion * t).exp()).powi(2)
        })
        .collect();
    let distinct = if antithetic { paths / 2 } else { paths };
    let mut out = vec![0.0; paths * months];
    for p in 0..paths {
        let (source, sign) = if p < distinct { (p, 1.0) } else { (p - distinct, -1.0) };
        let mut x = 0.0;
        for m in 0..months {
            x = b * x + step_sd * sign * generate_standard_normal(seed, source as u64, m as u64);
            out[p * months + m] = drift[m] + x;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms() -> PassThroughTerms {
        PassThroughTerms {
            original_balance: 100.0,
            mortgage_rate: 0.05,
            pass_through_rate: 0.045,
            months: 360,
            psa_multiplier: 1.0,
            incentive_sensitivity: 20.0,
            refinancing_cost: 0.005,
            cpr_cap: 0.6,
            amortization: Amortization::FixedPayment,
        }
    }

    #[test]
    fn principal_adds_up_and_the_balance_ends_at_zero() {
        for amortization in [Amortization::FixedPayment, Amortization::Reamortized] {
            let t = PassThroughTerms { amortization, ..terms() };
            let c = compute_mbs_cashflows(&t, &[0.035]).unwrap();
            let principal: f64 = c.scheduled_principal.iter().chain(&c.prepaid_principal).sum();
            assert!((principal - 100.0).abs() < 1e-9, "{principal}");
            assert!(c.end_balance.last().unwrap().abs() < 1e-9);
            assert!(c.end_balance.iter().all(|b| *b >= 0.0));
        }
    }

    #[test]
    fn without_prepayment_it_is_a_level_payment_loan() {
        let t = PassThroughTerms { psa_multiplier: 0.0, pass_through_rate: 0.05, ..terms() };
        for amortization in [Amortization::FixedPayment, Amortization::Reamortized] {
            let c = compute_mbs_cashflows(&PassThroughTerms { amortization, ..t }, &[0.05]).unwrap();
            let payment = level_payment(100.0, 0.05 / 12.0, 360);
            for (i, cf) in c.cashflow.iter().enumerate() {
                assert!((cf - payment).abs() < 1e-9, "month {} {cf} {payment}", i + 1);
            }
        }
    }

    #[test]
    fn faster_prepayment_shortens_the_life() {
        let rates = |f: f64| {
            let short = vec![0.04; 360];
            let refinance = vec![f; 360];
            let r = RatePaths { short_rates: &short, refinance_rates: &refinance, paths: 1 };
            compute_mbs_path_values(&terms(), &r, 0.0).unwrap().weighted_average_lives[0]
        };
        assert!(rates(0.035) < rates(0.05) && rates(0.05) < rates(0.065));
    }

    #[test]
    fn the_spread_reprices_and_a_flat_path_is_the_static_spread() {
        let short = vec![0.04; 360];
        let refinance = vec![0.05; 360];
        let r = RatePaths { short_rates: &short, refinance_rates: &refinance, paths: 1 };
        let z = compute_mbs_oas(&terms(), &r, 95.0).unwrap();
        assert!((compute_mbs_path_values(&terms(), &r, z).unwrap().mean_price - 95.0).abs() < 1e-9);
        // the same as discounting at exp(-(0.04 + z) m / 12)
        let c = compute_mbs_cashflows(&terms(), &[0.05]).unwrap();
        let direct: f64 = c.cashflow.iter().enumerate().map(|(m, cf)| cf * (-(0.04 + z) * (m + 1) as f64 / 12.0).exp()).sum();
        assert!((direct - 95.0).abs() < 1e-9, "{direct}");
    }

    #[test]
    fn hull_white_rates_have_the_drift_and_the_antithetic_pairs() {
        let r = generate_hull_white_monthly_rates(4000, 24, 0.02, 0.15, 0.012, 7, true).unwrap();
        // pairs mirror around the drift
        let drift = |m: usize| {
            let t = (m + 1) as f64 / 12.0;
            0.02 + 0.012f64.powi(2) / (2.0 * 0.15f64.powi(2)) * (1.0 - (-0.15 * t).exp()).powi(2)
        };
        for m in [0usize, 11, 23] {
            assert!((r[m] + r[2000 * 24 + m] - 2.0 * drift(m)).abs() < 1e-15);
            let mean: f64 = (0..4000).map(|p| r[p * 24 + m]).sum::<f64>() / 4000.0;
            assert!((mean - drift(m)).abs() < 1e-14, "{mean}");
        }
        // the variance of the state after 24 months
        let t: f64 = 2.0;
        let variance = 0.012f64.powi(2) * (1.0 - (-2.0 * 0.15 * t).exp()) / (2.0 * 0.15);
        let sample: f64 = (0..2000).map(|p| (r[p * 24 + 23] - drift(23)).powi(2)).sum::<f64>() / 2000.0;
        assert!((sample / variance - 1.0).abs() < 0.1, "{sample} {variance}");
    }

    #[test]
    fn invalid_inputs_are_errors() {
        assert!(compute_mbs_cashflows(&PassThroughTerms { pass_through_rate: 0.06, ..terms() }, &[0.05]).is_err());
        assert!(compute_mbs_cashflows(&terms(), &[0.05; 12]).is_err());
        assert!(compute_mbs_cashflows(&terms(), &[-0.01]).is_err());
        let short = vec![0.04; 359];
        let refinance = vec![0.05; 360];
        let r = RatePaths { short_rates: &short, refinance_rates: &refinance, paths: 1 };
        assert!(compute_mbs_path_values(&terms(), &r, 0.0).is_err());
        assert!(generate_hull_white_monthly_rates(3, 12, 0.02, 0.15, 0.01, 1, true).is_err());
    }
}
