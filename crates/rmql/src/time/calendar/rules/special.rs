//! Holiday rules of QuantLib C++ v1.42 (commit 41b0e14) that need dates, not only date parts:
//! Israel and Saudi Arabia (date tables and date arithmetic), Russia (year tables; MOEX has no
//! data before 2012) and China's interbank market (working weekends). Written by hand; the date
//! tables are in `tables.rs`. Checked on every valid date by `tests/golden_all_calendars.rs`.
//!
//! As in QuantLib, the conditions are evaluated in order and stop at the first that holds. Date
//! arithmetic outside the valid range is an error, so Israel's calendars fail near both ends of
//! the range on the same dates as QuantLib.
//!
//! Derived from QuantLib (modified BSD license; see THIRD_PARTY_NOTICES):
//!
//! - `ql/time/calendars/israel.cpp`
//!   Copyright (C) 2015 Paolo Mazzocchi
//!   Copyright (C) 2015 Riccardo Barone
//! - `ql/time/calendars/saudiarabia.cpp`
//!   Copyright (C) 2004 FIMAT Group
//!   Copyright (C) 2007, 2011 StatPro Italia srl
//! - `ql/time/calendars/russia.cpp`
//!   Copyright (C) 2010 StatPro Italia srl
//!   Copyright (C) 2015 Dmitri Nesteruk
//! - `ql/time/calendars/china.cpp`
//!   Copyright (C) 2004 FIMAT Group
//!   Copyright (C) 2007, 2009, 2010, 2011 StatPro Italia srl

use super::tables::{
    CHINA_IB_WORKING_WEEKENDS, ISRAEL_FAST_DAY, ISRAEL_INDEPENDENCE_DAY, ISRAEL_NEW_YEARS_DAY,
    ISRAEL_PASSOVER_1ST, ISRAEL_PURIM, ISRAEL_SHAVUOT, SAUDI_EID_AL_ADHA, SAUDI_EID_AL_FITR,
};
use super::{asia_pacific::china_sse, easter_monday, is_weekend, Day};
use crate::time::Date;
use crate::time::Weekday::{Friday, Monday, Saturday, Thursday};
use crate::Error;

fn in_table(table: &[(i32, i32, i32)], date: Date) -> bool {
    let (y, m, d) = date.ymd();
    table.contains(&(y, m as i32, d as i32))
}

fn is_passover_1st(date: Date) -> bool {
    in_table(ISRAEL_PASSOVER_1ST, date)
}

fn is_purim(date: Date) -> bool {
    in_table(ISRAEL_PURIM, date)
}

fn is_independence_day(date: Date) -> bool {
    in_table(ISRAEL_INDEPENDENCE_DAY, date)
}

fn is_memorial_day(date: Date) -> Result<bool, Error> {
    Ok(is_independence_day(date.add_days(1)?))
}

fn is_shavuot(date: Date) -> bool {
    in_table(ISRAEL_SHAVUOT, date)
}

fn is_fast_day(date: Date) -> bool {
    in_table(ISRAEL_FAST_DAY, date)
}

fn is_new_years_day(date: Date) -> bool {
    in_table(ISRAEL_NEW_YEARS_DAY, date)
}

fn is_yom_kippur(date: Date) -> Result<bool, Error> {
    Ok(is_new_years_day(date.add_days(-9)?))
}

fn is_sukkot(date: Date) -> Result<bool, Error> {
    is_yom_kippur(date.add_days(-5)?)
}

fn is_simchat_torah(date: Date) -> Result<bool, Error> {
    is_sukkot(date.add_days(-7)?)
}

/// QuantLib `Israel::TelAvivImpl`, for the markets Settlement and TASE.
pub(in crate::time::calendar) fn israel_tel_aviv(date: Date) -> Result<bool, Error> {
    let x = Day::of(date);
    let (w, y) = (x.w, x.y);
    let holiday = (w == Friday || w == Saturday)
        || is_purim(date)
        || (y <= 2020 && is_passover_1st(date.add_days(1)?)) // Eve of Passover, until 2020
        || is_passover_1st(date)
        || is_passover_1st(date.add_days(-5)?) // Eve of Passover VII, until 2020
        || is_passover_1st(date.add_days(-6)?) // Passover VII
        || is_memorial_day(date)?
        || is_independence_day(date)
        || (y <= 2020 && is_shavuot(date.add_days(1)?)) // Eve of Shavuot, until 2020
        || is_shavuot(date)
        || is_fast_day(date)
        || (y <= 2019 && is_new_years_day(date.add_days(1)?)) // Eve of new year, until 2019
        || is_new_years_day(date)
        || is_new_years_day(date.add_days(-1)?) // 2nd day of new year
        || is_yom_kippur(date.add_days(1)?)? // Eve of Yom Kippur
        || is_yom_kippur(date)?
        || is_sukkot(date.add_days(1)?)? // Eve of Sukkot
        || is_sukkot(date)?
        || is_simchat_torah(date.add_days(1)?)? // Eve of Simchat Torah
        || is_simchat_torah(date)?;
    Ok(!holiday)
}

