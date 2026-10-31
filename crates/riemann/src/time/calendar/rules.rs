//! Holiday rules derived from QuantLib.
//!
//! The business-day conditions in this file reproduce QuantLib's calendars and are
//! therefore treated as derived from QuantLib source code (modified BSD license):
//!
//! - `ql/time/calendars/japan.cpp`
//!   Copyright (C) 2000-2003 RiskMap srl; (C) 2003 Kawanishi Tomoya; (C) 2016, 2019, 2020 Eisuke Tani
//! - `ql/time/calendars/target.cpp`
//!   Copyright (C) 2000, 2001, 2002, 2003 RiskMap srl
//! - `ql/time/calendars/unitedstates.cpp`
//!   Copyright (C) 2004, 2005 Ferdinando Ametrano; (C) 2000, 2001, 2002, 2003 RiskMap srl;
//!   (C) 2003, 2004, 2005, 2006 StatPro Italia srl; (C) 2017 Peter Caspers; (C) 2017 Oleg Kulkov;
//!   (C) 2023 Skandinaviska Enskilda Banken AB (publ); (C) 2024 Dirk Eddelbuettel
//! - `ql/time/calendars/unitedkingdom.cpp`
//!   Copyright (C) 2004 Ferdinando Ametrano; (C) 2000, 2001, 2002, 2003 RiskMap srl
//!
//! QuantLib commit 70f240f29ddb84927ff654502f1d75b3de24ba43 (these files are unchanged
//! since v1.42). See THIRD_PARTY_NOTICES for the QuantLib license.
//!
//! The Japan rules are QuantLib's representation, not the text of the Japanese public
//! holiday law (see `docs/spec_dates_calendars.md`, section 3.3).

use super::easter::easter_monday_day_of_year;
use crate::time::{Date, Weekday};

/// The parts of a date the rules look at.
pub(super) struct Day {
    pub d: i32,
    pub m: i32,
    pub y: i32,
    pub w: Weekday,
    /// Day of the year (January 1st = 1).
    pub dd: i32,
}

impl Day {
    pub fn of(date: Date) -> Day {
        let (y, m, d) = date.ymd();
        let jan1 = Date::from_ymd(y, 1, 1).expect("January 1st of a valid year");
        Day { d: d as i32, m: m as i32, y, w: date.weekday(), dd: jan1.days_until(date) + 1 }
    }
}

use Weekday::{Friday, Monday, Saturday, Sunday, Thursday, Tuesday, Wednesday};

fn is_weekend(w: Weekday) -> bool {
    w == Saturday || w == Sunday
}

fn easter_monday(y: i32) -> i32 {
    easter_monday_day_of_year(y) as i32
}

