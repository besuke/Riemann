//! Yield curves bootstrapped from bond prices: linear on continuous zero rates or log-linear on
//! discount factors.
//!
//! Reproduces QuantLib v1.42 (modified BSD license; see THIRD_PARTY_NOTICES):
//! `ql/termstructures/yield/bondhelpers.cpp` (Copyright (C) 2008, 2009 Ferdinando Ametrano;
//! (C) 2005 Toyin Akin; (C) 2007 StatPro Italia srl), `BondHelper` and `FixedRateBondHelper`. The
//! bootstrap itself is [`crate::bootstrap`].
//!
//! Differences from QuantLib: the bonds and their prices are values; there are no quotes, handles
//! or global evaluation date.

use crate::bootstrap::{bootstrap_local, LocalInterpolation};
use crate::curves::YieldCurve;
use crate::instruments::fixed_rate_bond::{FixedRateBond, PriceType};
use crate::time::{Date, DayCount};
use crate::Error;

/// What the bootstrapped curve interpolates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BondCurveInterpolation {
    /// Continuous zero rates, linear in time (QuantLib `PiecewiseLinearZero`,
    /// `PiecewiseYieldCurve<ZeroYield, Linear>`).
    LinearZero,
    /// Discount factors, log-linear in time (QuantLib `PiecewiseYieldCurve<Discount, LogLinear>`).
    LogLinearDiscount,
}

/// One bond and its quoted price (QuantLib `BondHelper`).
#[derive(Debug, Clone, PartialEq)]
pub struct BondCurveQuote {
    pub bond: FixedRateBond,
    /// Per 100 of notional.
    pub price: f64,
    pub price_type: PriceType,
}

/// Everything needed to build a curve from bond prices.
#[derive(Debug, Clone, PartialEq)]
pub struct BondCurveSpec {
    /// The date the bonds' settlement is counted from (QuantLib's evaluation date).
    pub evaluation_date: Date,
    pub reference_date: Date,
    pub day_count: DayCount,
    pub interpolation: BondCurveInterpolation,
    /// Accuracy of each search, on zero rates or discount factors (QuantLib uses 1e-12).
    pub accuracy: f64,
    pub quotes: Vec<BondCurveQuote>,
}

/// How the curve reprices one bond, in the order of `BondCurveSpec::quotes`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BondQuoteRepricing {
    pub price: f64,
    /// The bond's last payment date, the node it fixes (QuantLib `latestDate`).
    pub pillar_date: Date,
    pub implied_price: f64,
}

/// A curve bootstrapped from bond prices.
#[derive(Debug, Clone, PartialEq)]
pub struct BondCurve {
    pub curve: YieldCurve,
    pub quote_repricing: Vec<BondQuoteRepricing>,
}

fn pillar(bond: &FixedRateBond) -> Date {
    bond.cashflows.iter().map(|cf| cf.date()).max().expect("a bond has cash flows")
}

fn implied_price(quote: &BondCurveQuote, curve: &YieldCurve, evaluation_date: Date) -> Result<f64, Error> {
    let v = quote.bond.value_on_curve(curve, evaluation_date)?;
    Ok(match quote.price_type {
        PriceType::Clean => v.clean_price,
        PriceType::Dirty => v.dirty_price,
    })
}

