//! Discount curves of a currency collateralized in another, bootstrapped from FX swaps and
//! cross-currency basis swaps with overnight-indexed legs, of constant or resetting (mark-to-market)
//! notional.
//!
//! Reproduces QuantLib v1.42 (modified BSD license; see THIRD_PARTY_NOTICES):
//! `ql/termstructures/yield/ratehelpers.cpp` (Copyright (C) 2000, 2001, 2002, 2003 RiskMap srl;
//! (C) 2003, 2004, 2005, 2006, 2007, 2008 StatPro Italia srl; (C) 2007, 2008, 2009, 2015 Ferdinando
//! Ametrano; (C) 2007, 2009 Roland Lichters; (C) 2015 Maddalena Zanzi; (C) 2015 Paolo Mazzocchi;
//! (C) 2018 Matthias Lungwitz), `FxSwapRateHelper`; and
//! `ql/experimental/termstructures/crosscurrencyratehelpers.cpp` (Copyright (C) 2021 Marcin Rybacki;
//! (C) 2025 Uzair Beg), `ConstNotionalCrossCurrencyBasisSwapRateHelper` and
//! `MtMCrossCurrencyBasisSwapRateHelper`.
//!
//! Conventions (QuantLib's): the pair is base/quote, so the FX rate is units of the quote currency
//! per unit of the base currency. The bootstrapped curve discounts the currency that is not the
//! collateral. Every leg has a notional of 1 in its own currency, with the notionals exchanged at
//! the start and the end. The overnight legs forecast from their indices' curves; a coupon pays
//! `P(start) / P(end) - 1`, the daily compounding on the same curve.
//!
//! Differences from QuantLib: the schedule calendar is given whole (QuantLib's `JointCalendar` is
//! [`HolidayCalendar::joined_with`] here); no fixings before the evaluation date.

use crate::bootstrap::{bootstrap_local, LocalInterpolation};
use crate::curves::YieldCurve;
use crate::ois::OvernightIndex;
use crate::time::{
    generate_schedule, BusinessDayConvention, Date, DateGenerationRule, DayCount, HolidayCalendar, Period, ScheduleSpec, TimeUnit,
};
use crate::Error;

fn invalid<T>(reason: String) -> Result<T, Error> {
    Err(Error::InvalidInput { reason })
}

/// Which leg of a basis swap resets its notional to the FX forward at each period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResettingLeg {
    /// Constant notionals (QuantLib `ConstNotionalCrossCurrencyBasisSwapRateHelper`).
    None,
    Base,
    Quote,
}

/// One quote of the curve.
#[derive(Debug, Clone, PartialEq)]
pub enum XccyInstrument {
    /// FX swap points: the forward minus the spot, in quote currency per unit of base currency
    /// (QuantLib `FxSwapRateHelper`).
    FxSwap { tenor: Period, fixing_days: u32, forward_points: f64 },
    /// A basis swap: the spread, as a decimal, added to one leg (QuantLib
    /// `ConstNotionalCrossCurrencyBasisSwapRateHelper` and `MtMCrossCurrencyBasisSwapRateHelper`).
    BasisSwap {
        tenor: Period,
        fixing_days: u32,
        /// Payments per year.
        payment_frequency: u32,
        basis: f64,
        basis_on_base_leg: bool,
        resetting: ResettingLeg,
    },
}

/// One currency of the pair: its overnight index and the curve it forecasts with.
#[derive(Debug, Clone, PartialEq)]
pub struct XccyLeg {
    pub index: OvernightIndex,
    pub forecast: YieldCurve,
}

/// Everything needed to build the curve.
#[derive(Debug, Clone, PartialEq)]
pub struct XccyCurveSpec {
    /// Also the curve's reference date.
    pub evaluation_date: Date,
    pub day_count: DayCount,
    pub base: XccyLeg,
    pub quote: XccyLeg,
    /// True when the base currency is the collateral; the curve then discounts the quote currency.
    pub base_is_collateral: bool,
    /// Discounts the collateral currency.
    pub collateral_curve: YieldCurve,
    /// Units of quote currency per unit of base currency, for the FX swaps.
    pub fx_spot: f64,
    /// Calendar of the instruments; for basis swaps, a business day for both indices.
    pub calendar: HolidayCalendar,
    pub convention: BusinessDayConvention,
    pub end_of_month: bool,
    pub accuracy: f64,
    pub instruments: Vec<XccyInstrument>,
}

