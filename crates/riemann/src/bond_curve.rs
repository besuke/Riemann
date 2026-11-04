//! Yield curves bootstrapped from bond prices: linear on continuous zero rates or log-linear on
//! discount factors.
//!
//! Reproduces QuantLib v1.42 (modified BSD license; see THIRD_PARTY_NOTICES):
//! `ql/termstructures/yield/bondhelpers.cpp` (Copyright (C) 2008, 2009 Ferdinando Ametrano;
//! (C) 2005 Toyin Akin; (C) 2007 StatPro Italia srl), `BondHelper` and `FixedRateBondHelper`;
//! `ql/termstructures/yield/bootstraptraits.hpp` (Copyright (C) 2005, 2007 StatPro Italia srl;
//! (C) 2011 Ferdinando Ametrano; (C) 2007 Chris Kenyon; (C) 2019 SoftSolutions! S.r.l.), the
//! `ZeroYield` and `Discount` traits; and `ql/termstructures/iterativebootstrap.hpp` (Copyright
//! (C) 2008, 2011, 2015 Ferdinando Ametrano; (C) 2007 Chris Kenyon; (C) 2007 StatPro Italia srl;
//! (C) 2015 Paolo Mazzocchi), one pass of the iterative bootstrap, enough for these local
//! interpolations.
//!
//! Differences from QuantLib: the bonds and their prices are values; there are no quotes, handles
//! or global evaluation date.

use crate::curves::{LogDiscountInterpolation, YieldCurve};
use crate::instruments::fixed_rate_bond::{FixedRateBond, PriceType};
use crate::interest_rate::Compounding;
use crate::math::solver1d::solve_brent;
use crate::time::{Date, DayCount};
use crate::Error;
use std::cell::Cell;

const AVG_RATE: f64 = 0.05;
const MAX_RATE: f64 = 1.0;
const MAX_FUNCTION_EVALUATIONS: usize = 100;

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

fn build_curve(
    interpolation: BondCurveInterpolation,
    dates: &[Date],
    data: &[f64],
    day_count: &DayCount,
) -> Result<YieldCurve, Error> {
    match interpolation {
        BondCurveInterpolation::LinearZero => {
            YieldCurve::zero(dates.to_vec(), data.to_vec(), day_count.clone(), Compounding::Continuous, 0)
        }
        BondCurveInterpolation::LogLinearDiscount => YieldCurve::interpolated_discounts(
            dates.to_vec(),
            data.to_vec(),
            day_count.clone(),
            LogDiscountInterpolation::LogLinear,
        ),
    }
}

/// Bootstraps the curve, one Brent search per node from the shortest bond, each node fixed by the
/// last payment date of its bond. The first node of a zero curve, at the reference date, takes
/// the value of the second (QuantLib `ZeroYield::updateGuess`).
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
    let day_count = &spec.day_count;
    let time = |d: Date| day_count.year_fraction(reference, d, None, None);
    let mut dates = vec![reference];
    let mut times = vec![time(reference)?];
    for &i in &order {
        let p = pillar(&spec.quotes[i].bond);
        if dates[dates.len() - 1] == p {
            return Err(Error::InvalidInput { reason: format!("more than one bond with pillar {p}") });
        }
        dates.push(p);
        times.push(time(p)?);
    }

    let zero = spec.interpolation == BondCurveInterpolation::LinearZero;
    let mut data = vec![if zero { AVG_RATE } else { 1.0 }; dates.len()];
    for (position, &i) in order.iter().enumerate() {
        let node = position + 1;
        let (mut guess, min, max) = if zero {
            let guess = if node == 1 {
                AVG_RATE
            } else if node == 2 {
                data[1]
            } else {
                // the zero rate at the new node on the line through the last two nodes
                let slope = (data[node - 1] - data[node - 2]) / (times[node - 1] - times[node - 2]);
                data[node - 1] + slope * (times[node] - times[node - 1])
            };
            (guess, -MAX_RATE, MAX_RATE)
        } else {
            let dt = times[node] - times[node - 1];
            let guess = if node == 1 {
                1.0 / (1.0 + AVG_RATE * times[1])
            } else {
                let r = -data[node - 1].ln() / times[node - 1];
                (-r * times[node]).exp()
            };
            (guess, data[node - 1] * (-MAX_RATE * dt).exp(), data[node - 1] * (MAX_RATE * dt).exp())
        };
        if guess >= max {
            guess = max - (max - min) / 5.0;
        } else if guess <= min {
            guess = min + (max - min) / 5.0;
        }
        let quote = &spec.quotes[i];
        // QuantLib keeps the value last passed to the objective
        let last_tried = Cell::new(guess);
        let error = |x: f64| {
            last_tried.set(x);
            let mut y = data[..=node].to_vec();
            y[node] = x;
            if zero && node == 1 {
                y[0] = x;
            }
            let curve = build_curve(spec.interpolation, &dates[..=node], &y, day_count)?;
            Ok(quote.price - implied_price(quote, &curve, spec.evaluation_date)?)
        };
        if let Err(e) = solve_brent(error, spec.accuracy, guess, min, max, MAX_FUNCTION_EVALUATIONS) {
            return Err(Error::NoConvergence {
                reason: format!("failed at bond {node}, pillar {}: {e}", dates[node]),
            });
        }
        data[node] = last_tried.get();
        if zero && node == 1 {
            data[0] = data[1];
        }
    }

    let curve = build_curve(spec.interpolation, &dates, &data, day_count)?;
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
