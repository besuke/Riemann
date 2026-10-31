//! OIS discount curves bootstrapped from overnight deposits and OIS quotes.
//!
//! Reproduces QuantLib C++ v1.42 (see `docs/spec_ois.md` and THIRD_PARTY_NOTICES; modified BSD
//! license): `ql/termstructures/yield/oisratehelper.cpp` (Copyright (C) 2009, 2012 Roland
//! Lichters; (C) 2009, 2012 Ferdinando Ametrano), `ql/termstructures/yield/ratehelpers.cpp`
//! (Copyright (C) 2000, 2001, 2002, 2003 RiskMap srl; (C) 2003, 2004, 2005, 2006, 2007, 2008
//! StatPro Italia srl; (C) 2007, 2008, 2009, 2015 Ferdinando Ametrano; (C) 2007, 2009 Roland
//! Lichters; (C) 2015 Maddalena Zanzi; (C) 2015 Paolo Mazzocchi; (C) 2018 Matthias Lungwitz),
//! `ql/instruments/makeois.cpp` (Copyright (C) 2009, 2014, 2015 Ferdinando Ametrano; (C) 2015
//! Paolo Mazzocchi; (C) 2017 Joseph Jeisman; (C) 2017 Fabrice Lecuyer),
//! `ql/instruments/overnightindexedswap.cpp` (Copyright (C) 2009 Roland Lichters; (C) 2009
//! Ferdinando Ametrano; (C) 2017 Joseph Jeisman; (C) 2017 Fabrice Lecuyer),
//! `ql/instruments/fixedvsfloatingswap.cpp` (Copyright (C) 2000, 2001, 2002, 2003 RiskMap srl;
//! (C) 2003, 2004, 2005, 2006, 2007 StatPro Italia srl; (C) 2007 Ferdinando Ametrano),
//! `ql/cashflows/overnightindexedcoupon.cpp` (Copyright (C) 2009 Roland Lichters; (C) 2009
//! Ferdinando Ametrano; (C) 2014 Peter Caspers; (C) 2017 Joseph Jeisman; (C) 2017 Fabrice
//! Lecuyer), `ql/cashflows/overnightindexedcouponpricer.cpp` (Copyright (C) 2009 Roland
//! Lichters; (C) 2009 Ferdinando Ametrano; (C) 2014 Peter Caspers; (C) 2016 Stefano Fondi;
//! (C) 2017 Joseph Jeisman; (C) 2017 Fabrice Lecuyer), `ql/indexes/ibor/tonar.hpp` (Copyright
//! (C) 2022 Jonghee Lee), `ql/indexes/iborindex.cpp` (Copyright (C) 2000, 2001, 2002, 2003
//! RiskMap srl; (C) 2003, 2004, 2005, 2006, 2008, 2009 StatPro Italia srl; (C) 2009 Ferdinando
//! Ametrano), `ql/termstructures/iterativebootstrap.hpp` (Copyright (C) 2008, 2011, 2015
//! Ferdinando Ametrano; (C) 2007 Chris Kenyon; (C) 2007 StatPro Italia srl; (C) 2015 Paolo
//! Mazzocchi), `ql/termstructures/yield/bootstraptraits.hpp` (Copyright (C) 2005, 2007 StatPro
//! Italia srl; (C) 2011 Ferdinando Ametrano; (C) 2007 Chris Kenyon; (C) 2019 SoftSolutions!
//! S.r.l.), `ql/termstructures/yield/piecewiseyieldcurve.hpp` (Copyright (C) 2005, 2006, 2007,
//! 2008 StatPro Italia srl; (C) 2007, 2008, 2009 Ferdinando Ametrano; (C) 2007 Chris Kenyon)
//! and `ql/pricingengines/swap/discountingswapengine.cpp` (Copyright (C) 2007, 2009 StatPro
//! Italia srl; (C) 2011 Ferdinando Ametrano).
//!
//! Differences from QuantLib: the evaluation date, reference date and all conventions are
//! explicit values (no global settings, handles or observers); no past fixings, lookback,
//! lockout, arithmetic averaging, spread, forward start or separate discount curve.
//! Conventions have no defaults, because QuantLib's defaults are not market conventions.

use crate::curves::YieldCurve;
use crate::math::interpolation::Linear;
use crate::math::solver1d::{brent, close_enough};
use crate::time::{
    generate_schedule, Actual365Kind, BusinessDayConvention, Calendar, Date, DateGenerationRule,
    DayCount, HolidayCalendar, Period, ScheduleSpec, TimeUnit,
};
use crate::Error;
use BusinessDayConvention::{Following, ModifiedFollowing};

