//! Compares every calendar and market with QuantLib C++ v1.42 built from source
//! (`tools/golden/p1_all_calendars.cpp`), on all 109,208 valid dates, region by region: business
//! days, errors by cause, weekend days and names. Agreement with QuantLib says nothing about
//! actual market holidays.

mod common;

use common::calendar_by_key as calendar;
use riemann::time::{Calendar, Date, HolidayCalendar, RussiaMarket, Weekday};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn golden_rows(file: &str) -> Vec<Vec<String>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/golden")
        .join(file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}; run tools/golden/p1_all_calendars.sh",
            path.display()
        )
    });
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').map(|s| s.trim().to_string()).collect())
        .collect()
}

const WEEKDAYS: [Weekday; 7] = [
    Weekday::Sunday,
    Weekday::Monday,
    Weekday::Tuesday,
    Weekday::Wednesday,
    Weekday::Thursday,
    Weekday::Friday,
    Weekday::Saturday,
];

/// The same cause: MOEX has no data, or the rules looked at a date outside the valid range.
fn same_cause(quantlib: &str, riemann: &riemann::Error) -> bool {
    let text = riemann.to_string();
    if quantlib.starts_with("MOEX calendar") {
        text.contains("MOEX calendar") && text.contains("does not exist")
    } else if quantlib.contains("outside allowed range") {
        matches!(riemann, riemann::Error::SerialOutOfRange { .. })
    } else {
        false
    }
}

fn check_region(region: &str) {
    let markets: Vec<Vec<String>> = golden_rows("calendar_markets.csv")
        .into_iter()
        .filter(|r| r[1] == region)
        .collect();
    assert!(!markets.is_empty(), "no calendars for {region}");

    let mut exceptions: BTreeMap<String, BTreeMap<i32, char>> = BTreeMap::new();
    for r in golden_rows(&format!("calendar_all_{region}.csv")) {
        exceptions
            .entry(r[0].clone())
            .or_default()
            .insert(r[1].parse().unwrap(), r[2].chars().next().unwrap());
    }
    let mut errors: BTreeMap<String, BTreeMap<i32, String>> = BTreeMap::new();
    for r in golden_rows("calendar_all_errors.csv") {
        let (first, last): (i32, i32) = (r[1].parse().unwrap(), r[2].parse().unwrap());
        let entry = errors.entry(r[0].clone()).or_default();
        for s in first..=last {
            entry.insert(s, r[3].clone());
        }
    }

    let empty_exceptions = BTreeMap::new();
    let empty_errors = BTreeMap::new();
    let mut failures = Vec::new();
    for market in &markets {
        let (key, name) = (&market[0], &market[4]);
        let cal = HolidayCalendar::from(calendar(key));
        // the golden file replaces commas in names, because they separate its columns
        if &cal.calendar.name().replace(',', ";") != name {
            failures.push(format!(
                "{key}: name {:?}, QuantLib {name:?}",
                cal.calendar.name()
            ));
        }
        let weekend: BTreeSet<u32> = WEEKDAYS
            .iter()
            .filter(|w| cal.is_weekend(**w))
            .map(|w| w.number())
            .collect();
        let expected_weekend: BTreeSet<u32> = market[5]
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        if weekend != expected_weekend {
            failures.push(format!(
                "{key}: weekend {weekend:?}, QuantLib {expected_weekend:?}"
            ));
        }

        let marks = exceptions.get(key).unwrap_or(&empty_exceptions);
        let failing = errors.get(key).unwrap_or(&empty_errors);
        let mut mismatches = Vec::new();
        for s in Date::MIN_SERIAL..=Date::MAX_SERIAL {
            let d = Date::from_serial(s as i64).unwrap();
            let got = cal.is_business_day(d);
            match (failing.get(&s), got) {
                (Some(message), Err(e)) => {
                    if !same_cause(message, &e) {
                        mismatches.push(format!("{d}: QuantLib `{message}`, Riemann `{e}`"));
                    }
                }
                (Some(message), Ok(b)) => {
                    mismatches.push(format!("{d}: QuantLib `{message}`, Riemann {b}"))
                }
                (None, Err(e)) => {
                    mismatches.push(format!("{d}: QuantLib has a value, Riemann `{e}`"))
                }
                (None, Ok(business)) => {
                    let on_weekend = cal.is_weekend(d.weekday());
                    let expected = match marks.get(&s) {
                        Some('H') => false,
                        Some('W') => true,
                        _ => !on_weekend,
                    };
                    if business != expected {
                        mismatches.push(format!(
                            "{d} ({:?}): QuantLib business={expected}, Riemann {business}",
                            d.weekday()
                        ));
                    }
                }
            }
        }
        if !mismatches.is_empty() {
            failures.push(format!(
                "{key}: {} of {} dates differ; first: {:?}",
                mismatches.len(),
                Date::MAX_SERIAL - Date::MIN_SERIAL + 1,
                &mismatches[..mismatches.len().min(5)]
            ));
        }
        eprintln!(
            "{key:36} holidays and exceptions {:6}  errors {:6}",
            marks.len(),
            failing.len()
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn common_calendars_match_quantlib_cpp_on_every_date() {
    check_region("common");
}

#[test]
fn americas_match_quantlib_cpp_on_every_date() {
    check_region("americas");
}

#[test]
fn europe_matches_quantlib_cpp_on_every_date() {
    check_region("europe");
}

#[test]
fn middle_east_and_africa_match_quantlib_cpp_on_every_date() {
    check_region("middle_east_africa");
}

#[test]
fn asia_pacific_matches_quantlib_cpp_on_every_date() {
    check_region("asia_pacific");
}

#[test]
fn exceptions_apply_to_new_calendars_too() {
    // QuantLib's addHoliday and removeHoliday do not depend on the calendar; these dates are a
    // business day and a holiday of the rules, and both operations return new values.
    let turkey = HolidayCalendar::from(Calendar::Turkey);
    let (business, holiday) = (
        Date::from_ymd(2040, 3, 5).unwrap(),
        Date::from_ymd(2040, 8, 30).unwrap(),
    );
    assert!(turkey.is_business_day(business).unwrap());
    assert!(turkey
        .with_holiday(business)
        .unwrap()
        .is_holiday(business)
        .unwrap());
    assert!(turkey.is_holiday(holiday).unwrap());
    assert!(turkey
        .without_holiday(holiday)
        .unwrap()
        .is_business_day(holiday)
        .unwrap());
    // A calendar that has no data on a date keeps failing unless the date is an exception.
    let moex = HolidayCalendar::from(Calendar::Russia(RussiaMarket::Moex));
    assert!(moex
        .is_business_day(Date::from_ymd(2011, 3, 1).unwrap())
        .is_err());
}
