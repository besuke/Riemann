//! Bond values with a zero spread and the spread of a price, against the QuantLib R package
//! (`tools/golden/p11_z_spread.R`). Values within a relative 1e-11; spreads within 1e-9.

mod common;

use common::{date, day_count, golden_rows, Stats};
use riemann::curves::YieldCurve;
use riemann::instruments::{FixedRateBond, FixedRateBondSpec, PriceType};
use riemann::interest_rate::Compounding;
use riemann::names;
use riemann::time::{BusinessDayConvention, DateGenerationRule, HolidayCalendar, Period, ScheduleSpec, TimeUnit};

fn bond(r: &[String]) -> FixedRateBond {
    let calendar = HolidayCalendar::from(names::parse_calendar(&r[0]).unwrap());
    FixedRateBond::new(FixedRateBondSpec {
        settlement_days: r[2].parse().unwrap(),
        face_amount: 100.0,
        schedule: ScheduleSpec {
            effective_date: date(&r[3]),
            termination_date: date(&r[4]),
            tenor: Period::new(6, TimeUnit::Months),
            calendar,
            convention: BusinessDayConvention::Unadjusted,
            termination_convention: BusinessDayConvention::Unadjusted,
            rule: DateGenerationRule::Backward,
            end_of_month: false,
            first_date: None,
            next_to_last_date: None,
        },
        coupon_rate: r[5].parse().unwrap(),
        day_count: day_count(&r[1]),
        payment_convention: BusinessDayConvention::Following,
        redemption: 100.0,
        issue_date: Some(date(&r[3])),
        payment_calendar: None,
    })
    .unwrap()
}

fn compounding(name: &str) -> Compounding {
    match name {
        "Simple" => Compounding::Simple,
        "Compounded" => Compounding::Compounded,
        "Continuous" => Compounding::Continuous,
        other => panic!("unknown compounding {other}"),
    }
}

/// The part of a QuantLib message without its numbers and dates.
fn message_kind(message: &str) -> &str {
    if message.starts_with("non tradable") {
        return "non tradable at"; // QuantLib writes the date in words
    }
    let end = message.find(|c: char| c.is_ascii_digit() || c == '(').unwrap_or(message.len());
    message[..end].trim_end()
}

/// The same search takes the same steps: spreads must agree whatever the accuracy, and failures
/// must happen in the same cases and for the same reason.
#[test]
fn boundary_cases_of_the_search_agree_with_quantlib() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/z_spread_edges.csv");
    let text = std::fs::read_to_string(path).unwrap();
    let header = text.lines().next().unwrap();
    let evaluation = date(header.split("evaluation ").nth(1).unwrap().split(';').next().unwrap());
    let (all_dates, all_discounts): (Vec<_>, Vec<f64>) = header
        .split("nodes ")
        .nth(1)
        .unwrap()
        .split(' ')
        .map(|n| {
            let (d, v) = n.split_once(':').unwrap();
            (date(d), v.parse::<f64>().unwrap())
        })
        .unzip();
    let bond_row: Vec<String> = ["japan", "actual_365_fixed", "2", "45371", "47927", "0.015"].iter().map(|s| s.to_string()).collect();
    assert_eq!(date(&bond_row[3]), riemann::Date::from_ymd(2024, 3, 20).unwrap());
    assert_eq!(date(&bond_row[4]), riemann::Date::from_ymd(2031, 3, 20).unwrap());
    let b = bond(&bond_row);
    let rows = golden_rows("z_spread_edges.csv");
    assert_eq!(rows.len(), 25);
    let (mut solved, mut failed, mut largest) = (0, 0, 0.0_f64);
    for r in &rows {
        // cases on a short curve end two years ahead
        let keep = if r[0].contains("extrapolation") { 5 } else { all_dates.len() };
        let curve = YieldCurve::discount_factors(all_dates[..keep].to_vec(), all_discounts[..keep].to_vec(), day_count("actual_365_fixed"))
            .unwrap()
            .with_extrapolation(r[9] == "1");
        let number = |i: usize| r[i].parse::<f64>().unwrap();
        let price_type = if r[2] == "Clean" { PriceType::Clean } else { PriceType::Dirty };
        let result = b.z_spread_from_price(
            &curve,
            number(1),
            price_type,
            compounding(&r[3]),
            r[4].parse().unwrap(),
            date(&r[5]),
            number(6),
            r[7].parse().unwrap(),
            number(8),
        );
        match (r[10].as_str(), result) {
            ("ok", Ok(z)) => {
                let difference = (z - number(11)).abs();
                largest = largest.max(difference);
                assert!(difference < 1e-11, "{}: {z} against {}", r[0], r[11]);
                solved += 1;
            }
            ("failed", Err(e)) => {
                let (ours, kind) = (e.to_string(), message_kind(&r[12]).to_string());
                assert!(ours.contains(&kind), "{}: '{ours}' against '{}'", r[0], r[12]);
                failed += 1;
            }
            (status, other) => panic!("{}: QuantLib {status} ({}) but ours {other:?}", r[0], r[12]),
        }
    }
    println!("{solved} solved, largest difference {largest:.3e}; {failed} failed alike; evaluation {evaluation:?}");
}

#[test]
fn zero_spread_values_and_spreads_agree_with_quantlib() {
    let rows = golden_rows("z_spreads.csv");
    assert_eq!(rows.len(), 200);
    let mut stats = Stats::default();
    let mut largest_spread_difference = 0.0_f64;
    for r in &rows {
        let b = bond(r);
        let evaluation = date(&r[6]);
        let (dates, discounts): (Vec<_>, Vec<f64>) = r[7]
            .split(' ')
            .map(|n| {
                let (d, v) = n.split_once(':').unwrap();
                (date(d), v.parse::<f64>().unwrap())
            })
            .unzip();
        let curve = YieldCurve::discount_factors(dates, discounts, day_count(&r[1])).unwrap().with_extrapolation(true);
        let (comp, freq) = (compounding(&r[8]), r[9].parse::<u32>().unwrap());
        let number = |i: usize| r[i].parse::<f64>().unwrap();
        let v = b.value_with_zero_spread(&curve, number(10), comp, freq, evaluation).unwrap();
        for (ours, column) in [(v.npv, 11), (v.settlement_value, 12), (v.dirty_price, 13), (v.clean_price, 14)] {
            assert!(stats.close(&r[column], ours, 1e-11), "value {ours} against {} in {r:?}", r[column]);
        }
        let price_type = if r[16] == "Clean" { PriceType::Clean } else { PriceType::Dirty };
        let settlement = b.settlement_date(evaluation).unwrap();
        let z = b.z_spread_from_price(&curve, number(15), price_type, comp, freq, settlement, 1e-12, 1000, 0.0).unwrap();
        let difference = (z - number(17)).abs();
        largest_spread_difference = largest_spread_difference.max(difference);
        assert!(difference < 1e-9, "spread {z} against {} in {r:?}", r[17]);
    }
    println!(
        "values: {} exact, {} inexact, largest relative {:.3e}; largest spread difference {largest_spread_difference:.3e}",
        stats.exact, stats.inexact, stats.max_rel
    );
}
