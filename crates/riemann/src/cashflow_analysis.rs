//! Valuation of a leg at a flat yield: present value, duration, convexity, BPV, yield value
//! of a basis point and internal rate of return.
//!
//! Reproduces QuantLib v1.42 `CashFlows::npv/duration/convexity/basisPointValue/
//! yieldValueBasisPoint` for an `InterestRate` and `CashFlows::IrrFinder` (`ql/cashflows/
//! cashflows.cpp`, Copyright (C) 2005, 2006 StatPro Italia srl; (C) 2005 Charles Whitmore;
//! (C) 2007-2012 Ferdinando Ametrano; (C) 2008 Toyin Akin; modified BSD license).
//! See THIRD_PARTY_NOTICES.
//!
//! In v1.42, `npv` multiplies stepwise discount factors while the durations and convexity
//! discount the accumulated time. QuantLib rewrote these functions after v1.42 (stepwise
//! products everywhere); v1.42, the version used for verification, is reproduced here.

use crate::autodiff::Operand;
use crate::cashflows::CashFlow;
use crate::curves::YieldCurve;
use crate::interest_rate::{Compounding, InterestRate};
use crate::math::solver1d::newton_safe;
use crate::time::{Date, DayCount, Period, TimeUnit};
use crate::Error;

/// Duration measures (QuantLib `Duration::Type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DurationType {
    Simple,
    Modified,
    Macaulay,
}

/// The yield's day count, compounding and frequency, without the rate.
#[derive(Debug, Clone, PartialEq)]
pub struct YieldConvention {
    pub day_count: DayCount,
    pub compounding: Compounding,
    pub frequency: u32,
}

impl YieldConvention {
    pub fn rate(&self, rate: f64) -> Result<InterestRate, Error> {
        InterestRate::new(rate, self.day_count.clone(), self.compounding, self.frequency)
    }
}

/// Settings of the yield solver; the defaults are QuantLib's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YieldSolver {
    pub accuracy: f64,
    pub max_iterations: usize,
    pub guess: f64,
}

impl Default for YieldSolver {
    fn default() -> Self {
        YieldSolver { accuracy: 1.0e-10, max_iterations: 100, guess: 0.05 }
    }
}

/// Year fraction between `last_date` and the cash flow date for the yield's day count
/// (QuantLib `getStepwiseDiscountTime`).
fn stepwise_discount_time(cf: &CashFlow, day_count: &DayCount, npv_date: Date, last_date: Date) -> Result<f64, Error> {
    let cf_date = cf.date();
    match cf {
        CashFlow::FixedRateCoupon(c) => {
            let (rs, re) = (Some(c.ref_start), Some(c.ref_end));
            if last_date != c.accrual_start {
                let coupon_period = day_count.year_fraction(c.accrual_start, cf_date, rs, re)?;
                let accrued_period = day_count.year_fraction(c.accrual_start, last_date, rs, re)?;
                Ok(coupon_period - accrued_period)
            } else {
                day_count.year_fraction(last_date, cf_date, rs, re)
            }
        }
        CashFlow::Redemption { .. } => {
            // we don't have a previous coupon date, so we fake it
            let ref_start = if last_date == npv_date {
                cf_date.add_period(Period::new(-1, TimeUnit::Years))?
            } else {
                last_date
            };
            day_count.year_fraction(last_date, cf_date, Some(ref_start), Some(cf_date))
        }
    }
}

/// Cash flows that have not occurred at `settlement`, with their amounts.
fn alive(leg: &[CashFlow], settlement: Date, include_settlement_date_flows: bool) -> Result<Vec<(&CashFlow, f64)>, Error> {
    leg.iter()
        .filter(|cf| !cf.has_occurred(settlement, include_settlement_date_flows))
        .map(|cf| Ok((cf, cf.amount()?)))
        .collect()
}

