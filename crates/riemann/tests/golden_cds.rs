//! Credit default swaps against the QuantLib R package (tools/golden/p8_cds.R). All inputs are
//! synthetic.

mod common;

use common::golden_rows;
use riemann::cds::{
    bootstrap_hazard, cds_value, Cds, CdsCurveSpec, CdsEngine, CdsSpec, HazardCurve, ProtectionSide, SpreadQuote,
};
use riemann::curves::YieldCurve;
use riemann::interest_rate::{Compounding, InterestRate};
use riemann::time::{
    Actual365Kind, BusinessDayConvention, Calendar, Date, DateGenerationRule, DayCount, HolidayCalendar, Period, ScheduleSpec, TimeUnit,
};

fn number(s: &str) -> f64 {
    s.parse().unwrap()
}

fn act365() -> DayCount {
    DayCount::Actual365Fixed(Actual365Kind::Standard)
}

fn act360() -> DayCount {
    DayCount::Actual360 { include_last_day: false }
}

fn today() -> Date {
    Date::from_ymd(2026, 11, 2).unwrap()
}

fn flat_rate(rate: f64) -> YieldCurve {
    YieldCurve::flat(today(), InterestRate::new(rate, act365(), Compounding::Continuous, 0).unwrap())
}

fn engine_of(name: &str) -> CdsEngine {
    match name {
        "midpoint" => CdsEngine::MidPoint,
        "isda" => CdsEngine::Isda,
        other => panic!("{other}"),
    }
}

#[test]
fn values_against_quantlib() {
    let rows = golden_rows("cds_values.csv");
    assert_eq!(rows.len(), 300);
    let mut worst = 0.0f64;
    for r in &rows {
        let n = |i: usize| number(&r[i]);
        let rule = match r[2].as_str() {
            "Forward" => DateGenerationRule::Forward,
            "Backward" => DateGenerationRule::Backward,
            "CDS2015" => DateGenerationRule::Cds2015,
            other => panic!("{other}"),
        };
        let spec = CdsSpec {
            side: if r[1] == "buyer" { ProtectionSide::Buyer } else { ProtectionSide::Seller },
            notional: n(5),
            running_spread: n(6),
            schedule: ScheduleSpec {
                effective_date: common::date(&r[3]),
                termination_date: common::date(&r[4]),
                tenor: Period::new(3, TimeUnit::Months),
                calendar: HolidayCalendar::from(Calendar::WeekendsOnly),
                convention: BusinessDayConvention::Following,
                termination_convention: BusinessDayConvention::Unadjusted,
                rule,
                end_of_month: false,
                first_date: None,
                next_to_last_date: None,
            },
            payment_convention: BusinessDayConvention::Following,
            day_count: act360(),
            last_period_day_count: None,
            upfront: Some(n(7)),
            protection_start: None,
            trade_date: None,
            cash_settlement_days: 3,
            rebates_accrual: true,
        };
        let cds = Cds::new(spec).unwrap();
        let hazard = HazardCurve::flat(today(), act365(), n(9)).unwrap();
        let v = cds_value(engine_of(&r[0]), &cds, &hazard, n(10), &flat_rate(n(8)), today()).unwrap();
        let mut check = |name: &str, got: f64, expected: f64, relative: f64| {
            let diff = (got - expected).abs() / (1.0 + expected.abs());
            worst = worst.max(diff);
            assert!(diff < relative, "{name} ({} {}): {got} against {expected}\n{r:?}", r[0], r[2]);
        };
        check("npv", v.npv, n(11), 1e-9);
        check("coupon leg", v.coupon_leg_npv, n(12), 1e-9);
        check("default leg", v.default_leg_npv, n(13), 1e-9);
        check("fair spread", v.fair_spread, n(14), 1e-9);
        if let Some(fair_upfront) = v.fair_upfront {
            if !n(15).is_nan() {
                check("fair upfront", fair_upfront, n(15), 1e-9);
            }
        }
    }
    println!("CDS values: worst difference {worst:e}");
}

#[test]
fn bootstrapped_hazards_against_quantlib() {
    let rows = golden_rows("cds_hazards.csv");
    assert_eq!(rows.len(), 240);
    let mut worst = 0.0f64;
    for case in rows.chunks(6) {
        let first = &case[0];
        let engine = engine_of(&first[1]);
        let spec = CdsCurveSpec {
            evaluation_date: today(),
            settlement_days: 0,
            calendar: HolidayCalendar::from(Calendar::WeekendsOnly),
            tenor: Period::new(3, TimeUnit::Months),
            payment_convention: BusinessDayConvention::Following,
            rule: DateGenerationRule::Backward,
            day_count: act360(),
            last_period_day_count: None,
            rebates_accrual: true,
            recovery: number(&first[3]),
            engine,
            curve_day_count: act365(),
            accuracy: 1e-12,
            quotes: case
                .iter()
                .map(|r| SpreadQuote {
                    maturity: today().add_period(Period::new(number(&r[4]) as i32, TimeUnit::Years)).unwrap(),
                    spread: number(&r[5]),
                })
                .collect(),
        };
        let built = bootstrap_hazard(&spec, &flat_rate(number(&first[2]))).unwrap();
        for (i, r) in case.iter().enumerate() {
            assert_eq!(built.hazard.dates()[i].serial() as i64, number(&r[6]) as i64, "pillar {i} of {r:?}");
            let survival = built.hazard.survival(built.hazard.dates()[i]).unwrap();
            let diff = (survival - number(&r[7])).abs();
            worst = worst.max(diff);
            assert!(diff < 1e-9, "survival {i}: {survival} against {} ({r:?})", number(&r[7]));
        }
    }
    println!("CDS survival at the nodes: worst absolute difference {worst:e}");
}