pub(super) fn japan(x: &Day) -> bool {
    let (d, m, y, w) = (x.d, x.m, x.y, x.w);
    // equinox calculation (integer division and float-to-integer conversion truncate toward zero, as in C++)
    let exact_vernal_equinox_time = 20.69115_f64;
    let exact_autumnal_equinox_time = 23.09_f64;
    let diff_per_year = 0.242194_f64;
    let moving_amount = (y - 2000) as f64 * diff_per_year;
    let number_of_leap_years = (y - 2000) / 4 + (y - 2000) / 100 - (y - 2000) / 400;
    let ve = (exact_vernal_equinox_time + moving_amount - number_of_leap_years as f64) as i32;
    let ae = (exact_autumnal_equinox_time + moving_amount - number_of_leap_years as f64) as i32;
    let holiday = is_weekend(w)
        // New Year's Day
        || (d == 1 && m == 1)
        // Bank Holiday
        || (d == 2 && m == 1)
        // Bank Holiday
        || (d == 3 && m == 1)
        // Coming of Age Day (2nd Monday in January), was January 15th until 2000
        || (w == Monday && (8..=14).contains(&d) && m == 1 && y >= 2000)
        || ((d == 15 || (d == 16 && w == Monday)) && m == 1 && y < 2000)
        // National Foundation Day
        || ((d == 11 || (d == 12 && w == Monday)) && m == 2)
        // Emperor's Birthday (Emperor Naruhito)
        || ((d == 23 || (d == 24 && w == Monday)) && m == 2 && y >= 2020)
        // Emperor's Birthday (Emperor Akihito)
        || ((d == 23 || (d == 24 && w == Monday)) && m == 12 && (1989..2019).contains(&y))
        // Vernal Equinox
        || ((d == ve || (d == ve + 1 && w == Monday)) && m == 3)
        // Greenery Day
        || ((d == 29 || (d == 30 && w == Monday)) && m == 4)
        // Constitution Memorial Day
        || (d == 3 && m == 5)
        // Holiday for a Nation
        || (d == 4 && m == 5)
        // Children's Day
        || (d == 5 && m == 5)
        // any of the three above observed later if on Saturday or Sunday
        || (d == 6 && m == 5 && (w == Monday || w == Tuesday || w == Wednesday))
        // Marine Day (3rd Monday in July), was July 20th until 2003, not a holiday before 1996,
        // July 23rd in 2020 and July 22nd in 2021 due to the Olympic games
        || (w == Monday && (15..=21).contains(&d) && m == 7 && ((2003..2020).contains(&y) || y >= 2022))
        || ((d == 20 || (d == 21 && w == Monday)) && m == 7 && (1996..2003).contains(&y))
        || (d == 23 && m == 7 && y == 2020)
        || (d == 22 && m == 7 && y == 2021)
        // Mountain Day (moved in 2020 and 2021 due to the Olympic games)
        || ((d == 11 || (d == 12 && w == Monday)) && m == 8 && ((2016..2020).contains(&y) || y >= 2022))
        || (d == 10 && m == 8 && y == 2020)
        || (d == 9 && m == 8 && y == 2021)
        // Respect for the Aged Day (3rd Monday in September), was September 15th until 2003
        || (w == Monday && (15..=21).contains(&d) && m == 9 && y >= 2003)
        || ((d == 15 || (d == 16 && w == Monday)) && m == 9 && y < 2003)
        // a single day between Respect for the Aged Day and the Autumnal Equinox is a holiday
        || (w == Tuesday && d + 1 == ae && (16..=22).contains(&d) && m == 9 && y >= 2003)
        // Autumnal Equinox
        || ((d == ae || (d == ae + 1 && w == Monday)) && m == 9)
        // Health and Sports Day (2nd Monday in October), was October 10th until 2000,
        // July 24th in 2020 and July 23rd in 2021 due to the Olympic games
        || (w == Monday && (8..=14).contains(&d) && m == 10 && ((2000..2020).contains(&y) || y >= 2022))
        || ((d == 10 || (d == 11 && w == Monday)) && m == 10 && y < 2000)
        || (d == 24 && m == 7 && y == 2020)
        || (d == 23 && m == 7 && y == 2021)
        // National Culture Day
        || ((d == 3 || (d == 4 && w == Monday)) && m == 11)
        // Labor Thanksgiving Day
        || ((d == 23 || (d == 24 && w == Monday)) && m == 11)
        // Bank Holiday
        || (d == 31 && m == 12)
        // one-shot holidays
        || (d == 10 && m == 4 && y == 1959) // Marriage of Prince Akihito
        || (d == 24 && m == 2 && y == 1989) // Rites of Imperial Funeral
        || (d == 12 && m == 11 && y == 1990) // Enthronement Ceremony (Emperor Akihito)
        || (d == 9 && m == 6 && y == 1993) // Marriage of Prince Naruhito
        || (d == 30 && m == 4 && y == 2019) // special holiday (public holidays law)
        || (d == 1 && m == 5 && y == 2019) // Enthronement Day (Emperor Naruhito)
        || (d == 2 && m == 5 && y == 2019) // special holiday (public holidays law)
        || (d == 22 && m == 10 && y == 2019); // Enthronement Ceremony (Emperor Naruhito)
    !holiday
}

pub(super) fn target(x: &Day) -> bool {
    let (d, m, y, w, dd) = (x.d, x.m, x.y, x.w, x.dd);
    let em = easter_monday(y);
    let holiday = is_weekend(w)
        // New Year's Day
        || (d == 1 && m == 1)
        // Good Friday
        || (dd == em - 3 && y >= 2000)
        // Easter Monday
        || (dd == em && y >= 2000)
        // Labour Day
        || (d == 1 && m == 5 && y >= 2000)
        // Christmas
        || (d == 25 && m == 12)
        // Day of Goodwill
        || (d == 26 && m == 12 && y >= 2000)
        // December 31st, 1998, 1999, and 2001 only
        || (d == 31 && m == 12 && (y == 1998 || y == 1999 || y == 2001));
    !holiday
}