/// Present value at `npv_date` of the cash flows after `settlement`, discounted at `y`.
pub fn npv<T: Operand>(leg: &[CashFlow], y: &InterestRate<T>, incl: bool, settlement: Date, npv_date: Date) -> Result<T, Error> {
    let (mut npv, mut discount, mut last_date) = (y.rate.like(0.0), y.rate.like(1.0), npv_date);
    for (cf, amount) in alive(leg, settlement, incl)? {
        let b = y.discount_factor(stepwise_discount_time(cf, &y.day_count, npv_date, last_date)?)?;
        discount = discount * b;
        last_date = cf.date();
        npv = npv + discount * amount;
    }
    Ok(npv)
}

/// Present value at `npv_date` of the cash flows after `settlement`, discounted on `curve`
/// (QuantLib `CashFlows::npv` with a yield term structure).
pub fn npv_on_curve<T: Operand>(
    leg: &[CashFlow],
    curve: &YieldCurve<T>,
    incl: bool,
    settlement: Date,
    npv_date: Date,
) -> Result<T, Error> {
    if leg.is_empty() {
        return Ok(curve.like(0.0));
    }
    let mut total = curve.like(0.0);
    for (cf, amount) in alive(leg, settlement, incl)? {
        total = total + curve.discount(cf.date())? * amount;
    }
    Ok(total / curve.discount(npv_date)?)
}

fn simple_duration(leg: &[CashFlow], y: &InterestRate, incl: bool, settlement: Date, npv_date: Date) -> Result<f64, Error> {
    let (mut p, mut dpdy, mut t, mut last_date) = (0.0, 0.0, 0.0, npv_date);
    for (cf, c) in alive(leg, settlement, incl)? {
        t += stepwise_discount_time(cf, &y.day_count, npv_date, last_date)?;
        let b = y.discount_factor(t)?;
        p += c * b;
        dpdy += t * c * b;
        last_date = cf.date();
    }
    Ok(if p == 0.0 { 0.0 } else { dpdy / p })
}

fn modified_duration(leg: &[CashFlow], y: &InterestRate, incl: bool, settlement: Date, npv_date: Date) -> Result<f64, Error> {
    let (r, n) = (y.rate, y.frequency as f64);
    let (mut p, mut t, mut dpdy, mut last_date) = (0.0, 0.0, 0.0, npv_date);
    for (cf, c) in alive(leg, settlement, incl)? {
        t += stepwise_discount_time(cf, &y.day_count, npv_date, last_date)?;
        let b = y.discount_factor(t)?;
        p += c * b;
        match y.compounding {
            Compounding::Simple => dpdy -= c * b * b * t,
            Compounding::Compounded => dpdy -= c * t * b / (1.0 + r / n),
            Compounding::Continuous => dpdy -= c * b * t,
        }
        last_date = cf.date();
    }
    Ok(if p == 0.0 { 0.0 } else { -dpdy / p })
}

/// Duration of the cash flows after `settlement` at yield `y`.
pub fn duration(
    leg: &[CashFlow],
    y: &InterestRate,
    kind: DurationType,
    incl: bool,
    settlement: Date,
    npv_date: Date,
) -> Result<f64, Error> {
    if leg.is_empty() {
        return Ok(0.0);
    }
    match kind {
        DurationType::Simple => simple_duration(leg, y, incl, settlement, npv_date),
        DurationType::Modified => modified_duration(leg, y, incl, settlement, npv_date),
        DurationType::Macaulay => {
            if y.compounding != Compounding::Compounded {
                return Err(Error::InvalidInput { reason: "compounded rate required".into() });
            }
            Ok((1.0 + y.rate / y.frequency as f64) * modified_duration(leg, y, incl, settlement, npv_date)?)
        }
    }
}

