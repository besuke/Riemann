//! Day count conventions: how long a period is, in days and in years
//! (QuantLib-compatible; see `docs/spec_dates_calendars.md`, chapter 6).
//!
//! Not to be confused with business day conventions, which move dates off holidays.
//!
//! The formulas reproduce QuantLib's day counters and are treated as derived from QuantLib
//! source code (modified BSD license), `ql/time/daycounters/`:
//! actual360.hpp (C) 2000-2003 RiskMap srl; actual364.hpp (C) 2014 StatPro Italia srl;
//! actual36525.hpp, actual366.hpp (C) 2022 Ignacio Anguita; actual365fixed.hpp/.cpp
//! (C) 2000-2003 RiskMap srl, (C) 2004 Ferdinando Ametrano, (C) 2013 BGC Partners L.P.;
//! actualactual.cpp (C) 2000-2003 RiskMap srl; thirty360.cpp (C) 2000-2003 RiskMap srl,
//! (C) 2018 Alexey Indiryakov; thirty365.cpp (C) 2015 StatPro Italia srl; one.hpp
//! (C) 2004 Ferdinando Ametrano; simpledaycounter.cpp (C) 2003 RiskMap srl; business252.cpp
//! (C) 2006 Piter Dias, (C) 2011 StatPro Italia srl. QuantLib commit
//! 70f240f29ddb84927ff654502f1d75b3de24ba43. See THIRD_PARTY_NOTICES.
//!
//! Not implemented: Actual/Actual (ISMA/Bond) driven by a schedule; the reference-period
//! variant (QuantLib's behaviour when no schedule is given) is implemented.

use super::{is_leap, Date, HolidayCalendar, Period, TimeUnit};
use crate::Error;

/// Variants of Actual/365 (Fixed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Actual365Kind {
    Standard,
    /// Canadian bond convention; needs a reference period.
    Canadian,
    /// February 29th is not counted.
    NoLeap,
}

/// Variants of Actual/Actual. QuantLib aliases: Bond = ISMA, Historical = Actual365 = ISDA,
/// Euro = AFB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActualActualKind {
    Isda,
    /// ISMA / Bond with a reference period (no schedule).
    Isma,
    Afb,
}

/// Variants of 30/360. QuantLib aliases: EurobondBasis = European, ISMA = BondBasis,
/// German = ISDA.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Thirty360Kind {
    Usa,
    BondBasis,
    European,
    Italian,
    /// 30E/360 ISDA (German); February month end is not moved on the termination date.
    Isda { termination: Option<Date> },
    Nasd,
}

/// A day count convention (QuantLib `DayCounter`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DayCount {
    Actual360 { include_last_day: bool },
    Actual364,
    Actual36525 { include_last_day: bool },
    Actual365Fixed(Actual365Kind),
    Actual366 { include_last_day: bool },
    ActualActual(ActualActualKind),
    Thirty360(Thirty360Kind),
    Thirty365,
    /// 1/1: one per period.
    One,
    /// Whole months as twelfths when the days of month match; 30/360 Bond Basis otherwise.
    Simple,
    /// Business days over 252 for the given calendar.
    Business252(HolidayCalendar),
}

fn is_last_of_february(d: u32, m: u32, y: i32) -> bool {
    m == 2 && d == 28 + is_leap(y) as u32
}

