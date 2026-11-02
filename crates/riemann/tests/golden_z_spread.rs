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
    let calendar = HolidayCalendar::from(names::calendar(&r[0]).unwrap());
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