/// How the curve reprices one quote, in the order of `XccyCurveSpec::instruments`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct XccyQuoteRepricing {
    pub quote: f64,
    pub pillar_date: Date,
    pub implied_quote: f64,
}

/// A bootstrapped curve and the repricing of its quotes.
#[derive(Debug, Clone, PartialEq)]
pub struct XccyCurve {
    pub curve: YieldCurve,
    pub quote_repricing: Vec<XccyQuoteRepricing>,
}

/// One period of an overnight-indexed leg with a payment lag of 0.
#[derive(Debug, Clone, Copy)]
struct Period1 {
    start: Date,
    end: Date,
    accrual: f64,
}

#[derive(Debug, Clone)]
enum Helper {
    FxSwap { earliest: Date, latest: Date, forward_points: f64 },
    BasisSwap {
        base: Vec<Period1>,
        quote: Vec<Period1>,
        start: Date,
        maturity: Date,
        basis: f64,
        basis_on_base_leg: bool,
        resetting: ResettingLeg,
    },
}

impl Helper {
    fn pillar(&self) -> Date {
        match self {
            Helper::FxSwap { latest, .. } => *latest,
            Helper::BasisSwap { maturity, .. } => *maturity,
        }
    }

    fn quote(&self) -> f64 {
        match self {
            Helper::FxSwap { forward_points, .. } => *forward_points,
            Helper::BasisSwap { basis, .. } => *basis,
        }
    }
}

fn period_of_frequency(frequency: u32) -> Result<Period, Error> {
    match frequency {
        1 => Ok(Period::new(1, TimeUnit::Years)),
        2 => Ok(Period::new(6, TimeUnit::Months)),
        4 => Ok(Period::new(3, TimeUnit::Months)),
        12 => Ok(Period::new(1, TimeUnit::Months)),
        other => invalid(format!("unsupported payment frequency ({other})")),
    }
}

fn helper_of(spec: &XccyCurveSpec, instrument: &XccyInstrument) -> Result<Helper, Error> {
    let calendar = &spec.calendar;
    let reference = calendar.adjust(spec.evaluation_date, BusinessDayConvention::Following)?;
    match instrument {
        XccyInstrument::FxSwap { tenor, fixing_days, forward_points } => {
            let earliest =
                calendar.advance(reference, Period::new(*fixing_days as i32, TimeUnit::Days), BusinessDayConvention::Following, false)?;
            let latest = calendar.advance(earliest, *tenor, spec.convention, spec.end_of_month)?;
            Ok(Helper::FxSwap { earliest, latest, forward_points: *forward_points })
        }
        XccyInstrument::BasisSwap { tenor, fixing_days, payment_frequency, basis, basis_on_base_leg, resetting } => {
            let frequency = period_of_frequency(*payment_frequency)?;
            let earliest = calendar.advance(reference, Period::new(*fixing_days as i32, TimeUnit::Days), spec.convention, false)?;
            let schedule = generate_schedule(&ScheduleSpec {
                effective_date: earliest,
                termination_date: earliest.add_period(*tenor)?,
                tenor: frequency,
                calendar: calendar.clone(),
                convention: spec.convention,
                termination_convention: spec.convention,
                rule: DateGenerationRule::Backward,
                end_of_month: spec.end_of_month,
                first_date: None,
                next_to_last_date: None,
            })?;
            let leg = |index: &OvernightIndex| -> Result<Vec<Period1>, Error> {
                schedule
                    .dates
                    .windows(2)
                    .map(|w| Ok(Period1 { start: w[0], end: w[1], accrual: index.day_count.year_fraction(w[0], w[1], None, None)? }))
                    .collect()
            };
            Ok(Helper::BasisSwap {
                base: leg(&spec.base.index)?,
                quote: leg(&spec.quote.index)?,
                start: schedule.dates[0],
                maturity: *schedule.dates.last().expect("a schedule has dates"),
                basis: *basis,
                basis_on_base_leg: *basis_on_base_leg,
                resetting: *resetting,
            })
        }
    }
}

/// The value and the annuity of a leg of notional 1 with the notionals exchanged (QuantLib
/// `npvbpsConstNotionalLeg`).
fn constant_leg(periods: &[Period1], start: Date, maturity: Date, forecast: &YieldCurve, discount: &YieldCurve) -> Result<(f64, f64), Error> {
    let (mut npv, mut bps) = (0.0, 0.0);
    for p in periods {
        let d = discount.discount(p.end)?;
        npv += (forecast.discount(p.start)? / forecast.discount(p.end)? - 1.0) * d;
        bps += p.accrual * d;
    }
    npv += -discount.discount(start)? + discount.discount(maturity)?;
    Ok((npv, bps))
}