// --- United States: rules shared by several markets

fn is_washington_birthday(d: i32, m: i32, y: i32, w: Weekday) -> bool {
    if y >= 1971 {
        // third Monday in February
        (15..=21).contains(&d) && w == Monday && m == 2
    } else {
        // February 22nd, possibly adjusted
        (d == 22 || (d == 23 && w == Monday) || (d == 21 && w == Friday)) && m == 2
    }
}

fn is_memorial_day(d: i32, m: i32, y: i32, w: Weekday) -> bool {
    if y >= 1971 {
        // last Monday in May
        d >= 25 && w == Monday && m == 5
    } else {
        // May 30th, possibly adjusted
        (d == 30 || (d == 31 && w == Monday) || (d == 29 && w == Friday)) && m == 5
    }
}

fn is_labor_day(d: i32, m: i32, w: Weekday) -> bool {
    // first Monday in September
    d <= 7 && w == Monday && m == 9
}

fn is_columbus_day(d: i32, m: i32, y: i32, w: Weekday) -> bool {
    // second Monday in October
    (8..=14).contains(&d) && w == Monday && m == 10 && y >= 1971
}

fn is_veterans_day(d: i32, m: i32, y: i32, w: Weekday) -> bool {
    if y <= 1970 || y >= 1978 {
        // November 11th, adjusted
        (d == 11 || (d == 12 && w == Monday) || (d == 10 && w == Friday)) && m == 11
    } else {
        // fourth Monday in October
        (22..=28).contains(&d) && w == Monday && m == 10
    }
}

fn is_veterans_day_no_saturday(d: i32, m: i32, y: i32, w: Weekday) -> bool {
    if y <= 1970 || y >= 1978 {
        // November 11th, adjusted, but no Saturday to Friday
        (d == 11 || (d == 12 && w == Monday)) && m == 11
    } else {
        // fourth Monday in October
        (22..=28).contains(&d) && w == Monday && m == 10
    }
}

fn is_juneteenth(d: i32, m: i32, y: i32, w: Weekday) -> bool {
    // declared in 2021, but only observed by exchanges since 2022
    (d == 19 || (d == 20 && w == Monday) || (d == 18 && w == Friday)) && m == 6 && y >= 2022
}

pub(super) fn us_settlement(x: &Day) -> bool {
    let (d, m, y, w) = (x.d, x.m, x.y, x.w);
    let holiday = is_weekend(w)
        // New Year's Day (possibly moved to Monday if on Sunday)
        || ((d == 1 || (d == 2 && w == Monday)) && m == 1)
        // (or to Friday if on Saturday)
        || (d == 31 && w == Friday && m == 12)
        // Martin Luther King's birthday (third Monday in January)
        || ((15..=21).contains(&d) && w == Monday && m == 1 && y >= 1983)
        // Washington's birthday (third Monday in February)
        || is_washington_birthday(d, m, y, w)
        // Memorial Day (last Monday in May)
        || is_memorial_day(d, m, y, w)
        // Juneteenth (Monday if Sunday or Friday if Saturday)
        || is_juneteenth(d, m, y, w)
        // Independence Day (Monday if Sunday or Friday if Saturday)
        || ((d == 4 || (d == 5 && w == Monday) || (d == 3 && w == Friday)) && m == 7)
        // Labor Day (first Monday in September)
        || is_labor_day(d, m, w)
        // Columbus Day (second Monday in October)
        || is_columbus_day(d, m, y, w)
        // Veteran's Day (Monday if Sunday or Friday if Saturday)
        || is_veterans_day(d, m, y, w)
        // Thanksgiving Day (fourth Thursday in November)
        || ((22..=28).contains(&d) && w == Thursday && m == 11)
        // Christmas (Monday if Sunday or Friday if Saturday)
        || ((d == 25 || (d == 26 && w == Monday) || (d == 24 && w == Friday)) && m == 12);
    !holiday
}

