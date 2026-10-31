//! Compares business day conventions with golden values from the QuantLib R binding
//! (`tools/golden/p1_business_days.R`). Results must match exactly, including errors.

use riemann::time::{
    BusinessDayConvention, Calendar, Date, HolidayCalendar, Period, TimeUnit, UkMarket, UsMarket,
};
use riemann::Error;
use std::path::PathBuf;

fn golden_rows(file: &str) -> Vec<Vec<String>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden").join(file);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}; run tools/golden/p1_business_days.R", path.display()));
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').map(|s| s.trim().to_string()).collect())
        .collect()
}

fn calendar(name: &str) -> HolidayCalendar {
    HolidayCalendar::from(match name {
        "japan" => Calendar::Japan,
        "target" => Calendar::Target,
        "us_sofr" => Calendar::UnitedStates(UsMarket::Sofr),
        "uk_settlement" => Calendar::UnitedKingdom(UkMarket::Settlement),
        other => panic!("unknown calendar {other}"),
    })
}

fn convention(name: &str) -> BusinessDayConvention {
    use BusinessDayConvention::*;
    match name {
        "Following" => Following,
        "ModifiedFollowing" => ModifiedFollowing,
        "Preceding" => Preceding,
        "ModifiedPreceding" => ModifiedPreceding,
        "Unadjusted" => Unadjusted,
        "HalfMonthModifiedFollowing" => HalfMonthModifiedFollowing,
        "Nearest" => Nearest,
        other => panic!("unknown convention {other}"),
    }
}

fn unit(name: &str) -> TimeUnit {
    match name {
        "Days" => TimeUnit::Days,
        "Weeks" => TimeUnit::Weeks,
        "Months" => TimeUnit::Months,
        "Years" => TimeUnit::Years,
        other => panic!("unknown unit {other}"),
    }
}

fn date(s: &str) -> Date {
    Date::from_serial(s.parse().unwrap()).unwrap()
}

/// Compares a Riemann result with a golden cell ("ERROR" or a value).
fn same<T: ToString>(golden: &str, got: &Result<T, Error>) -> bool {
    match got {
        Ok(v) => golden == v.to_string(),
        Err(_) => golden == "ERROR",
    }
}

fn check(name: &str, mismatches: Vec<String>, total: usize) {
    assert!(total > 0, "{name}: no golden rows");
    assert!(
        mismatches.is_empty(),
        "{name}: {} of {total} mismatches, first: {:#?}",
        mismatches.len(),
        &mismatches[..mismatches.len().min(10)]
    );
}

#[test]
fn adjust_matches_quantlib() {
    let rows = golden_rows("bdc_adjust.csv");
    let mut bad = Vec::new();
    for r in &rows {
        let got = calendar(&r[0]).adjust(date(&r[2]), convention(&r[1])).map(|d| d.serial());
        if !same(&r[3], &got) {
            bad.push(format!("{} {} {}: QuantLib {}, Riemann {got:?}", r[0], r[1], date(&r[2]), r[3]));
        }
    }
    check("adjust", bad, rows.len());
}

#[test]
fn month_start_and_end_match_quantlib() {
    let rows = golden_rows("bdc_month.csv");
    let mut bad = Vec::new();
    for r in &rows {
        let cal = calendar(&r[0]);
        let d = date(&r[1]);
        let ok = same(&r[2], &cal.end_of_month(d).map(|x| x.serial()))
            && same(&r[3], &cal.is_end_of_month(d).map(|b| b as i32))
            && same(&r[4], &cal.start_of_month(d).map(|x| x.serial()))
            && same(&r[5], &cal.is_start_of_month(d).map(|b| b as i32));
        if !ok {
            bad.push(format!("{} {d}: QuantLib {:?}", r[0], &r[2..]));
        }
    }
    check("month", bad, rows.len());
}

#[test]
fn advance_matches_quantlib() {
    let rows = golden_rows("bdc_advance.csv");
    let mut bad = Vec::new();
    for r in &rows {
        let period = Period::new(r[2].parse().unwrap(), unit(&r[3]));
        let got = calendar(&r[0])
            .advance(date(&r[1]), period, convention(&r[4]), r[5] == "1")
            .map(|d| d.serial());
        if !same(&r[6], &got) {
            bad.push(format!("{} {} {:?} {} eom={}: QuantLib {}, Riemann {got:?}", r[0], date(&r[1]), period, r[4], r[5], r[6]));
        }
    }
    check("advance", bad, rows.len());
}

#[test]
fn business_days_between_matches_quantlib() {
    let rows = golden_rows("bdc_between.csv");
    let mut bad = Vec::new();
    for r in &rows {
        let got = calendar(&r[0]).business_days_between(date(&r[1]), date(&r[2]), r[3] == "1", r[4] == "1").unwrap();
        if r[5] != got.to_string() {
            bad.push(format!("{} {} -> {} first={} last={}: QuantLib {}, Riemann {got}", r[0], date(&r[1]), date(&r[2]), r[3], r[4], r[5]));
        }
    }
    check("between", bad, rows.len());
}
