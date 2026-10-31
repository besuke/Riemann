//! Schedule generation (QuantLib-compatible; see `docs/spec_dates_calendars.md`, chapter 5).
//!
//! The generation algorithm reproduces `ql/time/schedule.cpp` and is treated as derived
//! from QuantLib source code (modified BSD license): Copyright (C) 2006, 2007, 2008, 2010,
//! 2011, 2015 Ferdinando Ametrano; (C) 2000, 2001, 2002, 2003 RiskMap srl; (C) 2009, 2012
//! StatPro Italia srl. QuantLib commit 70f240f29ddb84927ff654502f1d75b3de24ba43 (since v1.42
//! only `ext::optional` was replaced by `std::optional`). See THIRD_PARTY_NOTICES.
//!
//! Differences from QuantLib:
//! - The effective date is required. QuantLib can derive a placeholder from the global
//!   evaluation date; Riemann has no global state.
//! - Only the Backward, Forward and Zero rules are implemented; the other rules return
//!   `Error::Unsupported`.

use super::{BusinessDayConvention, Calendar, Date, HolidayCalendar, Period, TimeUnit};
use crate::Error;

/// How the dates of a schedule are generated (QuantLib `DateGeneration::Rule`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DateGenerationRule {
    /// Backward from the termination date to the effective date.
    Backward,
    /// Forward from the effective date to the termination date.
    Forward,
    /// No intermediate dates.
    Zero,
    ThirdWednesday,
    ThirdWednesdayInclusive,
    Twentieth,
    TwentiethImm,
    OldCds,
    Cds,
    Cds2015,
}

/// Everything needed to generate a schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleSpec {
    pub effective_date: Date,
    pub termination_date: Date,
    pub tenor: Period,
    pub calendar: HolidayCalendar,
    pub convention: BusinessDayConvention,
    pub termination_convention: BusinessDayConvention,
    pub rule: DateGenerationRule,
    pub end_of_month: bool,
    pub first_date: Option<Date>,
    pub next_to_last_date: Option<Date>,
}

/// Generated schedule: adjusted dates, and for each period whether it is regular.
/// `is_regular[i]` refers to the period from `dates[i]` to `dates[i + 1]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    pub dates: Vec<Date>,
    pub is_regular: Vec<bool>,
}

fn invalid(reason: String) -> Error {
    Error::InvalidSchedule { reason }
}

fn times(n: i32, p: Period) -> Period {
    Period::new(n * p.length, p.unit)
}

/// QuantLib `allowsEndOfMonth`: months or years, at least one month.
fn allows_end_of_month(tenor: Period) -> bool {
    match tenor.unit {
        TimeUnit::Months => tenor.length >= 1,
        TimeUnit::Years => tenor.length >= 1,
        _ => false,
    }
}

