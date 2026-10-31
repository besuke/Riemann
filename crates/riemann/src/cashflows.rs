//! Cash flows and legs (QuantLib-compatible fixed-rate coupons).
//!
//! A leg is a vector of `CashFlow` values; building a leg and valuing it are separate
//! functions. The coupon construction and accrual rules reproduce
//! `ql/cashflows/fixedratecoupon.cpp`, `ql/cashflows/cashflows.cpp` (accrued amount) and
//! `ql/cashflow.cpp` / `ql/event.cpp` (occurrence), and are treated as derived from QuantLib
//! source code (modified BSD license). QuantLib commit 70f240f29ddb84927ff654502f1d75b3de24ba43.
//! See THIRD_PARTY_NOTICES.
//!
//! Differences from QuantLib: no ex-coupon periods, no payment lag, and whether cash flows
//! on the reference date count as occurred is an argument instead of a global setting.

use crate::time::schedule::effective_tenor_and_end_of_month;
use crate::time::{BusinessDayConvention, Date, DayCount, HolidayCalendar, Period, Schedule, ScheduleSpec};
use crate::Error;

/// A fixed-rate coupon with simple interest (QuantLib `FixedRateCoupon`).
#[derive(Debug, Clone, PartialEq)]
pub struct FixedRateCoupon {
    pub payment_date: Date,
    pub nominal: f64,
    pub rate: f64,
    pub day_count: DayCount,
    pub accrual_start: Date,
    pub accrual_end: Date,
    pub ref_start: Date,
    pub ref_end: Date,
}

impl FixedRateCoupon {
    /// Simple compound factor 1 + r t, as in QuantLib `InterestRate::compoundFactor`.
    fn compound_factor(&self, start: Date, end: Date) -> Result<f64, Error> {
        let t = self.day_count.year_fraction(start, end, Some(self.ref_start), Some(self.ref_end))?;
        if t < 0.0 {
            return Err(Error::InvalidCashFlow { reason: format!("negative time ({t}) not allowed") });
        }
        Ok(1.0 + self.rate * t)
    }

    /// Coupon amount: nominal * (compound factor - 1), in QuantLib's order of operations.
    pub fn amount(&self) -> Result<f64, Error> {
        Ok(self.nominal * (self.compound_factor(self.accrual_start, self.accrual_end)? - 1.0))
    }

    /// Interest accrued up to `d` (QuantLib `FixedRateCoupon::accruedAmount`, no ex-coupon).
    pub fn accrued_amount(&self, d: Date) -> Result<f64, Error> {
        if d <= self.accrual_start || d > self.payment_date {
            return Ok(0.0);
        }
        Ok(self.nominal * (self.compound_factor(self.accrual_start, d.min(self.accrual_end))? - 1.0))
    }
}

/// A cash flow of a leg.
#[derive(Debug, Clone, PartialEq)]
pub enum CashFlow {
    FixedRateCoupon(FixedRateCoupon),
    /// Final repayment of the notional.
    Redemption { date: Date, amount: f64 },
}

impl CashFlow {
    pub fn date(&self) -> Date {
        match self {
            CashFlow::FixedRateCoupon(c) => c.payment_date,
            CashFlow::Redemption { date, .. } => *date,
        }
    }

    pub fn amount(&self) -> Result<f64, Error> {
        match self {
            CashFlow::FixedRateCoupon(c) => c.amount(),
            CashFlow::Redemption { amount, .. } => Ok(*amount),
        }
    }

    /// Whether the cash flow has occurred at `reference` (QuantLib `CashFlow::hasOccurred`).
    /// With `include_reference_date`, a flow on the reference date has not occurred yet.
    pub fn has_occurred(&self, reference: Date, include_reference_date: bool) -> bool {
        if include_reference_date {
            self.date() < reference
        } else {
            self.date() <= reference
        }
    }
}

/// The inputs of a fixed-rate leg (QuantLib `FixedRateLeg`) on a generated schedule.
#[derive(Debug, Clone, PartialEq)]
pub struct FixedRateLegSpec {
    pub notional: f64,
    pub rate: f64,
    pub day_count: DayCount,
    pub payment_calendar: HolidayCalendar,
    pub payment_convention: BusinessDayConvention,
}

/// Builds the coupons of a fixed-rate leg (QuantLib `FixedRateLeg::operator Leg`).
///
/// Irregular first and last periods get a notional reference period of one tenor,
/// computed with the schedule's calendar, convention and end-of-month rule.
pub fn fixed_rate_leg(
    schedule_spec: &ScheduleSpec,
    schedule: &Schedule,
    leg: &FixedRateLegSpec,
) -> Result<Vec<CashFlow>, Error> {
    let dates = &schedule.dates;
    let n = dates.len();
    if n < 2 {
        return Err(Error::InvalidCashFlow { reason: "schedule with fewer than two dates".into() });
    }
    let (tenor, end_of_month) = effective_tenor_and_end_of_month(schedule_spec);
    let cal = &schedule_spec.calendar;
    let conv = schedule_spec.convention;
    let payment = |end: Date| leg.payment_calendar.adjust(end, leg.payment_convention);
    let coupon = |start: Date, end: Date, ref_start: Date, ref_end: Date| -> Result<CashFlow, Error> {
        Ok(CashFlow::FixedRateCoupon(FixedRateCoupon {
            payment_date: payment(end)?,
            nominal: leg.notional,
            rate: leg.rate,
            day_count: leg.day_count.clone(),
            accrual_start: start,
            accrual_end: end,
            ref_start,
            ref_end,
        }))
    };

    let mut out = Vec::with_capacity(n - 1);
    // first period might be short or long
    let (start, end) = (dates[0], dates[1]);
    let first_ref = if !schedule.is_regular[0] {
        cal.advance(end, Period::new(-tenor.length, tenor.unit), conv, end_of_month)?
    } else {
        start
    };
    out.push(coupon(start, end, first_ref, end)?);
    // regular periods
    for i in 2..n.saturating_sub(1) {
        out.push(coupon(dates[i - 1], dates[i], dates[i - 1], dates[i])?);
    }
    if n > 2 {
        // last period might be short or long
        let (start, end) = (dates[n - 2], dates[n - 1]);
        if schedule.is_regular[n - 2] {
            out.push(coupon(start, end, start, end)?);
        } else {
            let last_ref = cal.advance(start, tenor, conv, end_of_month)?;
            out.push(coupon(start, end, start, last_ref)?);
        }
    }
    Ok(out)
}

/// Interest accrued at `settlement` on the next payment date of the leg
/// (QuantLib `CashFlows::accruedAmount`).
pub fn accrued_amount(leg: &[CashFlow], settlement: Date, include_settlement_date_flows: bool) -> Result<f64, Error> {
    let Some(first) = leg.iter().position(|cf| !cf.has_occurred(settlement, include_settlement_date_flows)) else {
        return Ok(0.0);
    };
    let payment_date = leg[first].date();
    let mut result = 0.0;
    for cf in leg[first..].iter().take_while(|cf| cf.date() == payment_date) {
        if let CashFlow::FixedRateCoupon(c) = cf {
            result += c.accrued_amount(settlement)?;
        }
    }
    Ok(result)
}
