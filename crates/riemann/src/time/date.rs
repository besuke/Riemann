//! Calendar dates as QuantLib serial numbers.
//!
//! The serial number is the same as QuantLib's (and Excel's) in the valid range
//! 367 (1901-01-01) to 109574 (2199-12-31). QuantLib numbers days as if 1900 were
//! a leap year (Excel compatibility); 1900 is outside the range, so dates in range
//! are unaffected and can be converted with the proleptic Gregorian calendar.

use crate::Error;
use std::fmt;

/// Serial number of 1970-01-01 in the QuantLib/Excel numbering.
const SERIAL_OF_UNIX_EPOCH: i64 = 25_569;

/// A date, stored as its QuantLib serial number. Always within the valid range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(i32);

/// Day of the week, numbered as in QuantLib (Sunday = 1, ..., Saturday = 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Weekday {
    Sunday = 1,
    Monday = 2,
    Tuesday = 3,
    Wednesday = 4,
    Thursday = 5,
    Friday = 6,
    Saturday = 7,
}

/// Month of the year (January = 1, ..., December = 12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Month {
    January = 1,
    February = 2,
    March = 3,
    April = 4,
    May = 5,
    June = 6,
    July = 7,
    August = 8,
    September = 9,
    October = 10,
    November = 11,
    December = 12,
}

impl Weekday {
    /// QuantLib weekday number (Sunday = 1, ..., Saturday = 7).
    pub fn number(self) -> u32 {
        self as u32
    }

    fn from_number(n: u32) -> Weekday {
        match n {
            1 => Weekday::Sunday,
            2 => Weekday::Monday,
            3 => Weekday::Tuesday,
            4 => Weekday::Wednesday,
            5 => Weekday::Thursday,
            6 => Weekday::Friday,
            _ => Weekday::Saturday,
        }
    }
}

impl Month {
    pub fn number(self) -> u32 {
        self as u32
    }

    pub fn from_number(n: u32) -> Option<Month> {
        Some(match n {
            1 => Month::January,
            2 => Month::February,
            3 => Month::March,
            4 => Month::April,
            5 => Month::May,
            6 => Month::June,
            7 => Month::July,
            8 => Month::August,
            9 => Month::September,
            10 => Month::October,
            11 => Month::November,
            12 => Month::December,
            _ => return None,
        })
    }
}

impl Date {
    pub const MIN_SERIAL: i32 = 367;
    pub const MAX_SERIAL: i32 = 109_574;

    /// Earliest valid date, 1901-01-01.
    pub const MIN: Date = Date(Self::MIN_SERIAL);
    /// Latest valid date, 2199-12-31.
    pub const MAX: Date = Date(Self::MAX_SERIAL);

    /// Date from a QuantLib serial number.
    pub fn from_serial(serial: i64) -> Result<Date, Error> {
        if serial < Self::MIN_SERIAL as i64 || serial > Self::MAX_SERIAL as i64 {
            return Err(Error::SerialOutOfRange { serial });
        }
        Ok(Date(serial as i32))
    }

    /// Date from year, month (1-12) and day. Years must be in [1901, 2199].
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Result<Date, Error> {
        let invalid = Error::InvalidDate { year, month, day };
        if !(1901..=2199).contains(&year) || !(1..=12).contains(&month) {
            return Err(invalid);
        }
        if day < 1 || day > month_length(month, is_leap(year)) {
            return Err(invalid);
        }
        Ok(Date(serial_from_civil(year, month, day) as i32))
    }

    pub fn serial(self) -> i32 {
        self.0
    }

    pub fn year(self) -> i32 {
        civil_from_serial(self.0 as i64).0
    }

    pub fn month(self) -> Month {
        Month::from_number(civil_from_serial(self.0 as i64).1).expect("month in 1..=12")
    }

    pub fn day_of_month(self) -> u32 {
        civil_from_serial(self.0 as i64).2
    }

    /// Year, month (1-12) and day.
    pub fn ymd(self) -> (i32, u32, u32) {
        civil_from_serial(self.0 as i64)
    }

    /// Day of the week; QuantLib takes the serial number modulo 7 (0 is Saturday).
    pub fn weekday(self) -> Weekday {
        let w = (self.0 % 7) as u32;
        Weekday::from_number(if w == 0 { 7 } else { w })
    }

    /// Last calendar day of the month this date belongs to.
    pub fn end_of_month(self) -> Date {
        let (y, m, _) = self.ymd();
        Date(serial_from_civil(y, m, month_length(m, is_leap(y))) as i32)
    }

    /// First calendar day of the month this date belongs to.
    pub fn start_of_month(self) -> Date {
        let (y, m, _) = self.ymd();
        Date(serial_from_civil(y, m, 1) as i32)
    }

    /// Whether this date is the last calendar day of its month.
    pub fn is_end_of_month(self) -> bool {
        let (y, m, d) = self.ymd();
        d == month_length(m, is_leap(y))
    }

    /// Calendar days from `self` to `other` (positive if `other` is later).
    pub fn days_until(self, other: Date) -> i32 {
        other.0 - self.0
    }

    /// Adds calendar days; fails outside the valid range.
    pub fn add_days(self, days: i64) -> Result<Date, Error> {
        Date::from_serial(self.0 as i64 + days)
    }
}

/// Leap years as in QuantLib's table: Gregorian rule, with 1900 treated as leap
/// (Excel compatibility; 1900 is outside the valid date range).
pub fn is_leap(year: i32) -> bool {
    if year == 1900 {
        return true;
    }
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub(crate) fn month_length(month: u32, leap: bool) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if leap {
                29
            } else {
                28
            }
        }
    }
}

/// QuantLib serial number of a proleptic Gregorian date (valid for dates after 1900-02-28).
pub(crate) fn serial_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = year as i64 - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = month as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468 + SERIAL_OF_UNIX_EPOCH
}

fn civil_from_serial(serial: i64) -> (i32, u32, u32) {
    let z = serial - SERIAL_OF_UNIX_EPOCH + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400 + if month <= 2 { 1 } else { 0 }) as i32;
    (year, month, day)
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (y, m, d) = self.ymd();
        write!(f, "{y:04}-{m:02}-{d:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_limits_match_quantlib() {
        assert_eq!(Date::MIN.ymd(), (1901, 1, 1));
        assert_eq!(Date::MAX.ymd(), (2199, 12, 31));
        assert!(Date::from_serial(366).is_err());
        assert!(Date::from_serial(109_575).is_err());
        assert!(Date::from_ymd(1900, 12, 31).is_err());
        assert!(Date::from_ymd(2200, 1, 1).is_err());
    }

    #[test]
    fn known_dates() {
        let d = Date::from_ymd(2024, 1, 31).unwrap();
        assert_eq!(d.serial(), 45_322);
        assert_eq!(d.weekday(), Weekday::Wednesday);
        assert_eq!(d.to_string(), "2024-01-31");
        assert!(Date::from_ymd(2023, 2, 29).is_err());
        assert!(Date::from_ymd(2024, 2, 29).is_ok());
    }

    #[test]
    fn date_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Date>();
    }
}