fn thirty_360(d1: Date, d2: Date, kind: Thirty360Kind) -> i64 {
    let (yy1, mm1, dd1) = d1.ymd();
    let (yy2, mm2, dd2) = d2.ymd();
    let (mut dd1, mut dd2, mm1, mut mm2) = (dd1 as i64, dd2 as i64, mm1 as i64, mm2 as i64);
    match kind {
        Thirty360Kind::Usa => {
            // the order of checks matters
            if is_last_of_february(dd1 as u32, mm1 as u32, yy1) {
                if is_last_of_february(dd2 as u32, mm2 as u32, yy2) {
                    dd2 = 30;
                }
                dd1 = 30;
            }
            if dd2 == 31 && dd1 >= 30 {
                dd2 = 30;
            }
            if dd1 == 31 {
                dd1 = 30;
            }
        }
        Thirty360Kind::BondBasis => {
            if dd1 == 31 {
                dd1 = 30;
            }
            if dd2 == 31 && dd1 == 30 {
                dd2 = 30;
            }
        }
        Thirty360Kind::European => {
            if dd1 == 31 {
                dd1 = 30;
            }
            if dd2 == 31 {
                dd2 = 30;
            }
        }
        Thirty360Kind::Italian => {
            if dd1 == 31 {
                dd1 = 30;
            }
            if dd2 == 31 {
                dd2 = 30;
            }
            if mm1 == 2 && dd1 > 27 {
                dd1 = 30;
            }
            if mm2 == 2 && dd2 > 27 {
                dd2 = 30;
            }
        }
        Thirty360Kind::Isda { termination } => {
            if dd1 == 31 {
                dd1 = 30;
            }
            if dd2 == 31 {
                dd2 = 30;
            }
            if is_last_of_february(dd1 as u32, mm1 as u32, yy1) {
                dd1 = 30;
            }
            if Some(d2) != termination && is_last_of_february(dd2 as u32, mm2 as u32, yy2) {
                dd2 = 30;
            }
        }
        Thirty360Kind::Nasd => {
            if dd1 == 31 {
                dd1 = 30;
            }
            if dd2 == 31 && dd1 >= 30 {
                dd2 = 30;
            }
            if dd2 == 31 && dd1 < 30 {
                dd2 = 1;
                mm2 += 1;
            }
        }
    }
    360 * (yy2 - yy1) as i64 + 30 * (mm2 - mm1) + (dd2 - dd1)
}

fn thirty_365(d1: Date, d2: Date) -> i64 {
    let (yy1, mm1, dd1) = d1.ymd();
    let (yy2, mm2, dd2) = d2.ymd();
    let dd1 = if dd1 == 31 { 30 } else { dd1 } as i64;
    let dd2 = if dd2 == 31 { 30 } else { dd2 } as i64;
    360 * (yy2 - yy1) as i64 + 30 * (mm2 as i64 - mm1 as i64) + (dd2 - dd1)
}