/// Generates the schedule described by `spec` (QuantLib `Schedule` constructor).
pub fn generate(spec: &ScheduleSpec) -> Result<Schedule, Error> {
    use BusinessDayConvention::Unadjusted;
    use DateGenerationRule::*;

    let effective = spec.effective_date;
    let termination = spec.termination_date;
    let convention = spec.convention;
    let cal = &spec.calendar;
    let null_cal = HolidayCalendar::from(Calendar::NullCalendar);
    let tenor = spec.tenor;
    let end_of_month = allows_end_of_month(tenor) && spec.end_of_month;
    let first = spec.first_date.filter(|&d| d != effective);
    let next_to_last = spec.next_to_last_date.filter(|&d| d != termination);

    if effective >= termination {
        return Err(invalid(format!(
            "effective date ({effective}) later than or equal to termination date ({termination})"
        )));
    }
    let mut rule = spec.rule;
    if tenor.length == 0 {
        rule = Zero;
    } else if tenor.length < 0 {
        return Err(invalid(format!("non positive tenor ({tenor:?}) not allowed")));
    }
    if !matches!(rule, Backward | Forward | Zero) {
        return Err(Error::Unsupported { feature: format!("date generation rule {rule:?}") });
    }
    if let Some(f) = first {
        if rule == Zero {
            return Err(invalid("first date incompatible with Zero date generation rule".into()));
        }
        if !(f > effective && f <= termination) {
            return Err(invalid(format!(
                "first date ({f}) out of effective-termination date range ({effective}, {termination}]"
            )));
        }
    }
    if let Some(n) = next_to_last {
        if rule == Zero {
            return Err(invalid("next to last date incompatible with Zero date generation rule".into()));
        }
        if !(n >= effective && n < termination) {
            return Err(invalid(format!(
                "next to last date ({n}) out of effective-termination date range [{effective}, {termination})"
            )));
        }
    }

    let null_advance = |d: Date, p: Period| null_cal.advance(d, p, convention, end_of_month);
    let mut dates: Vec<Date> = Vec::new();
    let mut is_regular: Vec<bool> = Vec::new();
    let mut periods = 1;
    // QuantLib leaves the seed as a null date for the Zero rule.
    let mut seed: Option<Date> = None;

    match rule {
        Zero => {
            dates.push(effective);
            dates.push(termination);
            is_regular.push(true);
        }
        Backward => {
            dates.push(termination);
            let mut s = termination;
            if let Some(n) = next_to_last {
                dates.push(n);
                let temp = null_advance(s, times(-periods, tenor))?;
                is_regular.push(temp == n);
                s = n;
            }
            seed = Some(s);
            let exit = first.unwrap_or(effective);
            loop {
                let temp = null_advance(s, times(-periods, tenor))?;
                if temp < exit {
                    if let Some(f) = first {
                        let last = *dates.last().expect("non-empty");
                        if cal.adjust(last, convention)? != cal.adjust(f, convention)? {
                            dates.push(f);
                            let prev = dates[dates.len() - 2];
                            is_regular.push(null_advance(prev, times(-1, tenor))? == f);
                        }
                    }
                    break;
                }
                let last = *dates.last().expect("non-empty");
                if cal.adjust(last, convention)? != cal.adjust(temp, convention)? {
                    dates.push(temp);
                    is_regular.push(true);
                }
                periods += 1;
            }
            let last = *dates.last().expect("non-empty");
            if cal.adjust(last, convention)? != cal.adjust(effective, convention)? {
                dates.push(effective);
                let prev = dates[dates.len() - 2];
                is_regular.push(null_advance(prev, times(-1, tenor))? == effective);
            }
            dates.reverse();
            is_regular.reverse();
        }
        Forward => {
            dates.push(effective);
            let mut s = effective;
            if let Some(f) = first {
                dates.push(f);
                let temp = null_advance(s, times(periods, tenor))?;
                is_regular.push(temp == f);
                s = f;
            }
            seed = Some(s);
            let exit = next_to_last.unwrap_or(termination);
            loop {
                let temp = null_advance(s, times(periods, tenor))?;
                if temp > exit {
                    if let Some(n) = next_to_last {
                        let last = *dates.last().expect("non-empty");
                        if cal.adjust(last, convention)? != cal.adjust(n, convention)? {
                            dates.push(n);
                            let prev = dates[dates.len() - 2];
                            is_regular.push(null_advance(prev, times(1, tenor))? == n);
                        }
                    }
                    break;
                }
                let last = *dates.last().expect("non-empty");
                if cal.adjust(last, convention)? != cal.adjust(temp, convention)? {
                    dates.push(temp);
                    is_regular.push(true);
                }
                periods += 1;
            }
            let last = *dates.last().expect("non-empty");
            let tc = spec.termination_convention;
            if cal.adjust(last, tc)? != cal.adjust(termination, tc)? {
                dates.push(termination);
                is_regular.push(false);
            }
        }
        _ => unreachable!("checked above"),
    }
    // adjustments
    if convention != Unadjusted {
        dates[0] = cal.adjust(dates[0], convention)?;
    }
    let n = dates.len();
    if spec.termination_convention != Unadjusted {
        dates[n - 1] = cal.adjust(dates[n - 1], spec.termination_convention)?;
    }
    let eom_seed = if end_of_month {
        match seed {
            Some(s) => cal.is_end_of_month(s)?,
            None => return Err(invalid("null seed date with end-of-month adjustment".into())),
        }
    } else {
        false
    };
    for date in dates.iter_mut().take(n - 1).skip(1) {
        *date = if eom_seed { cal.adjust(date.end_of_month(), convention)? } else { cal.adjust(*date, convention)? };
    }

    // final safety checks (QuantLib): remove an extra next-to-last or second date
    if dates.len() >= 2 && dates[dates.len() - 2] >= *dates.last().expect("non-empty") {
        let last = *dates.last().expect("non-empty");
        let len = is_regular.len();
        if len >= 2 {
            is_regular[len - 2] = dates[dates.len() - 2] == last;
        }
        let k = dates.len() - 2;
        dates[k] = last;
        dates.pop();
        is_regular.pop();
    }
    if dates.len() >= 2 && dates[1] <= dates[0] {
        if is_regular.len() >= 2 {
            is_regular[1] = dates[1] == dates[0];
        }
        dates[1] = dates[0];
        dates.remove(0);
        is_regular.remove(0);
    }
    if dates.len() <= 1 {
        return Err(invalid(format!("degenerate single date ({}) schedule", dates[0])));
    }
    Ok(Schedule { dates, is_regular })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semiannual_backward_with_end_of_month() {
        let spec = ScheduleSpec {
            effective_date: Date::from_ymd(2024, 1, 31).unwrap(),
            termination_date: Date::from_ymd(2026, 1, 31).unwrap(),
            tenor: Period::new(6, TimeUnit::Months),
            calendar: HolidayCalendar::from(Calendar::Japan),
            convention: BusinessDayConvention::ModifiedFollowing,
            termination_convention: BusinessDayConvention::ModifiedFollowing,
            rule: DateGenerationRule::Backward,
            end_of_month: true,
            first_date: None,
            next_to_last_date: None,
        };
        let s = generate(&spec).unwrap();
        let iso: Vec<String> = s.dates.iter().map(|d| d.to_string()).collect();
        // Same as QuantLib (checked with the R binding 1.42).
        assert_eq!(iso, ["2024-01-31", "2024-07-31", "2025-01-31", "2025-07-31", "2026-01-30"]);
        assert_eq!(s.is_regular, [true, true, true, true]);
    }
}
