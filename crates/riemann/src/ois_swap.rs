//! Overnight indexed swaps: a fixed leg against a leg compounding an overnight index daily.
//!
//! Written from QuantLib v1.42: `ql/instruments/makeois.cpp` (Copyright (C) 2009, 2014, 2015
//! Ferdinando Ametrano; (C) 2015 Paolo Mazzocchi; (C) 2017 Joseph Jeisman; (C) 2017 Fabrice
//! Lecuyer), `ql/instruments/overnightindexedswap.cpp` and
//! `ql/cashflows/overnightindexedcoupon.cpp` (Copyright (C) 2009 Roland Lichters; (C) 2009
//! Ferdinando Ametrano; (C) 2014 Peter Caspers; (C) 2017 Joseph Jeisman; (C) 2017 Fabrice Lecuyer),
//! `ql/cashflows/overnightindexedcouponpricer.cpp` (Copyright (C) 2009 Roland Lichters; (C) 2009
//! Ferdinando Ametrano; (C) 2014 Peter Caspers; (C) 2016 Stefano Fondi; (C) 2017 Joseph Jeisman;
//! (C) 2017 Fabrice Lecuyer), `ql/instruments/fixedvsfloatingswap.cpp` and
//! `ql/pricingengines/swap/discountingswapengine.cpp` (Copyright (C) 2007, 2009 StatPro Italia srl;
//! (C) 2011 Ferdinando Ametrano); modified BSD license. See THIRD_PARTY_NOTICES.
//!
//! Differences from QuantLib: the evaluation date and the fixings are arguments, with no global
//! settings or index manager. There is no lookback, lockout, observation shift or arithmetic
//! averaging, and the spread is added to the compounded rate. The leg has its full daily schedule:
//! QuantLib's optional telescopic value dates only shorten the calculation.

use crate::autodiff::{Operand, Recorder};
use crate::cashflows::FixedRateCoupon;
use crate::curves::YieldCurve;
use crate::ois::{OisCurve, OvernightIndex};
use crate::time::{
    generate_schedule, BusinessDayConvention, Date, DateGenerationRule, DayCount, Period, ScheduleSpec, TimeUnit,
};
use crate::Error;
use std::collections::BTreeMap;
use BusinessDayConvention::{Following, ModifiedFollowing};

/// Which side pays the fixed rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapSide {
    /// Pays the fixed leg and receives the overnight leg.
    Payer,
    /// Receives the fixed leg and pays the overnight leg.
    Receiver,
}

impl SwapSide {
    /// The signs of the fixed and of the overnight leg.
    fn signs(self) -> (f64, f64) {
        match self {
            SwapSide::Payer => (-1.0, 1.0),
            SwapSide::Receiver => (1.0, -1.0),
        }
    }
}

/// Everything needed to build a swap, as QuantLib `MakeOIS` with an effective and a termination date.
#[derive(Debug, Clone, PartialEq)]
pub struct OisSwapSpec {
    pub side: SwapSide,
    pub nominal: f64,
    pub effective_date: Date,
    pub termination_date: Date,
    pub index: OvernightIndex,
    pub fixed_rate: f64,
    /// The index's day count when not given.
    pub fixed_day_count: Option<DayCount>,
    /// Added to the compounded rate of each overnight coupon.
    pub spread: f64,
    /// Payments per year.
    pub fixed_frequency: u32,
    pub overnight_frequency: u32,
    /// Business days from the end of a period to its payment.
    pub payment_lag: u32,
}

/// One coupon of the overnight leg.
#[derive(Debug, Clone, PartialEq)]
pub struct OvernightCoupon {
    pub payment_date: Date,
    pub accrual_start: Date,
    pub accrual_end: Date,
    /// The index's year fraction of the period.
    pub accrual: f64,
    /// The business days from the start to the end. Each but the last is a fixing date.
    pub value_dates: Vec<Date>,
    /// The index's year fraction of each day.
    pub day_fractions: Vec<f64>,
}

/// A built swap. It does not change.
#[derive(Debug, Clone, PartialEq)]
pub struct OisSwap {
    pub spec: OisSwapSpec,
    pub fixed_leg: Vec<FixedRateCoupon>,
    pub overnight_leg: Vec<OvernightCoupon>,
}

