//! Compares dates with golden values from the QuantLib R binding
//! (`tools/golden/p1_dates.R`). Dates must match exactly.

use riemann::{Date, Period, TimeUnit};
use std::path::PathBuf;

fn golden_rows(file: &str) -> Vec<Vec<String>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden").join(file);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}; run tools/golden/p1_dates.R", path.display()));
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').map(|s| s.trim().trim_matches('"').to_string()).collect())
        .collect()
}

#[test]
fn every_date_matches_quantlib() {
    let rows = golden_rows("dates_all.csv");
    assert_eq!(rows.len() as i32, Date::MAX_SERIAL - Date::MIN_SERIAL + 1);
    let mut mismatches = Vec::new();
    for r in &rows {
        let n: Vec<i64> = r.iter().map(|s| s.parse().unwrap()).collect();
        let (serial, year, month, day, weekday, leap, eom, eom_serial) =
            (n[0], n[1] as i32, n[2] as u32, n[3] as u32, n[4] as u32, n[5] == 1, n[6] == 1, n[7]);
        let d = Date::from_serial(serial).unwrap();
        let ok = d.ymd() == (year, month, day)
            && Date::from_ymd(year, month, day).unwrap() == d
            && d.weekday().number() == weekday
            && riemann::time::is_leap(year) == leap
            && d.is_end_of_month() == eom
            && d.end_of_month().serial() as i64 == eom_serial;
        if !ok && mismatches.len() < 10 {
            mismatches.push(format!("serial {serial}: QuantLib {r:?}, Riemann {d} wd {}", d.weekday().number()));
        }
    }
    assert!(mismatches.is_empty(), "mismatches:\n{}", mismatches.join("\n"));
}

#[test]
fn date_plus_period_matches_quantlib() {
    let rows = golden_rows("dates_period.csv");
    assert!(!rows.is_empty());
    let mut mismatches = Vec::new();
    for r in &rows {
        let serial: i64 = r[0].parse().unwrap();
        let n: i32 = r[1].parse().unwrap();
        let unit = match r[2].as_str() {
            "Days" => TimeUnit::Days,
            "Weeks" => TimeUnit::Weeks,
            "Months" => TimeUnit::Months,
            "Years" => TimeUnit::Years,
            other => panic!("unknown unit {other}"),
        };
        let got = Date::from_serial(serial).unwrap().add_period(Period::new(n, unit));
        let ok = match (r[3].as_str(), &got) {
            ("ERROR", Err(_)) => true,
            ("ERROR", Ok(_)) => false,
            (s, Ok(d)) => s.parse::<i32>().unwrap() == d.serial(),
            (_, Err(_)) => false,
        };
        if !ok && mismatches.len() < 10 {
            mismatches.push(format!("{serial} + {n} {:?}: QuantLib {}, Riemann {got:?}", unit, r[3]));
        }
    }
    assert!(mismatches.is_empty(), "mismatches:\n{}", mismatches.join("\n"));
}
