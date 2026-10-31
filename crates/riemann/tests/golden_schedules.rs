//! Compares schedule generation with golden values from the QuantLib R binding
//! (`tools/golden/p1_schedules.R`). Dates and regularity flags must match exactly.

use riemann::time::{
    generate_schedule, BusinessDayConvention, Calendar, DateGenerationRule, HolidayCalendar, Period,
    ScheduleSpec, TimeUnit, UsMarket,
};
use riemann::Date;
use std::path::PathBuf;

fn golden_rows(file: &str) -> Vec<Vec<String>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden").join(file);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}; run tools/golden/p1_schedules.R", path.display()));
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
        "weekends_only" => Calendar::WeekendsOnly,
        "null_calendar" => Calendar::NullCalendar,
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
        other => panic!("unknown convention {other}"),
    }
}

fn date(s: &str) -> Date {
    Date::from_serial(s.parse().unwrap()).unwrap()
}

fn optional_date(s: &str) -> Option<Date> {
    if s == "0" {
        None
    } else {
        Some(date(s))
    }
}

#[test]
fn schedules_match_quantlib() {
    let rows = golden_rows("schedules.csv");
    assert!(!rows.is_empty());
    let mut bad = Vec::new();
    for r in &rows {
        let unit = match r[3].as_str() {
            "Days" => TimeUnit::Days,
            "Weeks" => TimeUnit::Weeks,
            "Months" => TimeUnit::Months,
            "Years" => TimeUnit::Years,
            other => panic!("unknown unit {other}"),
        };
        let rule = match r[7].as_str() {
            "Backward" => DateGenerationRule::Backward,
            "Forward" => DateGenerationRule::Forward,
            "Zero" => DateGenerationRule::Zero,
            other => panic!("unknown rule {other}"),
        };
        let spec = ScheduleSpec {
            effective_date: date(&r[0]),
            termination_date: date(&r[1]),
            tenor: Period::new(r[2].parse().unwrap(), unit),
            calendar: calendar(&r[4]),
            convention: convention(&r[5]),
            termination_convention: convention(&r[6]),
            rule,
            end_of_month: r[8] == "1",
            first_date: optional_date(&r[9]),
            next_to_last_date: optional_date(&r[10]),
        };
        let got = generate_schedule(&spec);
        let ok = match &got {
            Ok(s) => {
                let dates: Vec<String> = s.dates.iter().map(|d| d.serial().to_string()).collect();
                let regular: Vec<String> = s.is_regular.iter().map(|&b| (b as i32).to_string()).collect();
                r[11] == dates.join(" ") && r[12] == regular.join(" ")
            }
            Err(_) => r[11] == "ERROR",
        };
        if !ok {
            bad.push(format!(
                "{} -> {} {:?} {} {}/{} {} eom={} first={:?} ntl={:?}\n  QuantLib: {} | {}\n  Riemann:  {:?}",
                spec.effective_date, spec.termination_date, spec.tenor, r[4], r[5], r[6], r[7], r[8],
                spec.first_date.map(|d| d.to_string()), spec.next_to_last_date.map(|d| d.to_string()),
                r[11], r[12],
                got.as_ref().map(|s| (s.dates.iter().map(|d| d.serial()).collect::<Vec<_>>(), s.is_regular.clone()))
            ));
        }
    }
    assert!(
        bad.is_empty(),
        "{} of {} schedules differ; first:\n{}",
        bad.len(),
        rows.len(),
        bad[..bad.len().min(5)].join("\n")
    );
}
