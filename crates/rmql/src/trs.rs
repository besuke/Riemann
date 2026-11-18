//! Total return swaps: the dates and amounts of the funding leg and of the return leg.
//!
//! Written for rmql; no source was taken from QuantLib. The definitions follow the usual
//! terms of a TRS confirmation under the 2021 ISDA Interest Rate Derivatives Definitions, as stated
//! below, and every choice that a confirmation states is an argument:
//!
//! - **Calculation periods**: a regular schedule from the effective to the termination date (`frequency`
//!   payments a year), adjusted with the period convention on the period calendar.
//! - **Payment dates**: `payment_lag` business days of the payment calendar after each period end, then
//!   adjusted with the payment convention. A leg may have no payment terms (`None`): its payment dates are
//!   then not generated, because they are not defined.
//! - **Funding leg**: an overnight index compounded over the period with a lookback of `lookback`
//!   business days and no observation shift. Each business day `d_i` of the period (index calendar)
//!   carries the fixing of the business day `lookback` days before it, weighted by the index's year
//!   fraction from `d_i` to `d_{i+1}` (the calendar days of the calculation period):
//!   `rate = (prod_i (1 + r_i tau_i) - 1) / tau(start, end)`, and the amount is
//!   `notional * (rate + spread) * accrual`, where the accrual uses the leg's day count. Reset dates are
//!   the period start or end; fixing dates are `fixing_offset` business days from the reset date,
//!   adjusted with the fixing convention. Both are reported for the confirmation; the compounded rate
//!   itself uses the daily observation dates.
//! - **Return leg**: the valuation dates are `valuation_offset` business days from each payment date (or
//!   each period end), adjusted with the valuation convention. A stated final valuation date replaces the
//!   last one, and the date given by the rule is kept beside it. The amount of a period is
//!   `notional * (level_end / level_start - 1)`, the first start level being the initial price. For a
//!   total return index the income is in the level.
//!
//! Each amount says how it is known: from published values only (`Fixed`), with a forecast or a
//! scenario (`Projected`, `Scenario`), or not at all (`Missing`, with the reason and no amount).

use crate::curves::YieldCurve;
use crate::ois::OvernightIndex;
use crate::ois_swap::Fixings;
use crate::time::{
    generate_schedule, BusinessDayConvention, Date, DateGenerationRule, DayCount, HolidayCalendar, Period, ScheduleSpec,
    TimeUnit,
};
use crate::Error;
use std::collections::BTreeMap;

fn invalid<T>(reason: String) -> Result<T, Error> {
    Err(Error::InvalidInput { reason })
}

/// When payments are made, if the confirmation defines it.
#[derive(Debug, Clone, PartialEq)]
pub struct PaymentTerms {
    pub calendar: HolidayCalendar,
    pub convention: BusinessDayConvention,
    /// Business days after the period end.
    pub lag: u32,
}

/// The calculation periods of a leg.
#[derive(Debug, Clone, PartialEq)]
pub struct TrsPeriodSpec {
    pub effective_date: Date,
    pub termination_date: Date,
    /// Periods a year: 1, 2, 4 or 12.
    pub frequency: u32,
    pub calendar: HolidayCalendar,
    pub period_convention: BusinessDayConvention,
    pub rule: DateGenerationRule,
    pub end_of_month: bool,
    pub payment: Option<PaymentTerms>,
}

/// One calculation period of either leg.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrsPeriod {
    pub unadjusted_start: Date,
    pub unadjusted_end: Date,
    pub start: Date,
    pub end: Date,
    pub payment_date: Option<Date>,
}

fn period_of_frequency(frequency: u32) -> Result<Period, Error> {
    match frequency {
        1 => Ok(Period::new(1, TimeUnit::Years)),
        2 => Ok(Period::new(6, TimeUnit::Months)),
        4 => Ok(Period::new(3, TimeUnit::Months)),
        12 => Ok(Period::new(1, TimeUnit::Months)),
        other => invalid(format!("unsupported frequency ({other} a year)")),
    }
}

