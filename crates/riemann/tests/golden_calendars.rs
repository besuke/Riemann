//! Compares holiday calendars with golden values from the QuantLib R binding
//! (`tools/golden/p1_calendars.R`). Every date in the valid range must match exactly.

use riemann::time::{Calendar, Date, HolidayCalendar, JointRule, UkMarket, UsMarket, Weekday};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn golden_text(file: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden").join(file);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}; run tools/golden/p1_calendars.R", path.display()))
}

fn data_rows(text: &str) -> Vec<Vec<String>> {
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').map(|s| s.trim().to_string()).collect())
        .collect()
}

fn calendar(name: &str) -> Calendar {
    let jp_us = || vec![Calendar::Japan, Calendar::UnitedStates(UsMarket::Sofr)];
    match name {
        "japan" => Calendar::Japan,
        "target" => Calendar::Target,
        "us_settlement" => Calendar::UnitedStates(UsMarket::Settlement),
        "us_government_bond" => Calendar::UnitedStates(UsMarket::GovernmentBond),
        "us_sofr" => Calendar::UnitedStates(UsMarket::Sofr),
        "uk_settlement" => Calendar::UnitedKingdom(UkMarket::Settlement),
        "weekends_only" => Calendar::WeekendsOnly,
        "null_calendar" => Calendar::NullCalendar,
        "joint_japan_us_sofr_holidays" => Calendar::Joint(jp_us(), JointRule::JoinHolidays),
        "joint_japan_us_sofr_business_days" => Calendar::Joint(jp_us(), JointRule::JoinBusinessDays),
        other => panic!("unknown calendar {other}"),
    }
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

#[test]
fn holidays_match_quantlib_for_every_date() {
    let text = golden_text("calendar_holidays.csv");

    // Weekend days per calendar, from the header comment.
    let weekend_line = text.lines().find(|l| l.starts_with("# weekend:")).expect("weekend header");
    let weekends: BTreeMap<String, BTreeSet<u32>> = weekend_line["# weekend:".len()..]
        .split(';')
        .map(|kv| {
            let (k, v) = kv.trim().split_once('=').expect("name=days");
            (k.to_string(), v.split_whitespace().map(|n| n.parse().unwrap()).collect())
        })
        .collect();

    let mut expected: BTreeMap<String, BTreeSet<i32>> = weekends.keys().map(|k| (k.clone(), BTreeSet::new())).collect();
    for r in data_rows(&text) {
        expected.get_mut(&r[0]).expect("calendar in header").insert(r[1].parse().unwrap());
    }

    for (name, holidays) in &expected {
        let cal = HolidayCalendar::from(calendar(name));
        let weekend: BTreeSet<u32> = WEEKDAYS.iter().filter(|w| cal.is_weekend(**w)).map(|w| w.number()).collect();
        assert_eq!(&weekend, &weekends[name], "weekend days of {name}");

        let mut mismatches = Vec::new();
        for s in Date::MIN_SERIAL..=Date::MAX_SERIAL {
            let d = Date::from_serial(s as i64).unwrap();
            let riemann = cal.is_holiday(d) && !cal.is_weekend(d.weekday());
            let quantlib = holidays.contains(&s);
            let weekend_ok = !cal.is_weekend(d.weekday()) || cal.is_holiday(d);
            if riemann != quantlib || !weekend_ok {
                mismatches.push(format!("{d} ({:?}): QuantLib holiday={quantlib}, Riemann holiday={riemann}", d.weekday()));
            }
        }
        assert!(
            mismatches.is_empty(),
            "{name}: {} mismatches, first: {:?}",
            mismatches.len(),
            &mismatches[..mismatches.len().min(10)]
        );
    }
}

#[test]
fn holiday_adjustments_match_quantlib() {
    let rows = data_rows(&golden_text("calendar_adjustments.csv"));
    let mut cal = HolidayCalendar::from(Calendar::Japan);
    let mut step = 0;
    let mut mismatches = Vec::new();
    for r in &rows {
        let k: i32 = r[0].parse().unwrap();
        let date = Date::from_serial(r[2].parse().unwrap()).unwrap();
        if k != step {
            cal = match r[1].as_str() {
                "add" => cal.with_holiday(date),
                "remove" => cal.without_holiday(date),
                other => panic!("unknown op {other}"),
            };
            step = k;
        }
        let probe = Date::from_serial(r[3].parse().unwrap()).unwrap();
        let expected = r[4] == "1";
        if cal.is_business_day(probe) != expected {
            mismatches.push(format!("step {k} {} {date}: probe {probe} QuantLib business={expected}", r[1]));
        }
    }
    assert!(mismatches.is_empty(), "mismatches:\n{}", mismatches.join("\n"));
}