pub(super) fn us_government_bond(x: &Day) -> bool {
    let (d, m, y, w, dd) = (x.d, x.m, x.y, x.w, x.dd);
    let em = easter_monday(y);
    let holiday = is_weekend(w)
        // New Year's Day (possibly moved to Monday if on Sunday)
        || ((d == 1 || (d == 2 && w == Monday)) && m == 1)
        // Martin Luther King's birthday (third Monday in January)
        || ((15..=21).contains(&d) && w == Monday && m == 1 && y >= 1983)
        // Washington's birthday (third Monday in February)
        || is_washington_birthday(d, m, y, w)
        // Good Friday; since 1996 an early close, not a full close, when it is the first Friday of the month
        || (dd == em - 3 && (y < 1996 || d > 7))
        // Memorial Day (last Monday in May)
        || is_memorial_day(d, m, y, w)
        // Juneteenth (Monday if Sunday or Friday if Saturday)
        || is_juneteenth(d, m, y, w)
        // Independence Day (Monday if Sunday or Friday if Saturday)
        || ((d == 4 || (d == 5 && w == Monday) || (d == 3 && w == Friday)) && m == 7)
        // Labor Day (first Monday in September)
        || is_labor_day(d, m, w)
        // Columbus Day (second Monday in October)
        || is_columbus_day(d, m, y, w)
        // Veteran's Day (Monday if Sunday)
        || is_veterans_day_no_saturday(d, m, y, w)
        // Thanksgiving Day (fourth Thursday in November)
        || ((22..=28).contains(&d) && w == Thursday && m == 11)
        // Christmas (Monday if Sunday or Friday if Saturday)
        || ((d == 25 || (d == 26 && w == Monday) || (d == 24 && w == Friday)) && m == 12);
    if holiday {
        return false;
    }
    // Special closings
    let special = (y == 2018 && m == 12 && d == 5) // President Bush's funeral
        || (y == 2012 && m == 10 && d == 30) // Hurricane Sandy
        || (y == 2004 && m == 6 && d == 11); // President Reagan's funeral
    !special
}

pub(super) fn us_sofr(x: &Day) -> bool {
    // SOFR has not fixed on Good Friday; QuantLib extrapolates that pattern.
    if x.dd == easter_monday(x.y) - 3 {
        return false;
    }
    us_government_bond(x)
}

fn is_uk_bank_holiday(d: i32, w: Weekday, m: i32, y: i32) -> bool {
    // first Monday of May (Early May Bank Holiday), moved to May 8th in 1995 and 2020 for V.E. day
    (d <= 7 && w == Monday && m == 5 && y != 1995 && y != 2020)
        || (d == 8 && m == 5 && (y == 1995 || y == 2020))
        // last Monday of May (Spring Bank Holiday), moved in 2002, 2012 and 2022 for the
        // Golden, Diamond and Platinum Jubilee with an additional holiday
        || (d >= 25 && w == Monday && m == 5 && y != 2002 && y != 2012 && y != 2022)
        || ((d == 3 || d == 4) && m == 6 && y == 2002)
        || ((d == 4 || d == 5) && m == 6 && y == 2012)
        || ((d == 2 || d == 3) && m == 6 && y == 2022)
        // last Monday of August (Summer Bank Holiday)
        || (d >= 25 && w == Monday && m == 8)
        // April 29th, 2011 only (Royal Wedding Bank Holiday)
        || (d == 29 && m == 4 && y == 2011)
        // September 19th, 2022 only (The Queen's Funeral Bank Holiday)
        || (d == 19 && m == 9 && y == 2022)
        // May 8th, 2023 (King Charles III Coronation Bank Holiday)
        || (d == 8 && m == 5 && y == 2023)
}

pub(super) fn uk_settlement(x: &Day) -> bool {
    let (d, m, y, w, dd) = (x.d, x.m, x.y, x.w, x.dd);
    let em = easter_monday(y);
    let holiday = is_weekend(w)
        // New Year's Day (possibly moved to Monday)
        || ((d == 1 || ((d == 2 || d == 3) && w == Monday)) && m == 1)
        // Good Friday
        || (dd == em - 3)
        // Easter Monday
        || (dd == em)
        || is_uk_bank_holiday(d, w, m, y)
        // Christmas (possibly moved to Monday or Tuesday)
        || ((d == 25 || (d == 27 && (w == Monday || w == Tuesday))) && m == 12)
        // Boxing Day (possibly moved to Monday or Tuesday)
        || ((d == 26 || (d == 28 && (w == Monday || w == Tuesday))) && m == 12)
        // December 31st, 1999 only
        || (d == 31 && m == 12 && y == 1999);
    !holiday
}
