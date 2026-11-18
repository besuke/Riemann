//! Periods and calendar-day arithmetic (`Date + Period` in QuantLib).
//!
//! Month and year arithmetic clamps the day to the length of the target month.
//! Keeping end of month is not done here; it belongs to calendar adjustment.

use super::date::{is_leap, month_length, serial_from_civil};
use super::Date;
use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeUnit {
    Days,
    Weeks,
    Months,
    Years,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Period {
    pub length: i32,
    pub unit: TimeUnit,
}

impl Period {
    pub fn new(length: i32, unit: TimeUnit) -> Period {
        Period { length, unit }
    }
}

impl Date {
    /// `self + period` with QuantLib's `Date::advance` rules.
    pub fn add_period(self, period: Period) -> Result<Date, Error> {
        let n = period.length as i64;
        match period.unit {
            TimeUnit::Days => self.add_days(n),
            TimeUnit::Weeks => self.add_days(7 * n),
            TimeUnit::Months => {
                let (y, m, d) = self.ymd();
                let total = y as i64 * 12 + (m as i64 - 1) + n;
                let year = total.div_euclid(12);
                let month = (total.rem_euclid(12) + 1) as u32;
                let year = checked_year(year)?;
                let day = d.min(month_length(month, is_leap(year)));
                Ok(Date::from_serial(serial_from_civil(year, month, day))?)
            }
            TimeUnit::Years => {
                let (y, m, d) = self.ymd();
                let year = checked_year(y as i64 + n)?;
                let day = if m == 2 && d == 29 && !is_leap(year) { 28 } else { d };
                Ok(Date::from_serial(serial_from_civil(year, m, day))?)
            }
        }
    }

    /// `self - period`.
    pub fn sub_period(self, period: Period) -> Result<Date, Error> {
        self.add_period(Period::new(-period.length, period.unit))
    }
}

fn checked_year(year: i64) -> Result<i32, Error> {
    if (1901..=2199).contains(&year) {
        Ok(year as i32)
    } else {
        Err(Error::YearOutOfRange { year: year.clamp(i32::MIN as i64, i32::MAX as i64) as i32 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> Date {
        Date::from_ymd(y, m, day).unwrap()
    }

    #[test]
    fn months_clamp_to_month_end() {
        let p = Period::new(1, TimeUnit::Months);
        assert_eq!(d(2024, 1, 31).add_period(p).unwrap(), d(2024, 2, 29));
        assert_eq!(d(2023, 1, 31).add_period(p).unwrap(), d(2023, 2, 28));
        assert_eq!(d(2024, 2, 29).add_period(Period::new(1, TimeUnit::Years)).unwrap(), d(2025, 2, 28));
    }

    #[test]
    fn out_of_range_is_an_error() {
        assert!(d(2199, 12, 1).add_period(Period::new(1, TimeUnit::Months)).is_err());
        assert!(d(1901, 1, 1).add_period(Period::new(-1, TimeUnit::Days)).is_err());
    }
}