/// The periods of a leg, with their payment dates when the payment terms are given.
pub fn generate_trs_periods(spec: &TrsPeriodSpec) -> Result<Vec<TrsPeriod>, Error> {
    if spec.termination_date <= spec.effective_date {
        return invalid("the termination date must be after the effective date".into());
    }
    let schedule = |convention: BusinessDayConvention| -> Result<Vec<Date>, Error> {
        Ok(generate_schedule(&ScheduleSpec {
            effective_date: spec.effective_date,
            termination_date: spec.termination_date,
            tenor: period_of_frequency(spec.frequency)?,
            calendar: spec.calendar.clone(),
            convention,
            termination_convention: convention,
            rule: spec.rule,
            end_of_month: spec.end_of_month,
            first_date: None,
            next_to_last_date: None,
        })?
        .dates)
    };
    let unadjusted = schedule(BusinessDayConvention::Unadjusted)?;
    let adjusted = schedule(spec.period_convention)?;
    if unadjusted.len() != adjusted.len() {
        return invalid("the adjusted and unadjusted schedules differ in length".into());
    }
    (0..adjusted.len() - 1)
        .map(|i| {
            let payment_date = match &spec.payment {
                Some(p) => Some(p.calendar.advance(adjusted[i + 1], Period::new(p.lag as i32, TimeUnit::Days), p.convention, false)?),
                None => None,
            };
            Ok(TrsPeriod {
                unadjusted_start: unadjusted[i],
                unadjusted_end: unadjusted[i + 1],
                start: adjusted[i],
                end: adjusted[i + 1],
                payment_date,
            })
        })
        .collect()
}

/// Business days from a date on a calendar (negative: before), adjusted with a convention.
fn offset_date(calendar: &HolidayCalendar, d: Date, days: i32, convention: BusinessDayConvention) -> Result<Date, Error> {
    let moved = calendar.advance(d, Period::new(days, TimeUnit::Days), BusinessDayConvention::Unadjusted, false)?;
    calendar.adjust(moved, convention)
}

// ---- funding leg ------------------------------------------------------------------------------

/// The date a reset is relative to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetRelativeTo {
    PeriodStart,
    PeriodEnd,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrsFundingLegSpec {
    pub periods: TrsPeriodSpec,
    pub index: OvernightIndex,
    pub day_count: DayCount,
    pub notional: f64,
    /// Added to the compounded rate, as a decimal.
    pub spread: f64,
    /// Business days of the index calendar between a day of the period and its fixing.
    pub lookback: u32,
    pub reset_relative_to: ResetRelativeTo,
    /// Business days from the reset date to the fixing date (negative: before).
    pub fixing_offset: i32,
    pub fixing_calendar: HolidayCalendar,
    pub fixing_convention: BusinessDayConvention,
}