/// The fixings of an index, by date. Fixings for other dates are ignored.
pub type Fixings = BTreeMap<Date, f64>;

/// The value of a swap to its side, at the discount curve's reference date.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OisSwapValue<T = f64> {
    pub npv: T,
    pub fixed_leg_npv: T,
    pub overnight_leg_npv: T,
    /// The change of each leg for 1bp, with the leg's sign.
    pub fixed_leg_bps: T,
    pub overnight_leg_bps: T,
    /// `None` when no fixed coupon is left to pay.
    pub fair_rate: Option<T>,
    /// `None` when no overnight coupon is left to pay.
    pub fair_spread: Option<T>,
}

/// One day of an overnight coupon.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OvernightDay {
    pub fixing_date: Date,
    pub next_date: Date,
    /// The index's year fraction of the day.
    pub day_fraction: f64,
    /// The published fixing, if given.
    pub fixing: Option<f64>,
    /// The rate implied by the forecasting curve.
    pub forward_rate: f64,
    /// The fixing up to the evaluation date, and the forward rate after it.
    pub applied_rate: f64,
}

/// The change of a swap's NPV with one quote of its OIS curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OisSwapQuoteSensitivity {
    /// Position in `OisCurveSpec::instruments`.
    pub instrument: usize,
    pub pillar_date: Date,
    pub quote: f64,
    pub d_npv: f64,
}