/// Bounds of the bootstrap's search (QuantLib `detail::avgRate` and `detail::maxRate`).
const AVG_RATE: f64 = 0.05;
const MAX_RATE: f64 = 1.0;
/// Maximum evaluations of the root finder per node (QuantLib `MAX_FUNCTION_EVALUATIONS`).
const MAX_FUNCTION_EVALUATIONS: usize = 100;

/// An overnight index: fixed on its calendar with no fixing delay, for one business day.
#[derive(Debug, Clone, PartialEq)]
pub struct OvernightIndex {
    pub calendar: HolidayCalendar,
    pub day_count: DayCount,
}

/// TONAR: Japan calendar, Actual/365 Fixed (QuantLib `Tonar`).
pub fn tonar() -> OvernightIndex {
    OvernightIndex {
        calendar: HolidayCalendar::from(Calendar::Japan),
        day_count: DayCount::Actual365Fixed(Actual365Kind::Standard),
    }
}

/// An instrument of the curve, with its quoted rate as a decimal.
#[derive(Debug, Clone, PartialEq)]
pub enum OisCurveInstrument {
    /// From the evaluation date (moved to a business day) to the next business day.
    OvernightDeposit { rate: f64 },
    /// `payment_frequency` is per year (1, 2, 4 or 12) and `payment_lag` in business days.
    Ois {
        tenor: Period,
        rate: f64,
        settlement_days: u32,
        payment_frequency: u32,
        payment_lag: u32,
    },
}

/// How the curve interpolates between its nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OisInterpolation {
    /// Linear on the logarithm of the discount factor (QuantLib `LogLinear`).
    LogLinear,
}

/// Everything needed to build an OIS curve.
#[derive(Debug, Clone, PartialEq)]
pub struct OisCurveSpec {
    pub evaluation_date: Date,
    pub reference_date: Date,
    pub index: OvernightIndex,
    pub curve_day_count: DayCount,
    pub instruments: Vec<OisCurveInstrument>,
    pub interpolation: OisInterpolation,
    /// Accuracy on the discount factors (QuantLib uses 1e-12).
    pub accuracy: f64,
}

/// How the curve reprices one input instrument, in the order of `OisCurveSpec::instruments`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuoteRepricing {
    pub rate: f64,
    pub start_date: Date,
    pub maturity_date: Date,
    pub pillar_date: Date,
    pub latest_relevant_date: Date,
    /// False when the pillar is not after the reference date; the instrument is then dropped.
    pub used_in_bootstrap: bool,
    pub implied_rate: Option<f64>,
}

/// A bootstrapped curve and the repricing of its inputs.
#[derive(Debug, Clone, PartialEq)]
pub struct OisCurve {
    pub curve: YieldCurve,
    pub quote_repricing: Vec<QuoteRepricing>,
}

/// One period of an overnight-compounded leg and the matching fixed-leg period.
#[derive(Debug, Clone, Copy)]
struct OisPeriod {
    start: Date,
    end: Date,
    accrual: f64,
    fixed_payment: Date,
    overnight_payment: Date,
}

#[derive(Debug, Clone)]
enum Helper {
    Deposit { start: Date, maturity: Date, t: f64 },
    Ois { periods: Vec<OisPeriod> },
}

fn period_of_frequency(frequency: u32) -> Result<Period, Error> {
    match frequency {
        1 => Ok(Period::new(1, TimeUnit::Years)),
        2 => Ok(Period::new(6, TimeUnit::Months)),
        4 => Ok(Period::new(3, TimeUnit::Months)),
        12 => Ok(Period::new(1, TimeUnit::Months)),
        other => Err(Error::InvalidInput {
            reason: format!("unsupported payment frequency ({other})"),
        }),
    }
}

/// QuantLib `allowsEndOfMonth`.
fn allows_end_of_month(tenor: Period) -> bool {
    match tenor.unit {
        TimeUnit::Months => tenor.length >= 1,
        TimeUnit::Years => tenor.length >= 1,
        TimeUnit::Days | TimeUnit::Weeks => false,
    }
}

/// QuantLib `DepositRateHelper(rate, index)` dates.
fn deposit_helper(index: &OvernightIndex, evaluation_date: Date) -> Result<Helper, Error> {
    let calendar = &index.calendar;
    let start = calendar.adjust(evaluation_date, Following)?;
    let maturity = calendar.advance(start, Period::new(1, TimeUnit::Days), Following, false)?;
    Ok(Helper::Deposit {
        start,
        maturity,
        t: index.day_count.year_fraction(start, maturity, None, None)?,
    })
}