/// QuantLib `Israel::ShirImpl`.
pub(in crate::time::calendar) fn israel_shir(date: Date) -> Result<bool, Error> {
    let x = Day::of(date);
    let (d, m, y, w, dd) = (x.d, x.m, x.y, x.w, x.dd);
    let holiday = is_weekend(w)
        || is_purim(date)
        || is_purim(date.add_days(-1)?) // Purim (Jerusalem)
        || is_passover_1st(date.add_days(1)?) // Eve of Passover
        || is_passover_1st(date)
        || is_passover_1st(date.add_days(-6)?) // Last day of Passover
        || is_independence_day(date)
        || is_shavuot(date)
        || is_fast_day(date)
        || is_new_years_day(date.add_days(1)?) // Eve of new year, until 2019
        || is_new_years_day(date)
        || is_new_years_day(date.add_days(-1)?) // 2nd day of new year
        || is_yom_kippur(date.add_days(1)?)? // Eve of Yom Kippur
        || is_yom_kippur(date)?
        || is_sukkot(date)?
        || is_simchat_torah(date)?
        // one-off closings
        || (d == 27 && m == 2 && y == 2024) // Municipal elections
        // holidays abroad
        || (d == 1 && m == 1) // Western New Year's day
        || dd == easter_monday(y) - 3 // Good Friday
        || (d >= 25 && w == Monday && m == 5 && y != 2022) // Spring Bank Holiday
        || (d == 3 && m == 6 && y == 2022)
        || (d == 25 && m == 12) // Christmas
        || (d == 26 && m == 12) // Boxing day
        // other days when fixings were not published
        || (d == 1 && m == 11 && y == 2022) // no idea why
        || (d == 2 && m == 1 && y == 2023) // Maybe New Year's Day is adjusted to Monday?
        || (d == 10 && m == 4 && y == 2023); // Easter Monday, not a holiday in 2024 and 2025
    Ok(!holiday)
}

/// The Saudi weekend changed from June 29th, 2013.
fn is_saudi_true_weekend(date: Date) -> bool {
    let w = date.weekday();
    let change = Date::from_ymd(2013, 6, 29).expect("a valid date");
    if date < change {
        w == Thursday || w == Friday
    } else {
        w == Friday || w == Saturday
    }
}

/// From the day before an Eid to four days after it.
fn is_around_eid(table: &[(i32, i32, i32)], date: Date) -> bool {
    let serial = date.serial();
    table.iter().any(|&(y, m, d)| {
        let eid = Date::from_ymd(y, m as u32, d as u32)
            .expect("a valid date")
            .serial();
        serial >= eid - 1 && serial <= eid + 4
    })
}

/// QuantLib `SaudiArabia::TadawulImpl`.
pub(in crate::time::calendar) fn saudi_arabia_tadawul(date: Date) -> bool {
    let x = Day::of(date);
    let (d, m, y) = (x.d, x.m, x.y);
    let holiday = is_saudi_true_weekend(date)
        || is_around_eid(SAUDI_EID_AL_ADHA, date)
        || is_around_eid(SAUDI_EID_AL_FITR, date)
        // National Day
        || (d == 23 && m == 9)
        // other one-shot holidays
        || (d == 26 && m == 2 && y == 2011)
        || (d == 19 && m == 3 && y == 2011);
    !holiday
}

fn is_russia_extra_holiday_settlement(d: i32, m: i32, y: i32) -> bool {
    match (y, m) {
        (2017, 2) => d == 24,
        (2017, 5) => d == 8,
        (2017, 11) => d == 6,
        (2018, 3) => d == 9,
        (2018, 4) => d == 30,
        (2018, 5) => d == 2,
        (2018, 6) => d == 11,
        (2018, 12) => d == 31,
        (2019, 5) => d == 2 || d == 3 || d == 10,
        (2020, 3) => d == 30 || d == 31,
        (2020, 4) => d == 1 || d == 2 || d == 3,
        (2020, 5) => d == 4 || d == 5,
        _ => false,
    }
}