/// Bootstraps the curve, one Brent search per node from the shortest bond, each node fixed by the
/// last payment date of its bond ([`crate::bootstrap::bootstrap_local`]).
pub fn bootstrap_bond_curve(spec: &BondCurveSpec) -> Result<BondCurve, Error> {
    if spec.quotes.is_empty() {
        return Err(Error::InvalidInput { reason: "no bonds given".into() });
    }
    let reference = spec.reference_date;
    let mut order: Vec<usize> = (0..spec.quotes.len()).collect();
    order.sort_by_key(|&i| pillar(&spec.quotes[i].bond));
    order.retain(|&i| pillar(&spec.quotes[i].bond) > reference);
    if order.is_empty() {
        return Err(Error::InvalidInput { reason: "all bonds expired".into() });
    }
    let pillars: Vec<Date> = order.iter().map(|&i| pillar(&spec.quotes[i].bond)).collect();
    if let Some(w) = pillars.windows(2).find(|w| w[0] == w[1]) {
        return Err(Error::InvalidInput { reason: format!("more than one bond with pillar {}", w[0]) });
    }
    let interpolation = match spec.interpolation {
        BondCurveInterpolation::LinearZero => LocalInterpolation::LinearZero,
        BondCurveInterpolation::LogLinearDiscount => LocalInterpolation::LogLinearDiscount,
    };
    let curve = bootstrap_local(reference, &spec.day_count, interpolation, &pillars, spec.accuracy, |k, curve| {
        let quote = &spec.quotes[order[k]];
        Ok(quote.price - implied_price(quote, curve, spec.evaluation_date)?)
    })?;
    let quote_repricing = spec
        .quotes
        .iter()
        .map(|q| {
            Ok(BondQuoteRepricing {
                price: q.price,
                pillar_date: pillar(&q.bond),
                implied_price: implied_price(q, &curve, spec.evaluation_date)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(BondCurve { curve, quote_repricing })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruments::fixed_rate_bond::FixedRateBondSpec;
    use crate::time::{BusinessDayConvention, Calendar, DateGenerationRule, HolidayCalendar, Period, ScheduleSpec, Thirty360Kind, TimeUnit};

    fn d(y: i32, m: u32, day: u32) -> Date {
        Date::from_ymd(y, m, day).unwrap()
    }

    fn bond(years: i32, coupon: f64) -> FixedRateBond {
        let today = d(2026, 11, 2);
        FixedRateBond::new(FixedRateBondSpec {
            settlement_days: 1,
            face_amount: 100.0,
            schedule: ScheduleSpec {
                effective_date: today,
                termination_date: today.add_period(Period::new(years, TimeUnit::Years)).unwrap(),
                tenor: Period::new(6, TimeUnit::Months),
                calendar: HolidayCalendar::from(Calendar::WeekendsOnly),
                convention: BusinessDayConvention::Unadjusted,
                termination_convention: BusinessDayConvention::Unadjusted,
                rule: DateGenerationRule::Backward,
                end_of_month: false,
                first_date: None,
                next_to_last_date: None,
            },
            coupon_rate: coupon,
            day_count: DayCount::Thirty360(Thirty360Kind::BondBasis),
            payment_convention: BusinessDayConvention::Following,
            redemption: 100.0,
            issue_date: None,
            payment_calendar: None,
        })
        .unwrap()
    }

    fn spec(interpolation: BondCurveInterpolation, prices: &[(i32, f64, f64)]) -> BondCurveSpec {
        BondCurveSpec {
            evaluation_date: d(2026, 11, 2),
            reference_date: d(2026, 11, 2),
            day_count: DayCount::Actual360 { include_last_day: false },
            interpolation,
            accuracy: 1e-12,
            quotes: prices
                .iter()
                .map(|&(years, coupon, price)| BondCurveQuote { bond: bond(years, coupon), price, price_type: PriceType::Clean })
                .collect(),
        }
    }

    #[test]
    fn the_curve_reprices_its_bonds() {
        let prices = [(5, 0.03, 101.0), (1, 0.01, 99.8), (2, 0.02, 100.0), (10, 0.035, 98.5)];
        for interpolation in [BondCurveInterpolation::LinearZero, BondCurveInterpolation::LogLinearDiscount] {
            let built = bootstrap_bond_curve(&spec(interpolation, &prices)).unwrap();
            for r in &built.quote_repricing {
                assert!((r.implied_price - r.price).abs() < 1e-9, "{interpolation:?}: {r:?}");
            }
        }
    }

    #[test]
    fn bonds_on_the_same_date_and_none_are_errors() {
        assert!(bootstrap_bond_curve(&spec(BondCurveInterpolation::LinearZero, &[])).is_err());
        let same = [(2, 0.02, 100.0), (2, 0.03, 101.0)];
        assert!(bootstrap_bond_curve(&spec(BondCurveInterpolation::LinearZero, &same)).is_err());
    }
}