/// One period of the funding leg and its observation days.
#[derive(Debug, Clone, PartialEq)]
pub struct TrsFundingPeriod {
    pub period: TrsPeriod,
    pub reset_date: Date,
    pub fixing_date: Date,
    /// The fixing date of the first and the last day of the period (`lookback` business days before).
    pub observation_start: Date,
    pub observation_end: Date,
    /// The leg's year fraction of the period.
    pub accrual: f64,
    /// The business days of the period (index calendar), each with its fixing date and weight.
    pub days: Vec<Date>,
    pub observation_dates: Vec<Date>,
    pub day_fractions: Vec<f64>,
    /// The index's year fraction of the period.
    pub index_accrual: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrsFundingLeg {
    pub spec: TrsFundingLegSpec,
    pub periods: Vec<TrsFundingPeriod>,
}

/// Builds the funding leg: its periods, reset, fixing, observation and payment dates.
pub fn create_trs_funding_leg(spec: TrsFundingLegSpec) -> Result<TrsFundingLeg, Error> {
    if !spec.notional.is_finite() || !spec.spread.is_finite() {
        return invalid("the notional and the spread must be finite".into());
    }
    let index_calendar = spec.index.calendar.clone();
    let lookback = -(spec.lookback as i32);
    let periods = generate_trs_periods(&spec.periods)?
        .into_iter()
        .map(|p| {
            let reset_date = match spec.reset_relative_to {
                ResetRelativeTo::PeriodStart => p.start,
                ResetRelativeTo::PeriodEnd => p.end,
            };
            let fixing_date = offset_date(&spec.fixing_calendar, reset_date, spec.fixing_offset, spec.fixing_convention)?;
            // business days of the index from the start (adjusted to a business day) to the end
            let mut days = Vec::new();
            let mut d = index_calendar.adjust(p.start, BusinessDayConvention::Following)?;
            while d < p.end {
                days.push(d);
                d = index_calendar.advance(d, Period::new(1, TimeUnit::Days), BusinessDayConvention::Following, false)?;
            }
            if days.is_empty() {
                return invalid(format!("no business day in the period from {} to {}", p.start, p.end));
            }
            let mut bounds = days.clone();
            bounds.push(p.end);
            let day_fractions = bounds
                .windows(2)
                .map(|w| spec.index.day_count.year_fraction(w[0], w[1], None, None))
                .collect::<Result<Vec<_>, Error>>()?;
            let observation_dates = days
                .iter()
                .map(|&d| index_calendar.advance(d, Period::new(lookback, TimeUnit::Days), BusinessDayConvention::Following, false))
                .collect::<Result<Vec<_>, Error>>()?;
            let observation_end =
                index_calendar.advance(p.end, Period::new(lookback, TimeUnit::Days), BusinessDayConvention::Following, false)?;
            Ok(TrsFundingPeriod {
                period: p,
                reset_date,
                fixing_date,
                observation_start: observation_dates[0],
                observation_end,
                accrual: spec.day_count.year_fraction(p.start, p.end, Some(p.start), Some(p.end))?,
                index_accrual: spec.index.day_count.year_fraction(p.start, p.end, None, None)?,
                days,
                observation_dates,
                day_fractions,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(TrsFundingLeg { spec, periods })
}

/// How an amount is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmountStatus {
    /// Published values only.
    Fixed,
    /// Published values and a forecast from a curve.
    Projected,
    /// Uses scenario values.
    Scenario,
    /// Cannot be computed; there is no amount.
    Missing,
}

impl AmountStatus {
    pub fn name(self) -> &'static str {
        match self {
            AmountStatus::Fixed => "fixed",
            AmountStatus::Projected => "projected",
            AmountStatus::Scenario => "scenario",
            AmountStatus::Missing => "missing",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrsFundingAmount {
    pub status: AmountStatus,
    /// The compounded overnight rate, without the spread.
    pub compounded_rate: Option<f64>,
    pub amount: Option<f64>,
    /// Observation days with a published fixing, with a forecast, and all of them.
    pub fixed_days: usize,
    pub projected_days: usize,
    pub days: usize,
    /// Why the amount is missing, or empty.
    pub reason: String,
}

/// The overnight forward rate for a fixing date: the curve's discount factors from the date to the
/// next business day of the index, with the index's year fraction.
fn forward_overnight_rate(index: &OvernightIndex, curve: &YieldCurve, d: Date) -> Result<f64, Error> {
    let next = index.calendar.advance(d, Period::new(1, TimeUnit::Days), BusinessDayConvention::Following, false)?;
    let tau = index.day_count.year_fraction(d, next, None, None)?;
    Ok((curve.discount(d)? / curve.discount(next)? - 1.0) / tau)
}

/// The amounts of the funding leg at an evaluation date. A fixing date before the evaluation date
/// needs its published fixing; on the evaluation date the fixing is used when given; later fixing
/// dates use the forecast curve when one is given. Without the value a period has no amount.
pub fn compute_trs_funding_amounts(
    leg: &TrsFundingLeg,
    evaluation_date: Date,
    fixings: &Fixings,
    forecast: Option<&YieldCurve>,
) -> Vec<TrsFundingAmount> {
    leg.periods
        .iter()
        .map(|p| {
            let n = p.days.len();
            let mut compound = 1.0;
            let (mut fixed_days, mut projected_days) = (0, 0);
            let mut missing: Vec<String> = Vec::new();
            for (o, tau) in p.observation_dates.iter().zip(&p.day_fractions) {
                let published = fixings.get(o).copied();
                let rate = if *o < evaluation_date || (*o == evaluation_date && published.is_some()) {
                    match published {
                        Some(r) => {
                            fixed_days += 1;
                            Some(r)
                        }
                        None => {
                            missing.push(format!("no published fixing for {o}"));
                            None
                        }
                    }
                } else {
                    match forecast {
                        Some(curve) => match forward_overnight_rate(&leg.spec.index, curve, *o) {
                            Ok(r) => {
                                projected_days += 1;
                                Some(r)
                            }
                            Err(e) => {
                                missing.push(format!("no forecast for {o}: {e}"));
                                None
                            }
                        },
                        None => {
                            missing.push(format!("no forecast curve for {o}"));
                            None
                        }
                    }
                };
                if let Some(r) = rate {
                    compound *= 1.0 + r * tau;
                }
            }
            if !missing.is_empty() {
                let reason = if missing.len() > 3 {
                    format!("{}; and {} more", missing[..3].join("; "), missing.len() - 3)
                } else {
                    missing.join("; ")
                };
                return TrsFundingAmount {
                    status: AmountStatus::Missing,
                    compounded_rate: None,
                    amount: None,
                    fixed_days,
                    projected_days,
                    days: n,
                    reason,
                };
            }
            let rate = (compound - 1.0) / p.index_accrual;
            TrsFundingAmount {
                status: if projected_days > 0 { AmountStatus::Projected } else { AmountStatus::Fixed },
                compounded_rate: Some(rate),
                amount: Some(leg.spec.notional * (rate + leg.spec.spread) * p.accrual),
                fixed_days,
                projected_days,
                days: n,
                reason: String::new(),
            }
        })
        .collect()
}

// ---- return leg -------------------------------------------------------------------------------

/// The date valuation dates are relative to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValuationRelativeTo {
    PaymentDate,
    PeriodEnd,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrsReturnLegSpec {
    pub periods: TrsPeriodSpec,
    pub notional: f64,
    /// The level at the start of the first period.
    pub initial_price: f64,
    pub valuation_relative_to: ValuationRelativeTo,
    /// Business days from the payment date (or period end) to the valuation date (negative: before).
    pub valuation_offset: i32,
    pub valuation_calendar: HolidayCalendar,
    pub valuation_convention: BusinessDayConvention,
    /// Replaces the last valuation date when the confirmation states it.
    pub final_valuation_date: Option<Date>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrsReturnPeriod {
    pub period: TrsPeriod,
    /// The valuation date of the previous period; `None` for the first, which starts at the initial price.
    pub start_valuation_date: Option<Date>,
    pub valuation_date: Date,
    /// The valuation date given by the rule (differs from `valuation_date` only when a stated final
    /// valuation date replaced it).
    pub valuation_date_by_rule: Date,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrsReturnLeg {
    pub spec: TrsReturnLegSpec,
    pub periods: Vec<TrsReturnPeriod>,
}

pub fn create_trs_return_leg(spec: TrsReturnLegSpec) -> Result<TrsReturnLeg, Error> {
    if !spec.notional.is_finite() || !spec.initial_price.is_finite() || spec.initial_price <= 0.0 {
        return invalid("the notional must be finite and the initial price positive".into());
    }
    let raw = generate_trs_periods(&spec.periods)?;
    let mut periods: Vec<TrsReturnPeriod> = Vec::with_capacity(raw.len());
    for (i, p) in raw.iter().enumerate() {
        let anchor = match spec.valuation_relative_to {
            ValuationRelativeTo::PeriodEnd => p.end,
            ValuationRelativeTo::PaymentDate => match p.payment_date {
                Some(d) => d,
                None => return invalid("valuation dates relative to payment dates need the payment terms".into()),
            },
        };
        let by_rule = offset_date(&spec.valuation_calendar, anchor, spec.valuation_offset, spec.valuation_convention)?;
        let valuation_date = match spec.final_valuation_date {
            Some(d) if i + 1 == raw.len() => d,
            _ => by_rule,
        };
        let start_valuation_date = periods.last().map(|q| q.valuation_date);
        if let Some(s) = start_valuation_date {
            if valuation_date <= s {
                return invalid(format!("valuation dates must increase ({s}, then {valuation_date})"));
            }
        }
        periods.push(TrsReturnPeriod { period: *p, start_valuation_date, valuation_date, valuation_date_by_rule: by_rule });
    }
    Ok(TrsReturnLeg { spec, periods })
}

/// Where a level of the reference asset comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelKind {
    Observed,
    Scenario,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrsReturnAmount {
    pub status: AmountStatus,
    pub start_level: Option<f64>,
    pub end_level: Option<f64>,
    /// `level_end / level_start - 1`.
    pub period_return: Option<f64>,
    pub amount: Option<f64>,
    pub reason: String,
}

/// The amounts of the return leg from the levels at the valuation dates. The first period starts at
/// the initial price. A period without one of its levels has no amount.
pub fn compute_trs_return_amounts(leg: &TrsReturnLeg, levels: &BTreeMap<Date, (f64, LevelKind)>) -> Vec<TrsReturnAmount> {
    leg.periods
        .iter()
        .map(|p| {
            let start = match p.start_valuation_date {
                None => Some((leg.spec.initial_price, LevelKind::Observed)),
                Some(d) => levels.get(&d).copied(),
            };
            let end = levels.get(&p.valuation_date).copied();
            let mut reasons = Vec::new();
            if start.is_none() {
                reasons.push(format!("no level for {}", p.start_valuation_date.map(|d| d.to_string()).unwrap_or_default()));
            }
            if end.is_none() {
                reasons.push(format!("no level for {}", p.valuation_date));
            }
            match (start, end) {
                (Some((s, ks)), Some((e, ke))) if s > 0.0 => {
                    let r = e / s - 1.0;
                    let scenario = ks == LevelKind::Scenario || ke == LevelKind::Scenario;
                    TrsReturnAmount {
                        status: if scenario { AmountStatus::Scenario } else { AmountStatus::Fixed },
                        start_level: Some(s),
                        end_level: Some(e),
                        period_return: Some(r),
                        amount: Some(leg.spec.notional * r),
                        reason: String::new(),
                    }
                }
                _ => TrsReturnAmount {
                    status: AmountStatus::Missing,
                    start_level: start.map(|x| x.0),
                    end_level: end.map(|x| x.0),
                    period_return: None,
                    amount: None,
                    reason: if reasons.is_empty() { "the start level is not positive".into() } else { reasons.join("; ") },
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ois::create_tonar_index;
    use crate::time::{Actual365Kind, Calendar};
    use BusinessDayConvention::*;

    fn d(y: i32, m: u32, day: u32) -> Date {
        Date::from_ymd(y, m, day).unwrap()
    }
    fn tokyo() -> HolidayCalendar {
        HolidayCalendar::from(Calendar::Japan)
    }
    fn periods(payment: Option<PaymentTerms>) -> TrsPeriodSpec {
        TrsPeriodSpec {
            effective_date: d(2026, 10, 8),
            termination_date: d(2027, 10, 8),
            frequency: 4,
            calendar: tokyo(),
            period_convention: ModifiedFollowing,
            rule: DateGenerationRule::Backward,
            end_of_month: false,
            payment,
        }
    }
    fn funding() -> TrsFundingLeg {
        create_trs_funding_leg(TrsFundingLegSpec {
            periods: periods(Some(PaymentTerms { calendar: tokyo(), convention: ModifiedFollowing, lag: 2 })),
            index: create_tonar_index(),
            day_count: DayCount::Actual365Fixed(Actual365Kind::Standard),
            notional: 1e9,
            spread: 0.0035,
            lookback: 5,
            reset_relative_to: ResetRelativeTo::PeriodStart,
            fixing_offset: -2,
            fixing_calendar: tokyo(),
            fixing_convention: Preceding,
        })
        .unwrap()
    }

    // Japanese holidays used: 2027-01-11 (Coming of Age Day), 2027-10-11 (Sports Day), 2026-10-12 (Sports Day).
    #[test]
    fn funding_dates_follow_the_calendar() {
        let leg = funding();
        let ends: Vec<Date> = leg.periods.iter().map(|p| p.period.end).collect();
        assert_eq!(ends, vec![d(2027, 1, 8), d(2027, 4, 8), d(2027, 7, 8), d(2027, 10, 8)]);
        let pay: Vec<Date> = leg.periods.iter().map(|p| p.period.payment_date.unwrap()).collect();
        assert_eq!(pay, vec![d(2027, 1, 13), d(2027, 4, 12), d(2027, 7, 12), d(2027, 10, 13)]);
        let fix: Vec<Date> = leg.periods.iter().map(|p| p.fixing_date).collect();
        assert_eq!(fix, vec![d(2026, 10, 6), d(2027, 1, 6), d(2027, 4, 6), d(2027, 7, 6)]);
        // the first day of the first period looks back 5 Tokyo business days: 10/7, 10/6, 10/5, 10/2, 10/1
        assert_eq!(leg.periods[0].observation_start, d(2026, 10, 1));
        // 10/9 (Friday) is followed by the weekend and Sports Day: weight 4 days
        let i = leg.periods[0].days.iter().position(|&x| x == d(2026, 10, 9)).unwrap();
        assert!((leg.periods[0].day_fractions[i] - 4.0 / 365.0).abs() < 1e-15);
        let total: f64 = leg.periods[0].day_fractions.iter().sum();
        assert!((total - leg.periods[0].index_accrual).abs() < 1e-14);
    }

    #[test]
    fn funding_amount_with_flat_fixings_matches_the_closed_form() {
        let leg = funding();
        let p = &leg.periods[1];
        let fixings: Fixings = p.observation_dates.iter().map(|&o| (o, 0.01)).collect();
        let a = &compute_trs_funding_amounts(&leg, d(2027, 12, 1), &fixings, None)[1];
        assert_eq!(a.status, AmountStatus::Fixed);
        let expected_rate = (p.day_fractions.iter().map(|t| 1.0 + 0.01 * t).product::<f64>() - 1.0) / p.index_accrual;
        assert!((a.compounded_rate.unwrap() - expected_rate).abs() < 1e-15);
        assert!((a.amount.unwrap() - 1e9 * (expected_rate + 0.0035) * p.accrual).abs() < 1e-6);
        // without fixings or a curve, the amount is missing and says why
        let none = &compute_trs_funding_amounts(&leg, d(2027, 12, 1), &Fixings::new(), None)[1];
        assert_eq!(none.status, AmountStatus::Missing);
        assert!(none.amount.is_none() && none.reason.contains("no published fixing"));
    }

    #[test]
    fn return_valuation_dates_and_final_date() {
        let by_end = create_trs_return_leg(TrsReturnLegSpec {
            periods: periods(None),
            notional: 1e9,
            initial_price: 100.0,
            valuation_relative_to: ValuationRelativeTo::PeriodEnd,
            valuation_offset: -2,
            valuation_calendar: tokyo(),
            valuation_convention: Preceding,
            final_valuation_date: Some(d(2027, 10, 6)),
        })
        .unwrap();
        let v: Vec<Date> = by_end.periods.iter().map(|p| p.valuation_date).collect();
        assert_eq!(v, vec![d(2027, 1, 6), d(2027, 4, 6), d(2027, 7, 6), d(2027, 10, 6)]);
        assert_eq!(by_end.periods[3].valuation_date_by_rule, d(2027, 10, 6));
        assert!(by_end.periods.iter().all(|p| p.period.payment_date.is_none()));
        let mut levels = BTreeMap::new();
        levels.insert(d(2027, 1, 6), (101.0, LevelKind::Observed));
        levels.insert(d(2027, 4, 6), (100.5, LevelKind::Scenario));
        let a = compute_trs_return_amounts(&by_end, &levels);
        assert_eq!(a[0].status, AmountStatus::Fixed);
        assert!((a[0].amount.unwrap() - 1e9 * 0.01).abs() < 1e-6);
        assert_eq!(a[1].status, AmountStatus::Scenario);
        assert_eq!(a[2].status, AmountStatus::Missing);
        assert!(a[2].amount.is_none());
        // relative to lagged payment dates the last valuation date by the rule is 2027-10-08
        let by_payment = create_trs_return_leg(TrsReturnLegSpec {
            periods: periods(Some(PaymentTerms { calendar: tokyo(), convention: ModifiedFollowing, lag: 2 })),
            notional: 1e9,
            initial_price: 100.0,
            valuation_relative_to: ValuationRelativeTo::PaymentDate,
            valuation_offset: -2,
            valuation_calendar: tokyo(),
            valuation_convention: Preceding,
            final_valuation_date: Some(d(2027, 10, 6)),
        })
        .unwrap();
        assert_eq!(by_payment.periods[3].valuation_date_by_rule, d(2027, 10, 8));
        assert_eq!(by_payment.periods[3].valuation_date, d(2027, 10, 6));
    }
}
