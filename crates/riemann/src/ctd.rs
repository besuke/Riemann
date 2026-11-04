//! The intrinsic value of the choice of collateral currency (Cheapest to Deposit, CTD,
//! LagrangeFinance II-3.7.5, second step): discount curves of the same cash flows under each
//! eligible collateral currency, joined day by day at the highest forward.
//!
//! Each curve discounts the cash-flow currency under one collateral currency: the overnight curve
//! of that currency when it is also the collateral, a cross-currency curve from
//! [`crate::xccy::bootstrap_xccy_curve`] otherwise. The party that posts collateral picks, each day,
//! the currency whose collateral rate, converted by the basis, is the highest, so the cash flows are
//! discounted at `exp(-Σ max_c f_c)` over the daily forwards `f_c = ln(P_c(d) / P_c(d + 1))`.
//!
//! The choice uses today's forwards only: this is the intrinsic value. The time value of the option
//! to switch later, haircuts, substitution rules and costs are not included (see
//! `docs/spec_rates_fx_extension.md` 4 章).

use crate::curves::{LogDiscountInterpolation, YieldCurve};
use crate::time::{Date, DayCount};
use crate::Error;

/// A discount curve under one collateral currency.
#[derive(Debug, Clone, PartialEq)]
pub struct CollateralOption {
    pub name: String,
    pub curve: YieldCurve,
}

/// A run of days with the same cheapest collateral.
#[derive(Debug, Clone, PartialEq)]
pub struct CtdChoice {
    pub start: Date,
    pub end: Date,
    /// Position in the options given.
    pub option: usize,
    /// The chosen forward minus the next highest, summed over the run (continuous, not annualized).
    pub advantage: f64,
}

/// The discount curve with the daily choice, and the choices.
#[derive(Debug, Clone, PartialEq)]
pub struct CtdCurve {
    pub curve: YieldCurve,
    pub choices: Vec<CtdChoice>,
}

/// Joins the curves at the highest daily forward from `reference` to `end`. Every curve must have
/// `reference` as its reference date and reach `end`. The result is log-linear between the days, so
/// on each day it equals the product of the chosen daily discount ratios.
pub fn create_ctd_curve(reference: Date, end: Date, day_count: &DayCount, options: &[CollateralOption]) -> Result<CtdCurve, Error> {
    if options.is_empty() {
        return Err(Error::InvalidInput { reason: "no collateral curves given".into() });
    }
    if end <= reference {
        return Err(Error::InvalidInput { reason: format!("the end {end} must follow the reference date {reference}") });
    }
    if let Some(o) = options.iter().find(|o| o.curve.reference_date() != reference) {
        return Err(Error::InvalidInput {
            reason: format!("the curve {} starts on {}, not on {reference}", o.name, o.curve.reference_date()),
        });
    }
    let days = (end.serial() - reference.serial()) as usize;
    let mut dates = Vec::with_capacity(days + 1);
    let mut discounts = Vec::with_capacity(days + 1);
    let mut previous: Vec<f64> = options.iter().map(|o| o.curve.discount(reference)).collect::<Result<_, Error>>()?;
    let mut log_discount = 0.0;
    dates.push(reference);
    discounts.push(1.0);
    let mut choices: Vec<CtdChoice> = Vec::new();
    for k in 1..=days {
        let d = Date::from_serial(reference.serial() as i64 + k as i64)?;
        let current: Vec<f64> = options.iter().map(|o| o.curve.discount(d)).collect::<Result<_, Error>>()?;
        let forwards: Vec<f64> = previous.iter().zip(&current).map(|(p, c)| (p / c).ln()).collect();
        let (best, &top) = forwards
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .expect("at least one curve");
        let second = forwards.iter().enumerate().filter(|(i, _)| *i != best).map(|(_, f)| *f).fold(f64::NEG_INFINITY, f64::max);
        let advantage = if second.is_finite() { top - second } else { 0.0 };
        log_discount -= top;
        dates.push(d);
        discounts.push(log_discount.exp());
        let start = Date::from_serial(d.serial() as i64 - 1)?;
        match choices.last_mut() {
            Some(run) if run.option == best => {
                run.end = d;
                run.advantage += advantage;
            }
            _ => choices.push(CtdChoice { start, end: d, option: best, advantage }),
        }
        previous = current;
    }
    let curve = YieldCurve::interpolated_discounts(dates, discounts, day_count.clone(), LogDiscountInterpolation::LogLinear)?;
    Ok(CtdCurve { curve, choices })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interest_rate::{Compounding, InterestRate};
    use crate::time::Actual365Kind;

    fn d(y: i32, m: u32, day: u32) -> Date {
        Date::from_ymd(y, m, day).unwrap()
    }

    fn act365() -> DayCount {
        DayCount::Actual365Fixed(Actual365Kind::Standard)
    }

    fn zero(rates: &[f64]) -> YieldCurve {
        let today = d(2026, 11, 4);
        let dates = vec![today, d(2027, 11, 4), d(2029, 11, 4), d(2036, 11, 4)];
        YieldCurve::zero(dates, rates.to_vec(), act365(), Compounding::Continuous, 0).unwrap()
    }

    #[test]
    fn one_curve_is_reproduced_and_the_choice_never_discounts_less() {
        let today = d(2026, 11, 4);
        let end = d(2036, 11, 4);
        let jpy = CollateralOption { name: "JPY".into(), curve: zero(&[0.005, 0.005, 0.008, 0.012]) };
        // crosses the yen curve: higher at the short end, lower at the long end
        let usd = CollateralOption { name: "USD".into(), curve: zero(&[0.009, 0.009, 0.007, 0.006]) };
        let alone = create_ctd_curve(today, end, &act365(), std::slice::from_ref(&jpy)).unwrap();
        let both = create_ctd_curve(today, end, &act365(), &[jpy.clone(), usd.clone()]).unwrap();
        for day in [d(2027, 1, 1), d(2029, 11, 4), d(2033, 6, 30), end] {
            let single = jpy.curve.discount(day).unwrap();
            assert!((alone.curve.discount(day).unwrap() - single).abs() < 1e-13);
            let ctd = both.curve.discount(day).unwrap();
            assert!(ctd <= single + 1e-15 && ctd <= usd.curve.discount(day).unwrap() + 1e-15);
        }
        assert_eq!(alone.choices.len(), 1);
        // the dollar is chosen first and the yen later
        assert_eq!(both.choices.first().unwrap().option, 1);
        assert_eq!(both.choices.last().unwrap().option, 0);
        assert!(both.choices.iter().all(|c| c.advantage >= 0.0));
    }

    #[test]
    fn invalid_inputs_are_errors() {
        let today = d(2026, 11, 4);
        let jpy = CollateralOption { name: "JPY".into(), curve: zero(&[0.005, 0.005, 0.008, 0.012]) };
        assert!(create_ctd_curve(today, today, &act365(), std::slice::from_ref(&jpy)).is_err());
        assert!(create_ctd_curve(today, d(2030, 1, 1), &act365(), &[]).is_err());
        let other = CollateralOption {
            name: "late".into(),
            curve: YieldCurve::flat(d(2026, 11, 5), InterestRate::new(0.01, act365(), Compounding::Continuous, 0).unwrap()),
        };
        assert!(create_ctd_curve(today, d(2030, 1, 1), &act365(), &[jpy, other]).is_err());
    }
}