/// Convexity of the cash flows after `settlement` at yield `y`.
pub fn convexity(leg: &[CashFlow], y: &InterestRate, incl: bool, settlement: Date, npv_date: Date) -> Result<f64, Error> {
    if leg.is_empty() {
        return Ok(0.0);
    }
    let (r, n) = (y.rate, y.frequency as f64);
    let (mut p, mut t, mut d2pdy2, mut last_date) = (0.0, 0.0, 0.0, npv_date);
    for (cf, c) in alive(leg, settlement, incl)? {
        t += stepwise_discount_time(cf, &y.day_count, npv_date, last_date)?;
        let b = y.discount_factor(t)?;
        p += c * b;
        match y.compounding {
            Compounding::Simple => d2pdy2 += c * 2.0 * b * b * b * t * t,
            Compounding::Compounded => d2pdy2 += c * b * t * (n * t + 1.0) / (n * (1.0 + r / n) * (1.0 + r / n)),
            Compounding::Continuous => d2pdy2 += c * b * t * t,
        }
        last_date = cf.date();
    }
    Ok(if p == 0.0 { 0.0 } else { d2pdy2 / p })
}

/// Price change for a 1bp yield increase, second order (QuantLib `basisPointValue`).
pub fn basis_point_value(leg: &[CashFlow], y: &InterestRate, incl: bool, settlement: Date, npv_date: Date) -> Result<f64, Error> {
    if leg.is_empty() {
        return Ok(0.0);
    }
    let npv = npv(leg, y, incl, settlement, npv_date)?;
    let modified = duration(leg, y, DurationType::Modified, incl, settlement, npv_date)?;
    let convexity = convexity(leg, y, incl, settlement, npv_date)?;
    let mut delta = -modified * npv;
    let mut gamma = (convexity / 100.0) * npv;
    let shift = 0.0001;
    delta *= shift;
    gamma *= shift * shift;
    Ok(delta + 0.5 * gamma)
}

/// Yield change for a price change of 0.01 (QuantLib `yieldValueBasisPoint`).
pub fn yield_value_basis_point(leg: &[CashFlow], y: &InterestRate, incl: bool, settlement: Date, npv_date: Date) -> Result<f64, Error> {
    if leg.is_empty() {
        return Ok(0.0);
    }
    let npv = npv(leg, y, incl, settlement, npv_date)?;
    let modified = duration(leg, y, DurationType::Modified, incl, settlement, npv_date)?;
    let shift = 0.01;
    Ok((1.0 / (-npv * modified)) * shift)
}

fn sign(x: f64) -> i32 {
    if x > 0.0 {
        1
    } else if x < 0.0 {
        -1
    } else {
        0
    }
}

/// Internal rate of return giving present value `target` (QuantLib `CashFlows::yield` with NewtonSafe).
pub fn irr(
    leg: &[CashFlow],
    target: f64,
    convention: &YieldConvention,
    incl: bool,
    settlement: Date,
    npv_date: Date,
    solver: YieldSolver,
) -> Result<f64, Error> {
    // the cash flows must change sign relative to the price (QuantLib IrrFinder::checkSign)
    let (mut last_sign, mut sign_changes) = (sign(-target), 0);
    for (_, amount) in alive(leg, settlement, incl)? {
        let this_sign = sign(amount);
        if last_sign * this_sign < 0 {
            sign_changes += 1;
        }
        if this_sign != 0 {
            last_sign = this_sign;
        }
    }
    if sign_changes == 0 {
        return Err(Error::InvalidInput {
            reason: "the given cash flows cannot result in the given market price due to their sign".into(),
        });
    }
    // QuantLib v1.42 IrrFinder: the objective is target - NPV and the "derivative" passed to
    // NewtonSafe is the modified duration itself (later versions use NPV - target and
    // -duration * NPV). The v1.42 form is kept so that the iterations match.
    let f = |r: f64| Ok(target - npv(leg, &convention.rate(r)?, incl, settlement, npv_date)?);
    let df = |r: f64| modified_duration(leg, &convention.rate(r)?, incl, settlement, npv_date);
    newton_safe(f, df, solver.accuracy, solver.guess, solver.guess / 10.0, solver.max_iterations)
}