/// QuantLib `OISRateHelper` and `MakeOIS` dates and periods, at v1.42: on a non-business
/// evaluation date, the settlement days count from the next business day.
fn ois_helper(
    index: &OvernightIndex,
    evaluation_date: Date,
    tenor: Period,
    settlement_days: u32,
    payment_frequency: u32,
    payment_lag: u32,
) -> Result<Helper, Error> {
    let calendar = &index.calendar;
    let days = |n: i32| Period::new(n, TimeUnit::Days);
    let reference = calendar.adjust(evaluation_date, Following)?;
    let spot = calendar.advance(reference, days(settlement_days as i32), Following, false)?;
    let start = calendar.adjust(spot, Following)?;
    let end_of_month = calendar.is_end_of_month(start)?;
    let mut end = start.add_period(tenor)?;
    if end_of_month && allows_end_of_month(tenor) {
        end = calendar.end_of_month(end)?;
    }
    let schedule = generate_schedule(&ScheduleSpec {
        effective_date: start,
        termination_date: end,
        tenor: period_of_frequency(payment_frequency)?,
        calendar: calendar.clone(),
        convention: ModifiedFollowing,
        termination_convention: ModifiedFollowing,
        rule: DateGenerationRule::Backward,
        end_of_month,
        first_date: None,
        next_to_last_date: None,
    })?;
    let periods = schedule
        .dates
        .windows(2)
        .map(|w| {
            Ok(OisPeriod {
                start: w[0],
                end: w[1],
                accrual: index.day_count.year_fraction(w[0], w[1], None, None)?,
                // the fixed leg adjusts with the schedule's convention (FixedVsFloatingSwap)
                fixed_payment: calendar.advance(
                    w[1],
                    days(payment_lag as i32),
                    ModifiedFollowing,
                    false,
                )?,
                overnight_payment: calendar.advance(
                    w[1],
                    days(payment_lag as i32),
                    Following,
                    false,
                )?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(Helper::Ois { periods })
}

impl Helper {
    fn start(&self) -> Date {
        match self {
            Helper::Deposit { start, .. } => *start,
            Helper::Ois { periods } => periods[0].start,
        }
    }

    fn maturity(&self) -> Date {
        match self {
            Helper::Deposit { maturity, .. } => *maturity,
            Helper::Ois { periods } => periods[periods.len() - 1].end,
        }
    }

    /// The last date the instrument depends on; also its pillar (QuantLib `LastRelevantDate`).
    fn latest_relevant_date(&self) -> Date {
        match self {
            Helper::Deposit { maturity, .. } => *maturity,
            Helper::Ois { periods } => {
                let last = &periods[periods.len() - 1];
                self.maturity()
                    .max(last.fixed_payment)
                    .max(last.overnight_payment)
            }
        }
    }

    /// QuantLib `impliedQuote`. `reference_date` is the curve's: flows paid on or before it
    /// have occurred. Overnight compounding is the discount ratio of `docs/spec_ois.md` 2.4,
    /// and the arithmetic follows QuantLib's order.
    fn implied_quote(
        &self,
        reference_date: Date,
        discount: &dyn Fn(Date) -> Result<f64, Error>,
    ) -> Result<f64, Error> {
        match self {
            Helper::Deposit { start, maturity, t } => {
                Ok((discount(*start)? / discount(*maturity)? - 1.0) / t)
            }
            Helper::Ois { periods } => {
                let (mut overnight_npv, mut fixed_bps) = (0.0, 0.0);
                for p in periods {
                    if p.overnight_payment > reference_date {
                        let compound = discount(p.start)? / discount(p.end)?;
                        let rate = (compound - 1.0) / p.accrual;
                        overnight_npv += rate * p.accrual * 1.0 * discount(p.overnight_payment)?;
                    }
                }
                for p in periods {
                    if p.fixed_payment > reference_date {
                        fixed_bps += 1.0 * p.accrual * discount(p.fixed_payment)?;
                    }
                }
                let d = discount(reference_date)?;
                let basis_point = 1.0e-4;
                Ok(-(-(overnight_npv / d) * -1.0) / (-(basis_point * fixed_bps / d) / basis_point))
            }
        }
    }
}

/// Bootstraps the curve (QuantLib `IterativeBootstrap` with `Discount` traits), one Brent
/// search per node from the shortest pillar; a local interpolation needs one pass only.
pub fn build_ois_curve(spec: &OisCurveSpec) -> Result<OisCurve, Error> {
    let OisInterpolation::LogLinear = spec.interpolation;
    if spec.instruments.is_empty() {
        return Err(Error::InvalidInput {
            reason: "no bootstrap helpers given".into(),
        });
    }
    let helpers = spec
        .instruments
        .iter()
        .map(|i| match i {
            OisCurveInstrument::OvernightDeposit { .. } => {
                deposit_helper(&spec.index, spec.evaluation_date)
            }
            OisCurveInstrument::Ois {
                tenor,
                settlement_days,
                payment_frequency,
                payment_lag,
                ..
            } => ois_helper(
                &spec.index,
                spec.evaluation_date,
                *tenor,
                *settlement_days,
                *payment_frequency,
                *payment_lag,
            ),
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let rate_of = |i: usize| match spec.instruments[i] {
        OisCurveInstrument::OvernightDeposit { rate } | OisCurveInstrument::Ois { rate, .. } => {
            rate
        }
    };

    // alive helpers sorted by pillar
    let reference = spec.reference_date;
    let mut order: Vec<usize> = (0..helpers.len()).collect();
    order.sort_by_key(|&i| helpers[i].latest_relevant_date());
    if helpers[order[order.len() - 1]].latest_relevant_date() <= reference {
        return Err(Error::InvalidInput {
            reason: "all instruments expired".into(),
        });
    }
    order.retain(|&i| helpers[i].latest_relevant_date() > reference);

    let day_count = &spec.curve_day_count;
    let time = |d: Date| day_count.year_fraction(reference, d, None, None);
    let mut dates = vec![reference];
    let mut times = vec![time(reference)?];
    for &i in &order {
        let pillar = helpers[i].latest_relevant_date();
        if dates[dates.len() - 1] == pillar {
            return Err(Error::InvalidInput {
                reason: format!("more than one instrument with pillar {pillar}"),
            });
        }
        dates.push(pillar);
        times.push(time(pillar)?);
    }
    let max_time = times[times.len() - 1];

    let mut data = vec![1.0; dates.len()];
    for (position, &i) in order.iter().enumerate() {
        let node = position + 1;
        let dt = times[node] - times[node - 1];
        let min = data[node - 1] * (-MAX_RATE * dt).exp();
        let max = data[node - 1] * (MAX_RATE * dt).exp();
        let mut guess = if node == 1 {
            1.0 / (1.0 + AVG_RATE * times[1])
        } else {
            let r = -data[node - 1].ln() / times[node - 1];
            (-r * times[node]).exp()
        };
        if guess >= max {
            guess = max - (max - min) / 5.0;
        } else if guess <= min {
            guess = min + (max - min) / 5.0;
        }
        let logs: Vec<f64> = data[..=node].iter().map(|d| d.ln()).collect();
        let knots = &times[..=node];
        let error = |x: f64| {
            let mut y = logs.clone();
            y[node] = x.ln();
            let line = Linear::new(knots.to_vec(), y)?;
            let discount = |d: Date| {
                let t = time(d)?;
                if t < 0.0 {
                    return Err(Error::InvalidInput {
                        reason: format!("negative time ({t}) given"),
                    });
                }
                if !(t <= max_time || close_enough(t, max_time)) {
                    return Err(Error::OutsideCurveRange {
                        reason: format!("time ({t}) is past max curve time ({max_time})"),
                    });
                }
                Ok(line.value(t).exp())
            };
            Ok(rate_of(i) - helpers[i].implied_quote(reference, &discount)?)
        };
        data[node] = brent(
            error,
            spec.accuracy,
            guess,
            min,
            max,
            MAX_FUNCTION_EVALUATIONS,
        )?;
    }

    let curve = YieldCurve::discount_factors(dates, data, day_count.clone())?;
    let quote_repricing = helpers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let used_in_bootstrap = h.latest_relevant_date() > reference;
            let implied_rate = if used_in_bootstrap {
                Some(h.implied_quote(reference, &|d| curve.discount(d))?)
            } else {
                None
            };
            Ok(QuoteRepricing {
                rate: rate_of(i),
                start_date: h.start(),
                maturity_date: h.maturity(),
                pillar_date: h.latest_relevant_date(),
                latest_relevant_date: h.latest_relevant_date(),
                used_in_bootstrap,
                implied_rate,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(OisCurve {
        curve,
        quote_repricing,
    })
}