/// QuantLib `Russia::SettlementImpl`.
pub(in crate::time::calendar) fn russia_settlement(x: &Day) -> bool {
    let (d, m, y, w) = (x.d, x.m, x.y, x.w);
    let holiday = is_weekend(w)
        // New Year's holidays
        || (y <= 2005 && d <= 2 && m == 1)
        || (y >= 2005 && d <= 5 && m == 1)
        // in 2012, the 6th was also a holiday
        || (y == 2012 && d == 6 && m == 1)
        // Christmas (possibly moved to Monday)
        || ((d == 7 || ((d == 8 || d == 9) && w == Monday)) && m == 1)
        // Defender of the Fatherland Day (possibly moved to Monday)
        || ((d == 23 || ((d == 24 || d == 25) && w == Monday)) && m == 2)
        // International Women's Day (possibly moved to Monday)
        || ((d == 8 || ((d == 9 || d == 10) && w == Monday)) && m == 3)
        // Labour Day (possibly moved to Monday)
        || ((d == 1 || ((d == 2 || d == 3) && w == Monday)) && m == 5)
        // Victory Day (possibly moved to Monday)
        || ((d == 9 || ((d == 10 || d == 11) && w == Monday)) && m == 5)
        // Russia Day (possibly moved to Monday)
        || ((d == 12 || ((d == 13 || d == 14) && w == Monday)) && m == 6)
        // Unity Day (possibly moved to Monday)
        || ((d == 4 || ((d == 5 || d == 6) && w == Monday)) && m == 11);
    !holiday && !is_russia_extra_holiday_settlement(d, m, y)
}

fn is_moex_working_weekend(d: i32, m: i32, y: i32) -> bool {
    match (y, m) {
        (2012, 3) => d == 11,
        (2012, 4) => d == 28,
        (2012, 5) => d == 5 || d == 12,
        (2012, 6) => d == 9,
        (2016, 2) => d == 20,
        (2018, 4) => d == 28,
        (2018, 6) => d == 9,
        (2018, 12) => d == 29,
        _ => false,
    }
}

fn is_moex_extra_holiday(d: i32, m: i32, y: i32) -> bool {
    match (y, m) {
        (2012, 1) => d == 2,
        (2012, 3) => d == 9,
        (2012, 4) => d == 30,
        (2012, 6) => d == 11,
        (2013, 1) => d == 1 || d == 2 || d == 3 || d == 4 || d == 7,
        (2014, 1) => d == 1 || d == 2 || d == 3 || d == 7,
        (2015, 1) => d == 1 || d == 2 || d == 7,
        (2016, 1) => d == 1 || d == 7 || d == 8,
        (2016, 5) => d == 2 || d == 3,
        (2016, 6) => d == 13,
        (2016, 12) => d == 30,
        (2017, 1) => d == 2,
        (2017, 5) => d == 8,
        (2018, 1) => d == 1 || d == 2 || d == 8,
        (2018, 12) => d == 31,
        (2019, 1) => d == 1 || d == 2 || d == 7,
        (2019, 12) => d == 31,
        (2020, 1) => d == 1 || d == 2 || d == 7,
        (2020, 2) => d == 24,
        (2020, 6) => d == 24,
        (2020, 7) => d == 1,
        _ => false,
    }
}

/// QuantLib `Russia::ExchangeImpl`. The exchange was established in 2011, so there are no data
/// before 2012 and QuantLib fails there.
pub(in crate::time::calendar) fn russia_moex(x: &Day) -> Result<bool, Error> {
    let (d, m, y, w) = (x.d, x.m, x.y, x.w);
    if y < 2012 {
        return Err(Error::InvalidInput {
            reason: format!("MOEX calendar for the year {y} does not exist."),
        });
    }
    if is_moex_working_weekend(d, m, y) {
        return Ok(true);
    }
    let holiday = is_weekend(w)
        // Defender of the Fatherland Day
        || (d == 23 && m == 2)
        // International Women's Day (possibly moved to Monday)
        || ((d == 8 || ((d == 9 || d == 10) && w == Monday)) && m == 3)
        // Labour Day
        || (d == 1 && m == 5)
        // Victory Day (possibly moved to Monday)
        || ((d == 9 || ((d == 10 || d == 11) && w == Monday)) && m == 5)
        // Russia Day
        || (d == 12 && m == 6)
        // Unity Day (possibly moved to Monday)
        || ((d == 4 || ((d == 5 || d == 6) && w == Monday)) && m == 11)
        // New Years Eve
        || (d == 31 && m == 12);
    Ok(!holiday && !is_moex_extra_holiday(d, m, y))
}

/// QuantLib `China::IbImpl`: a business day of the Shanghai exchange, or a working weekend.
pub(in crate::time::calendar) fn china_ib(date: Date) -> bool {
    china_sse(&Day::of(date)) || in_table(CHINA_IB_WORKING_WEEKENDS, date)
}