/// The value and the annuity of a leg whose notional resets to the FX forward at each period start,
/// lending at the start and receiving the amount with interest at the end (QuantLib
/// `ResettingLegCalculator`). `other` discounts the other currency.
fn resetting_leg(periods: &[Period1], forecast: &YieldCurve, discount: &YieldCurve, other: &YieldCurve) -> Result<(f64, f64), Error> {
    let (mut npv, mut bps) = (0.0, 0.0);
    for p in periods {
        let notional = other.discount(p.start)? / discount.discount(p.start)?;
        let (d_start, d_end) = (discount.discount(p.start)?, discount.discount(p.end)?);
        let growth = forecast.discount(p.start)? / forecast.discount(p.end)?;
        npv += notional * d_end * growth - notional * d_start;
        bps += notional * d_end * p.accrual;
    }
    Ok((npv, bps))
}

fn implied_quote(spec: &XccyCurveSpec, helper: &Helper, curve: &YieldCurve) -> Result<f64, Error> {
    let collateral = &spec.collateral_curve;
    let (base_discount, quote_discount) = if spec.base_is_collateral { (collateral, curve) } else { (curve, collateral) };
    match helper {
        Helper::FxSwap { earliest, latest, .. } => {
            let coll_ratio = collateral.discount(*earliest)? / collateral.discount(*latest)?;
            let ratio = curve.discount(*earliest)? / curve.discount(*latest)?;
            Ok(if spec.base_is_collateral {
                (ratio / coll_ratio - 1.0) * spec.fx_spot
            } else {
                (coll_ratio / ratio - 1.0) * spec.fx_spot
            })
        }
        Helper::BasisSwap { base, quote, start, maturity, basis_on_base_leg, resetting, .. } => {
            let (base_forecast, quote_forecast) = (&spec.base.forecast, &spec.quote.forecast);
            let ((npv_base, bps_base), (npv_quote, bps_quote)) = match resetting {
                ResettingLeg::None => (
                    constant_leg(base, *start, *maturity, base_forecast, base_discount)?,
                    constant_leg(quote, *start, *maturity, quote_forecast, quote_discount)?,
                ),
                ResettingLeg::Base => (
                    resetting_leg(base, base_forecast, base_discount, quote_discount)?,
                    constant_leg(quote, *start, *maturity, quote_forecast, quote_discount)?,
                ),
                ResettingLeg::Quote => (
                    constant_leg(base, *start, *maturity, base_forecast, base_discount)?,
                    resetting_leg(quote, quote_forecast, quote_discount, base_discount)?,
                ),
            };
            let bps = if *basis_on_base_leg { -bps_base } else { bps_quote };
            if bps == 0.0 {
                return invalid("null BPS".into());
            }
            Ok(-(npv_quote - npv_base) / bps)
        }
    }
}

/// Bootstraps the discount curve of the currency that is not the collateral, log-linear in the
/// discount factors (QuantLib `PiecewiseLogLinearDiscount` with these helpers).
pub fn bootstrap_xccy_curve(spec: &XccyCurveSpec) -> Result<XccyCurve, Error> {
    if spec.fx_spot.is_nan() || spec.fx_spot <= 0.0 {
        return invalid(format!("the FX spot must be positive, not {}", spec.fx_spot));
    }
    let helpers = spec.instruments.iter().map(|i| helper_of(spec, i)).collect::<Result<Vec<_>, Error>>()?;
    let mut order: Vec<usize> = (0..helpers.len()).collect();
    order.sort_by_key(|&i| helpers[i].pillar());
    let pillars: Vec<Date> = order.iter().map(|&i| helpers[i].pillar()).collect();
    if let Some(w) = pillars.windows(2).find(|w| w[0] == w[1]) {
        return invalid(format!("more than one instrument with pillar {}", w[0]));
    }
    let curve = bootstrap_local(spec.evaluation_date, &spec.day_count, LocalInterpolation::LogLinearDiscount, &pillars, spec.accuracy, |k, curve| {
        let h = &helpers[order[k]];
        Ok(h.quote() - implied_quote(spec, h, curve)?)
    })?;
    let quote_repricing = helpers
        .iter()
        .map(|h| Ok(XccyQuoteRepricing { quote: h.quote(), pillar_date: h.pillar(), implied_quote: implied_quote(spec, h, &curve)? }))
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(XccyCurve { curve, quote_repricing })
}