fn no_leap_day_count(d1: Date, d2: Date) -> i64 {
    const MONTH_OFFSET: [i64; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let s = |d: Date| {
        let (y, m, day) = d.ymd();
        let mut s = day as i64 + MONTH_OFFSET[(m - 1) as usize] + y as i64 * 365;
        if m == 2 && day == 29 {
            s -= 1;
        }
        s
    };
    s(d2) - s(d1)
}

fn days(d1: Date, d2: Date) -> f64 {
    d1.days_until(d2) as f64
}

fn months(d: Date, n: i32) -> Result<Date, Error> {
    d.add_period(Period::new(n, TimeUnit::Months))
}

/// Actual/Actual ISMA with a reference period (QuantLib `Old_ISMA_Impl`).
fn act_act_isma(d1: Date, d2: Date, ref_start: Option<Date>, ref_end: Option<Date>) -> Result<f64, Error> {
    if d1 == d2 {
        return Ok(0.0);
    }
    if d1 > d2 {
        return Ok(-act_act_isma(d2, d1, ref_start, ref_end)?);
    }
    let mut ref_period_start = ref_start.unwrap_or(d1);
    let mut ref_period_end = ref_end.unwrap_or(d2);
    if !(ref_period_end > ref_period_start && ref_period_end > d1) {
        return Err(Error::InvalidReferencePeriod {
            reason: format!(
                "date 1: {d1}, date 2: {d2}, reference period start: {ref_period_start}, reference period end: {ref_period_end}"
            ),
        });
    }
    // estimate roughly the length in months of a period
    let mut m = (12.0 * days(ref_period_start, ref_period_end) / 365.0).round() as i32;
    // for short periods, take the reference period as 1 year from d1
    if m == 0 {
        ref_period_start = d1;
        ref_period_end = d1.add_period(Period::new(1, TimeUnit::Years))?;
        m = 12;
    }
    let period = m as f64 / 12.0;
    if d2 <= ref_period_end {
        if d1 >= ref_period_start {
            Ok(period * days(d1, d2) / days(ref_period_start, ref_period_end))
        } else {
            // long first coupon
            let previous_ref = months(ref_period_start, -m)?;
            if d2 > ref_period_start {
                Ok(act_act_isma(d1, ref_period_start, Some(previous_ref), Some(ref_period_start))?
                    + act_act_isma(ref_period_start, d2, Some(ref_period_start), Some(ref_period_end))?)
            } else {
                act_act_isma(d1, d2, Some(previous_ref), Some(ref_period_start))
            }
        }
    } else {
        if ref_period_start > d1 {
            return Err(Error::InvalidReferencePeriod {
                reason: "invalid dates: d1 < refPeriodStart < refPeriodEnd < d2".into(),
            });
        }
        let mut sum = act_act_isma(d1, ref_period_end, Some(ref_period_start), Some(ref_period_end))?;
        let mut i = 0;
        let (new_ref_start, new_ref_end) = loop {
            let new_ref_start = months(ref_period_end, m * i)?;
            let new_ref_end = months(ref_period_end, m * (i + 1))?;
            if d2 < new_ref_end {
                break (new_ref_start, new_ref_end);
            }
            sum += period;
            i += 1;
        };
        sum += act_act_isma(new_ref_start, d2, Some(new_ref_start), Some(new_ref_end))?;
        Ok(sum)
    }
}

fn act_act_isda(d1: Date, d2: Date) -> Result<f64, Error> {
    if d1 == d2 {
        return Ok(0.0);
    }
    if d1 > d2 {
        return Ok(-act_act_isda(d2, d1)?);
    }
    let (y1, y2) = (d1.year(), d2.year());
    let dib1 = if is_leap(y1) { 366.0 } else { 365.0 };
    let dib2 = if is_leap(y2) { 366.0 } else { 365.0 };
    let mut sum = (y2 - y1 - 1) as f64;
    // As in QuantLib, January 1st of y1 + 1 must be a valid date (fails for y1 = 2199).
    sum += days(d1, Date::from_ymd(y1 + 1, 1, 1)?) / dib1;
    sum += days(Date::from_ymd(y2, 1, 1)?, d2) / dib2;
    Ok(sum)
}

fn act_act_afb(d1: Date, d2: Date) -> Result<f64, Error> {
    if d1 == d2 {
        return Ok(0.0);
    }
    if d1 > d2 {
        return Ok(-act_act_afb(d2, d1)?);
    }
    let mut new_d2 = d2;
    let mut temp = d2;
    let mut sum = 0.0;
    while temp > d1 {
        temp = new_d2.add_period(Period::new(-1, TimeUnit::Years))?;
        if temp.day_of_month() == 28 && temp.month() as u32 == 2 && is_leap(temp.year()) {
            temp = temp.add_days(1)?;
        }
        if temp >= d1 {
            sum += 1.0;
            new_d2 = temp;
        }
    }
    let mut den = 365.0;
    if is_leap(new_d2.year()) {
        let feb29 = Date::from_ymd(new_d2.year(), 2, 29)?;
        if new_d2 > feb29 && d1 <= feb29 {
            den += 1.0;
        }
    } else if is_leap(d1.year()) {
        let feb29 = Date::from_ymd(d1.year(), 2, 29)?;
        if new_d2 > feb29 && d1 <= feb29 {
            den += 1.0;
        }
    }
    Ok(sum + days(d1, new_d2) / den)
}

fn actual_365_canadian(d1: Date, d2: Date, ref_start: Option<Date>, ref_end: Option<Date>) -> Result<f64, Error> {
    if d1 == d2 {
        return Ok(0.0);
    }
    let (Some(rs), Some(re)) = (ref_start, ref_end) else {
        return Err(Error::InvalidReferencePeriod { reason: "Actual/365 Canadian needs a reference period".into() });
    };
    let dcs = days(d1, d2);
    let dcc = days(rs, re);
    let months = (12.0 * dcc / 365.0).round() as i32;
    if months == 0 {
        return Err(Error::InvalidReferencePeriod {
            reason: "invalid reference period for Act/365 Canadian; must be longer than a month".into(),
        });
    }
    let frequency = 12 / months;
    if frequency == 0 {
        return Err(Error::InvalidReferencePeriod {
            reason: "invalid reference period for Act/365 Canadian; must not be longer than a year".into(),
        });
    }
    if dcs < (365 / frequency) as f64 {
        return Ok(dcs / 365.0);
    }
    Ok(1.0 / frequency as f64 - (dcc - dcs) / 365.0)
}

impl DayCount {
    /// Number of days in the period according to the convention (QuantLib `dayCount`).
    pub fn day_count(&self, d1: Date, d2: Date) -> i64 {
        let actual = d1.days_until(d2) as i64;
        match self {
            DayCount::Actual360 { include_last_day }
            | DayCount::Actual36525 { include_last_day }
            | DayCount::Actual366 { include_last_day } => actual + *include_last_day as i64,
            DayCount::Actual365Fixed(Actual365Kind::NoLeap) => no_leap_day_count(d1, d2),
            DayCount::Thirty360(kind) => thirty_360(d1, d2, *kind),
            DayCount::Thirty365 => thirty_365(d1, d2),
            DayCount::Simple => thirty_360(d1, d2, Thirty360Kind::BondBasis),
            DayCount::One => {
                if d2 >= d1 {
                    1
                } else {
                    -1
                }
            }
            DayCount::Business252(cal) => cal.business_days_between(d1, d2, true, false) as i64,
            DayCount::Actual364 | DayCount::Actual365Fixed(_) | DayCount::ActualActual(_) => actual,
        }
    }

    /// Period length in years (QuantLib `yearFraction`). The reference period is used by
    /// Actual/Actual ISMA and Actual/365 Canadian and ignored by the others.
    pub fn year_fraction(
        &self,
        d1: Date,
        d2: Date,
        ref_start: Option<Date>,
        ref_end: Option<Date>,
    ) -> Result<f64, Error> {
        let actual = days(d1, d2);
        Ok(match self {
            DayCount::Actual360 { include_last_day } => (actual + *include_last_day as i64 as f64) / 360.0,
            DayCount::Actual364 => actual / 364.0,
            DayCount::Actual36525 { include_last_day } => (actual + *include_last_day as i64 as f64) / 365.25,
            DayCount::Actual366 { include_last_day } => (actual + *include_last_day as i64 as f64) / 366.0,
            DayCount::Actual365Fixed(Actual365Kind::Standard) => actual / 365.0,
            DayCount::Actual365Fixed(Actual365Kind::NoLeap) => no_leap_day_count(d1, d2) as f64 / 365.0,
            DayCount::Actual365Fixed(Actual365Kind::Canadian) => actual_365_canadian(d1, d2, ref_start, ref_end)?,
            DayCount::ActualActual(ActualActualKind::Isda) => act_act_isda(d1, d2)?,
            DayCount::ActualActual(ActualActualKind::Isma) => act_act_isma(d1, d2, ref_start, ref_end)?,
            DayCount::ActualActual(ActualActualKind::Afb) => act_act_afb(d1, d2)?,
            DayCount::Thirty360(kind) => thirty_360(d1, d2, *kind) as f64 / 360.0,
            DayCount::Thirty365 => thirty_365(d1, d2) as f64 / 365.0,
            DayCount::One => self.day_count(d1, d2) as f64,
            DayCount::Simple => {
                let (dm1, dm2) = (d1.day_of_month(), d2.day_of_month());
                if dm1 == dm2 || (dm1 > dm2 && d2.is_end_of_month()) || (dm1 < dm2 && d1.is_end_of_month()) {
                    (d2.year() - d1.year()) as f64 + (d2.month() as i32 - d1.month() as i32) as f64 / 12.0
                } else {
                    thirty_360(d1, d2, Thirty360Kind::BondBasis) as f64 / 360.0
                }
            }
            DayCount::Business252(_) => self.day_count(d1, d2) as f64 / 252.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> Date {
        Date::from_ymd(y, m, day).unwrap()
    }

    #[test]
    fn values_checked_with_quantlib() {
        // QuantLib R binding 1.42, 2024-01-31 -> 2024-07-31.
        let (a, b) = (d(2024, 1, 31), d(2024, 7, 31));
        let act360 = DayCount::Actual360 { include_last_day: false };
        // QuantLib prints 0.50555555555555554, i.e. 182 / 360 in double precision.
        assert_eq!(act360.year_fraction(a, b, None, None).unwrap(), 182.0 / 360.0);
        let bond = DayCount::Thirty360(Thirty360Kind::BondBasis);
        assert_eq!(bond.day_count(a, b), 180);
        let isma = DayCount::ActualActual(ActualActualKind::Isma);
        assert_eq!(isma.year_fraction(a, b, None, None).unwrap(), 0.5);
    }
}
