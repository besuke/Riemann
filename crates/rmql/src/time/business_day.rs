//! Business day conventions: moving dates that fall on holidays
//! (QuantLib-compatible; see `docs/spec_dates_calendars.md`, chapter 4).
//!
//! Not to be confused with day count conventions, which measure year fractions.
//!
//! The adjustment and advance algorithms reproduce `ql/time/calendar.cpp` and are
//! treated as derived from QuantLib source code (modified BSD license):
//! Copyright (C) 2000, 2001, 2002, 2003 RiskMap srl; (C) 2003, 2004, 2005, 2006, 2007
//! StatPro Italia srl; (C) 2004 Jeff Yu; (C) 2014 Paolo Mazzocchi; (C) 2020 Leonardo Arcari;
//! (C) 2020 Kline s.r.l. QuantLib commit 70f240f29ddb84927ff654502f1d75b3de24ba43
//! (unchanged since v1.42). See THIRD_PARTY_NOTICES for the QuantLib license.

use super::{Date, HolidayCalendar, Period, TimeUnit};
use crate::Error;

/// How to move a date that falls on a holiday (QuantLib `BusinessDayConvention`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BusinessDayConvention {
    /// First business day after the holiday.
    Following,
    /// Following, unless that crosses into the next month; then Preceding.
    ModifiedFollowing,
    /// Last business day before the holiday.
    Preceding,
    /// Preceding, unless that crosses into the previous month; then Following.
    ModifiedPreceding,
    /// No adjustment.
    Unadjusted,
    /// ModifiedFollowing, and also Preceding if it would cross the middle of the month.
    HalfMonthModifiedFollowing,
    /// The nearest business day; the later one on a tie.
    Nearest,
}

use BusinessDayConvention::*;

impl HolidayCalendar {
    fn following(&self, d: Date) -> Result<Date, Error> {
        let mut d1 = d;
        while self.is_holiday(d1)? {
            d1 = d1.add_days(1)?;
        }
        Ok(d1)
    }

    fn preceding(&self, d: Date) -> Result<Date, Error> {
        let mut d1 = d;
        while self.is_holiday(d1)? {
            d1 = d1.add_days(-1)?;
        }
        Ok(d1)
    }

    /// Moves `d` to a business day according to `convention` (QuantLib `Calendar::adjust`).
    pub fn adjust(&self, d: Date, convention: BusinessDayConvention) -> Result<Date, Error> {
        match convention {
            Unadjusted => Ok(d),
            Following => self.following(d),
            ModifiedFollowing | HalfMonthModifiedFollowing => {
                let d1 = self.following(d)?;
                if d1.month() != d.month() {
                    return self.preceding(d);
                }
                if convention == HalfMonthModifiedFollowing && d.day_of_month() <= 15 && d1.day_of_month() > 15 {
                    return self.preceding(d);
                }
                Ok(d1)
            }
            Preceding => self.preceding(d),
            ModifiedPreceding => {
                let d1 = self.preceding(d)?;
                if d1.month() != d.month() {
                    return self.following(d);
                }
                Ok(d1)
            }
            Nearest => {
                let (mut d1, mut d2) = (d, d);
                while self.is_holiday(d1)? && self.is_holiday(d2)? {
                    d1 = d1.add_days(1)?;
                    d2 = d2.add_days(-1)?;
                }
                Ok(if self.is_holiday(d1)? { d2 } else { d1 })
            }
        }
    }

    /// Moves `d` by `period` (QuantLib `Calendar::advance`).
    ///
    /// Days count business days. Weeks, months and years move by calendar time and then
    /// adjust; with `end_of_month`, a month end maps to a month end (calendar month end
    /// for `Unadjusted`, business month end otherwise).
    pub fn advance(
        &self,
        d: Date,
        period: Period,
        convention: BusinessDayConvention,
        end_of_month: bool,
    ) -> Result<Date, Error> {
        let n = period.length;
        if n == 0 {
            return self.adjust(d, convention);
        }
        match period.unit {
            TimeUnit::Days => {
                let step = if n > 0 { 1 } else { -1 };
                let mut d1 = d;
                for _ in 0..n.unsigned_abs() {
                    d1 = d1.add_days(step)?;
                    while self.is_holiday(d1)? {
                        d1 = d1.add_days(step)?;
                    }
                }
                Ok(d1)
            }
            TimeUnit::Weeks => self.adjust(d.add_period(period)?, convention),
            TimeUnit::Months | TimeUnit::Years => {
                let d1 = d.add_period(period)?;
                if end_of_month {
                    if convention == Unadjusted {
                        if d.is_end_of_month() {
                            return Ok(d1.end_of_month());
                        }
                    } else if self.is_end_of_month(d)? {
                        return self.end_of_month(d1);
                    }
                }
                self.adjust(d1, convention)
            }
        }
    }

    /// Last business day of the month of `d` (QuantLib `Calendar::endOfMonth`).
    pub fn end_of_month(&self, d: Date) -> Result<Date, Error> {
        self.adjust(d.end_of_month(), Preceding)
    }

    /// Whether `d` is on or after the last business day of its month.
    pub fn is_end_of_month(&self, d: Date) -> Result<bool, Error> {
        Ok(d >= self.end_of_month(d)?)
    }

    /// First business day of the month of `d` (QuantLib `Calendar::startOfMonth`).
    pub fn start_of_month(&self, d: Date) -> Result<Date, Error> {
        self.adjust(d.start_of_month(), Following)
    }

    /// Whether `d` is on or before the first business day of its month.
    pub fn is_start_of_month(&self, d: Date) -> Result<bool, Error> {
        Ok(d <= self.start_of_month(d)?)
    }

    /// Business days between `from` and `to` (QuantLib `Calendar::businessDaysBetween`);
    /// negative when `from` is after `to`.
    pub fn business_days_between(
        &self,
        from: Date,
        to: Date,
        include_first: bool,
        include_last: bool,
    ) -> Result<i32, Error> {
        let count = |a: Date, b: Date, first: bool, last: bool| -> Result<i32, Error> {
            let mut res = (last && self.is_business_day(b)?) as i32;
            let start = if first { a.serial() } else { a.serial() + 1 };
            for s in start..b.serial() {
                let d = Date::from_serial(s as i64).expect("between two valid dates");
                res += self.is_business_day(d)? as i32;
            }
            Ok(res)
        };
        if from < to {
            count(from, to, include_first, include_last)
        } else if from > to {
            Ok(-count(to, from, include_last, include_first)?)
        } else {
            Ok((include_first && include_last && self.is_business_day(from)?) as i32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::Calendar;

    #[test]
    fn showa_day_2024_moves_to_next_business_day() {
        let jp = HolidayCalendar::from(Calendar::Japan);
        let d = Date::from_ymd(2024, 4, 29).unwrap();
        assert_eq!(jp.adjust(d, ModifiedFollowing).unwrap(), Date::from_ymd(2024, 4, 30).unwrap());
    }

    #[test]
    fn adjusting_past_the_range_is_an_error() {
        let wk = HolidayCalendar::from(Calendar::WeekendsOnly);
        // 2199-12-31 is a Tuesday; 2199-12-28 is a Saturday.
        let sat = Date::from_ymd(2199, 12, 28).unwrap();
        assert!(wk.adjust(sat, Following).is_ok());
        let jp = HolidayCalendar::from(Calendar::Japan);
        assert!(jp.adjust(Date::MAX, Following).is_err());
    }
}
