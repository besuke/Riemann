//! Shared helpers for the golden tests.
#![allow(dead_code)]

use riemann::instruments::FixedRateBondSpec;
use riemann::time::{
    ActualActualKind, Actual365Kind, BusinessDayConvention, Calendar, DateGenerationRule, DayCount,
    HolidayCalendar, Period, ScheduleSpec, Thirty360Kind, TimeUnit, UsMarket,
};
use riemann::Date;
use std::path::PathBuf;

/// Data rows of a golden CSV (comments and header skipped), split on commas.
pub fn golden_rows(file: &str) -> Vec<Vec<String>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden").join(file);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}; run the scripts in tools/golden", path.display()));
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').map(|s| s.trim().to_string()).collect())
        .collect()
}

pub fn date(s: &str) -> Date {
    Date::from_serial(s.parse().unwrap()).unwrap()
}

pub fn optional_date(s: &str) -> Option<Date> {
    if s == "0" {
        None
    } else {
        Some(date(s))
    }
}

pub fn convention(name: &str) -> BusinessDayConvention {
    use BusinessDayConvention::*;
    match name {
        "Following" => Following,
        "ModifiedFollowing" => ModifiedFollowing,
        "Preceding" => Preceding,
        "ModifiedPreceding" => ModifiedPreceding,
        "Unadjusted" => Unadjusted,
        other => panic!("unknown convention {other}"),
    }
}

pub fn day_count(name: &str) -> DayCount {
    match name {
        "actual_365_fixed" => DayCount::Actual365Fixed(Actual365Kind::Standard),
        "actual_360" => DayCount::Actual360 { include_last_day: false },
        "thirty_360_bond_basis" => DayCount::Thirty360(Thirty360Kind::BondBasis),
        "act_act_isma" => DayCount::ActualActual(ActualActualKind::Isma),
        "act_act_isda" => DayCount::ActualActual(ActualActualKind::Isda),
        other => panic!("unknown day counter {other}"),
    }
}

/// Fixed-rate bond inputs from a row of `bond_cases.csv`.
pub fn bond_spec(r: &[String]) -> FixedRateBondSpec {
    let calendar = HolidayCalendar::from(match r[5].as_str() {
        "japan" => Calendar::Japan,
        "target" => Calendar::Target,
        "us_sofr" => Calendar::UnitedStates(UsMarket::Sofr),
        "null_calendar" => Calendar::NullCalendar,
        other => panic!("unknown calendar {other}"),
    });
    let unit = match r[4].as_str() {
        "Months" => TimeUnit::Months,
        "Years" => TimeUnit::Years,
        other => panic!("unknown unit {other}"),
    };
    FixedRateBondSpec {
        settlement_days: r[16].parse().unwrap(),
        face_amount: r[17].parse().unwrap(),
        schedule: ScheduleSpec {
            effective_date: date(&r[1]),
            termination_date: date(&r[2]),
            tenor: Period::new(r[3].parse().unwrap(), unit),
            calendar,
            convention: convention(&r[6]),
            termination_convention: convention(&r[7]),
            rule: if r[8] == "Backward" { DateGenerationRule::Backward } else { DateGenerationRule::Forward },
            end_of_month: r[9] == "1",
            first_date: optional_date(&r[10]),
            next_to_last_date: optional_date(&r[11]),
        },
        coupon_rate: r[12].parse().unwrap(),
        day_count: day_count(&r[13]),
        payment_convention: convention(&r[14]),
        redemption: r[15].parse().unwrap(),
        issue_date: optional_date(&r[18]),
        payment_calendar: None,
    }
}

/// Counts exact and inexact numeric matches.
#[derive(Default)]
pub struct Stats {
    pub exact: usize,
    pub inexact: usize,
    pub max_rel: f64,
    pub max_abs: f64,
}

impl Stats {
    /// True if `value` equals the golden number or is within `rel` relative difference.
    pub fn close(&mut self, golden: &str, value: f64, rel: f64) -> bool {
        let q: f64 = golden.parse().unwrap();
        if q == value {
            self.exact += 1;
            return true;
        }
        let abs = (q - value).abs();
        let r = abs / q.abs().max(1e-300);
        self.inexact += 1;
        self.max_rel = self.max_rel.max(r);
        self.max_abs = self.max_abs.max(abs);
        r <= rel
    }
}
