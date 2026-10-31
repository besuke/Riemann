//! Fixed-rate bonds (QuantLib-compatible `FixedRateBond`, single redemption).
//!
//! Construction, notional, settlement date and accrued interest reproduce
//! `ql/instruments/bonds/fixedratebond.cpp`, `ql/instruments/bond.cpp` and
//! `ql/pricingengines/bond/bondfunctions.cpp` (accrued amount), and are treated as derived
//! from QuantLib source code (modified BSD license). QuantLib commit
//! 70f240f29ddb84927ff654502f1d75b3de24ba43. See THIRD_PARTY_NOTICES.
//!
//! Differences from QuantLib: the evaluation date is an argument (no global settings);
//! a single coupon rate and notional; no amortization, ex-coupon period or payment lag.

use crate::cashflows::{self, CashFlow, FixedRateLegSpec};
use crate::time::{
    generate_schedule, BusinessDayConvention, Date, DayCount, HolidayCalendar, Period, Schedule,
    ScheduleSpec, TimeUnit,
};
use crate::Error;

/// Everything needed to build a fixed-rate bond.
#[derive(Debug, Clone, PartialEq)]
pub struct FixedRateBondSpec {
    pub settlement_days: u32,
    pub face_amount: f64,
    pub schedule: ScheduleSpec,
    pub coupon_rate: f64,
    pub day_count: DayCount,
    pub payment_convention: BusinessDayConvention,
    /// Redemption per 100 of notional.
    pub redemption: f64,
    pub issue_date: Option<Date>,
    /// Defaults to the schedule's calendar.
    pub payment_calendar: Option<HolidayCalendar>,
}

/// A built fixed-rate bond: its inputs, schedule and cash flows (coupons, then redemption).
#[derive(Debug, Clone, PartialEq)]
pub struct FixedRateBond {
    pub spec: FixedRateBondSpec,
    pub schedule: Schedule,
    pub cashflows: Vec<CashFlow>,
    /// Payment date of the last coupon; the notional is repaid on this date.
    pub maturity_payment_date: Date,
}

impl FixedRateBond {
    /// Builds the bond (QuantLib `FixedRateBond` constructor).
    pub fn new(spec: FixedRateBondSpec) -> Result<FixedRateBond, Error> {
        let schedule = generate_schedule(&spec.schedule)?;
        let payment_calendar = spec.payment_calendar.clone().unwrap_or_else(|| spec.schedule.calendar.clone());
        let leg = FixedRateLegSpec {
            notional: spec.face_amount,
            rate: spec.coupon_rate,
            day_count: spec.day_count.clone(),
            payment_calendar,
            payment_convention: spec.payment_convention,
        };
        let mut cashflows = cashflows::fixed_rate_leg(&spec.schedule, &schedule, &leg)?;
        // the notional is repaid on the payment date of the last coupon
        let maturity_payment_date = cashflows.last().expect("at least one coupon").date();
        cashflows.push(CashFlow::Redemption {
            date: maturity_payment_date,
            amount: (spec.redemption / 100.0) * (spec.face_amount - 0.0),
        });
        // stable sort by date: redemptions follow coupons paid on the same date
        cashflows.sort_by_key(|cf| cf.date());
        Ok(FixedRateBond { spec, schedule, cashflows, maturity_payment_date })
    }

    fn payment_calendar(&self) -> &HolidayCalendar {
        self.spec.payment_calendar.as_ref().unwrap_or(&self.spec.schedule.calendar)
    }

    /// Settlement date for trades on `evaluation_date`: T + settlement days, not before issue.
    pub fn settlement_date(&self, evaluation_date: Date) -> Result<Date, Error> {
        let settlement = self.payment_calendar().advance(
            evaluation_date,
            Period::new(self.spec.settlement_days as i32, TimeUnit::Days),
            BusinessDayConvention::Following,
            false,
        )?;
        Ok(match self.spec.issue_date {
            Some(issue) => settlement.max(issue),
            None => settlement,
        })
    }

    /// Outstanding notional at `d` (QuantLib `Bond::notional`); zero from the redemption date on.
    pub fn notional(&self, d: Date) -> f64 {
        if d > self.maturity_payment_date {
            return 0.0;
        }
        if d < self.maturity_payment_date {
            self.spec.face_amount
        } else {
            // on the redemption date the payment has occurred and the notional changed
            0.0
        }
    }

    /// Accrued interest per 100 of notional at `settlement` (QuantLib `BondFunctions::accruedAmount`).
    pub fn accrued_amount(&self, settlement: Date) -> Result<f64, Error> {
        let notional = self.notional(settlement);
        if notional == 0.0 {
            return Ok(0.0);
        }
        Ok(cashflows::accrued_amount(&self.cashflows, settlement, false)? * 100.0 / notional)
    }
}