fn invalid<T>(reason: String) -> Result<T, Error> {
    Err(Error::InvalidInput { reason })
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

impl OisSwap {
    /// Builds the swap. Both schedules are backward and modified following on the index's calendar,
    /// and end of month when the start is. Fixed payments adjust modified following and overnight
    /// payments following, as `FixedVsFloatingSwap` and `OvernightLeg` do.
    pub fn new(spec: OisSwapSpec) -> Result<OisSwap, Error> {
        if spec.termination_date <= spec.effective_date {
            return invalid("the termination date must be after the effective date".into());
        }
        let calendar = spec.index.calendar.clone();
        let end_of_month = calendar.is_end_of_month(spec.effective_date)?;
        let schedule_of = |frequency: u32| -> Result<Vec<Date>, Error> {
            Ok(generate_schedule(&ScheduleSpec {
                effective_date: spec.effective_date,
                termination_date: spec.termination_date,
                tenor: period_of_frequency(frequency)?,
                calendar: calendar.clone(),
                convention: ModifiedFollowing,
                termination_convention: ModifiedFollowing,
                rule: DateGenerationRule::Backward,
                end_of_month,
                first_date: None,
                next_to_last_date: None,
            })?
            .dates)
        };
        let lag = Period::new(spec.payment_lag as i32, TimeUnit::Days);
        let fixed_day_count = spec.fixed_day_count.clone().unwrap_or_else(|| spec.index.day_count.clone());
        let fixed_leg = schedule_of(spec.fixed_frequency)?
            .windows(2)
            .map(|w| {
                Ok(FixedRateCoupon {
                    payment_date: calendar.advance(w[1], lag, ModifiedFollowing, false)?,
                    nominal: spec.nominal,
                    rate: spec.fixed_rate,
                    day_count: fixed_day_count.clone(),
                    accrual_start: w[0],
                    accrual_end: w[1],
                    ref_start: w[0],
                    ref_end: w[1],
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let overnight_leg = schedule_of(spec.overnight_frequency)?
            .windows(2)
            .map(|w| {
                let value_dates = daily_dates(&spec.index, w[0], w[1])?;
                let day_fractions = value_dates
                    .windows(2)
                    .map(|d| spec.index.day_count.year_fraction(d[0], d[1], None, None))
                    .collect::<Result<Vec<_>, Error>>()?;
                Ok(OvernightCoupon {
                    payment_date: calendar.advance(w[1], lag, Following, false)?,
                    accrual_start: w[0],
                    accrual_end: w[1],
                    accrual: spec.index.day_count.year_fraction(w[0], w[1], Some(w[0]), Some(w[1]))?,
                    value_dates,
                    day_fractions,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        Ok(OisSwap { spec, fixed_leg, overnight_leg })
    }
}

/// The daily schedule of `OvernightIndexedCoupon`: the business days from `start` to `end`.
fn daily_dates(index: &OvernightIndex, start: Date, end: Date) -> Result<Vec<Date>, Error> {
    let mut dates = generate_schedule(&ScheduleSpec {
        effective_date: start,
        termination_date: end,
        tenor: Period::new(1, TimeUnit::Days),
        calendar: index.calendar.clone(),
        convention: Following,
        termination_convention: Following,
        rule: DateGenerationRule::Backward,
        end_of_month: false,
        first_date: None,
        next_to_last_date: None,
    })?
    .dates;
    dates.dedup();
    if dates.len() < 2 {
        return invalid(format!("degenerate overnight schedule from {start} to {end}"));
    }
    Ok(dates)
}

/// The compounded rate of a coupon (`CompoundingOvernightIndexedCouponPricer::compute` for a date
/// at the end of the period): published fixings up to the evaluation date, today's if given, and
/// the ratio of the forecasting curve's discount factors after that.
fn compounded_rate<T: Operand>(
    coupon: &OvernightCoupon,
    forecast: &YieldCurve<T>,
    evaluation_date: Date,
    fixings: &Fixings,
) -> Result<T, Error> {
    let n = coupon.value_dates.len() - 1;
    let mut compound = 1.0;
    let mut i = 0;
    while i < n && coupon.value_dates[i] < evaluation_date {
        let date = coupon.value_dates[i];
        let Some(&fixing) = fixings.get(&date) else {
            return invalid(format!("Missing overnight index fixing for {date}"));
        };
        compound *= 1.0 + fixing * coupon.day_fractions[i];
        i += 1;
    }
    if i < n && coupon.value_dates[i] == evaluation_date {
        if let Some(&fixing) = fixings.get(&coupon.value_dates[i]) {
            compound *= 1.0 + fixing * coupon.day_fractions[i];
            i += 1;
        }
    }
    let factor = if i < n {
        forecast.discount(coupon.value_dates[i])? / forecast.discount(coupon.value_dates[n])? * compound
    } else {
        forecast.discount(forecast.reference_date())?.like(compound)
    };
    Ok((factor - 1.0) / coupon.accrual)
}

/// The amount of a coupon of the overnight leg (`FloatingRateCoupon::amount`).
pub fn overnight_coupon_amount<T: Operand>(
    swap: &OisSwap,
    coupon: &OvernightCoupon,
    forecast: &YieldCurve<T>,
    evaluation_date: Date,
    fixings: &Fixings,
) -> Result<T, Error> {
    Ok((compounded_rate(coupon, forecast, evaluation_date, fixings)? + swap.spec.spread)
        * (coupon.accrual * swap.spec.nominal))
}

/// The value at the discount curve's reference date (`DiscountingSwapEngine` with
/// `FixedVsFloatingSwap::fetchResults`). Flows paid on or before that date have occurred, and their
/// fixings are not needed.
pub fn ois_swap_value<T: Operand>(
    swap: &OisSwap,
    forecast: &YieldCurve<T>,
    discount: &YieldCurve<T>,
    evaluation_date: Date,
    fixings: &Fixings,
) -> Result<OisSwapValue<T>, Error> {
    let reference = discount.reference_date();
    let basis_point = 1.0e-4;
    let (fixed_sign, overnight_sign) = swap.spec.side.signs();
    let d = discount.discount(reference)?;
    let zero = d.like(0.0);

    // `Swap::isExpired` looks at the evaluation date, and not at the curve's reference date.
    let expired = swap.fixed_leg.iter().all(|c| c.payment_date <= evaluation_date)
        && swap.overnight_leg.iter().all(|c| c.payment_date <= evaluation_date);
    if expired {
        return Ok(OisSwapValue {
            npv: zero,
            fixed_leg_npv: zero,
            overnight_leg_npv: zero,
            fixed_leg_bps: zero,
            overnight_leg_bps: zero,
            fair_rate: None,
            fair_spread: None,
        });
    }

    let (mut fixed_npv, mut fixed_bps, mut any_fixed) = (zero, zero, false);
    for coupon in swap.fixed_leg.iter().filter(|c| c.payment_date > reference) {
        let df = discount.discount(coupon.payment_date)?;
        fixed_npv = fixed_npv + df * coupon.amount()?;
        let accrual = coupon.day_count.year_fraction(coupon.accrual_start, coupon.accrual_end, Some(coupon.ref_start), Some(coupon.ref_end))?;
        fixed_bps = fixed_bps + df * (coupon.nominal * accrual);
        any_fixed = true;
    }
    let (mut overnight_npv, mut overnight_bps, mut any_overnight) = (zero, zero, false);
    for coupon in swap.overnight_leg.iter().filter(|c| c.payment_date > reference) {
        let df = discount.discount(coupon.payment_date)?;
        overnight_npv = overnight_npv + overnight_coupon_amount(swap, coupon, forecast, evaluation_date, fixings)? * df;
        overnight_bps = overnight_bps + df * (swap.spec.nominal * coupon.accrual);
        any_overnight = true;
    }
    let fixed_leg_npv = fixed_npv / d * fixed_sign;
    let overnight_leg_npv = overnight_npv / d * overnight_sign;
    let fixed_leg_bps = fixed_bps * basis_point / d * fixed_sign;
    let overnight_leg_bps = overnight_bps * basis_point / d * overnight_sign;
    let npv = fixed_leg_npv + overnight_leg_npv;
    Ok(OisSwapValue {
        npv,
        fixed_leg_npv,
        overnight_leg_npv,
        fixed_leg_bps,
        overnight_leg_bps,
        fair_rate: any_fixed.then(|| zero.like(swap.spec.fixed_rate) - npv / (fixed_leg_bps / basis_point)),
        fair_spread: any_overnight.then(|| zero.like(swap.spec.spread) - npv / (overnight_leg_bps / basis_point)),
    })
}

/// The days of an overnight coupon, with the rate each one uses.
pub fn overnight_days(
    coupon: &OvernightCoupon,
    forecast: &YieldCurve,
    evaluation_date: Date,
    fixings: &Fixings,
) -> Result<Vec<OvernightDay>, Error> {
    let n = coupon.value_dates.len() - 1;
    (0..n)
        .map(|i| {
            let (start, end) = (coupon.value_dates[i], coupon.value_dates[i + 1]);
            let forward_rate = (forecast.discount(start)? / forecast.discount(end)? - 1.0) / coupon.day_fractions[i];
            let fixing = fixings.get(&start).copied();
            let applied_rate = match fixing {
                Some(f) if start <= evaluation_date => f,
                _ => forward_rate,
            };
            Ok(OvernightDay { fixing_date: start, next_date: end, day_fraction: coupon.day_fractions[i], fixing, forward_rate, applied_rate })
        })
        .collect()
}

/// The derivatives of the NPV with respect to each quote of the curve, which both forecasts and
/// discounts: automatic differentiation on the nodes, and the implicit function theorem from the
/// nodes to the quotes.
pub fn ois_swap_quote_sensitivity(
    swap: &OisSwap,
    ois: &OisCurve,
    evaluation_date: Date,
    fixings: &Fixings,
) -> Result<Vec<OisSwapQuoteSensitivity>, Error> {
    let recorder = Recorder::new();
    let lifted = ois.curve.lift(&recorder)?;
    let value = ois_swap_value(swap, &lifted.curve, &lifted.curve, evaluation_date, fixings)?;
    let vars: Vec<_> = lifted.inputs.iter().map(|(_, v)| *v).collect();
    let node = recorder.gradient(value.npv, &vars)?;
    let jacobian = ois.quote_jacobian()?;
    let n = jacobian.instruments.len();
    if node.len() != n {
        return invalid("the curve's nodes do not match its instruments".into());
    }
    Ok((0..n)
        .map(|k| OisSwapQuoteSensitivity {
            instrument: jacobian.instruments[k],
            pillar_date: jacobian.node_dates[k],
            quote: ois.quote_repricing[jacobian.instruments[k]].rate,
            d_npv: (0..n).map(|i| node[i].mean() * jacobian.d_log_discount_d_quote[i][k]).sum(),
        })
        .collect())
}
