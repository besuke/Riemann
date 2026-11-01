//! Asset swaps against GaussQuant and futures basis against QuantLib's accrued amounts
//! (tools/golden/p7_asset_swaps.R). All inputs are synthetic. The curves are the golden nodes, so
//! the comparison does not depend on the bootstrap.

mod common;

use common::golden_rows;
use riemann::asset_swap::{
    asset_swap_quote_sensitivity, future_basis, ois_asset_swap, yen_asset_swap, FutureBasisTerms,
    OisAssetSwapTerms, YenBasisTerms,
};
use riemann::curves::{LogDiscountInterpolation, YieldCurve};
use riemann::instruments::{FixedRateBond, FixedRateBondSpec};
use riemann::names;
use riemann::ois::{build_ois_curve, estr, sofr, sonia, tonar, OisCurveInstrument, OisCurveSpec, OisInterpolation, OvernightIndex};
use riemann::time::{BusinessDayConvention as B, Date, DateGenerationRule, HolidayCalendar, Period, ScheduleSpec, TimeUnit};

fn number(s: &str) -> f64 {
    s.parse().unwrap()
}

fn date(y: i32, m: u32, d: u32) -> Date {
    Date::from_ymd(y, m, d).unwrap()
}

fn curve_of(currency: &str) -> YieldCurve {
    let rows: Vec<_> = golden_rows("asw_nodes.csv").into_iter().filter(|r| r[0] == currency).collect();
    assert!(rows.len() > 5);
    YieldCurve::interpolated_discounts(
        rows.iter().map(|r| common::date(&r[1])).collect(),
        rows.iter().map(|r| number(&r[2])).collect(),
        names::day_count("actual_365_fixed").unwrap(),
        LogDiscountInterpolation::MonotonicLogCubic,
    )
    .unwrap()
}

fn index_of(currency: &str) -> OvernightIndex {
    match currency {
        "JPY" => tonar(),
        "USD" => sofr(),
        "EUR" => estr(),
        "GBP" => sonia(),
        other => panic!("{other}"),
    }
}

fn bond_of(currency: &str, coupon: f64, years: i32) -> FixedRateBond {
    let (calendar, day_count) = match currency {
        "JPY" => ("japan", "actual_365_fixed"),
        "USD" => ("us_sofr", "thirty_360_bond_basis"),
        "EUR" => ("target", "thirty_360_bond_basis"),
        "GBP" => ("uk_exchange", "thirty_360_bond_basis"),
        other => panic!("{other}"),
    };
    FixedRateBond::new(FixedRateBondSpec {
        settlement_days: 2,
        face_amount: 100.0,
        schedule: ScheduleSpec {
            effective_date: date(2026, 9, 30),
            termination_date: date(2026 + years, 9, 30),
            tenor: Period::new(6, TimeUnit::Months),
            calendar: HolidayCalendar::from(names::calendar(calendar).unwrap()),
            convention: B::Unadjusted,
            termination_convention: B::Unadjusted,
            rule: DateGenerationRule::Backward,
            end_of_month: false,
            first_date: None,
            next_to_last_date: None,
        },
        coupon_rate: coupon,
        day_count: names::day_count(day_count).unwrap(),
        payment_convention: B::Following,
        redemption: 100.0,
        issue_date: None,
        payment_calendar: None,
    })
    .unwrap()
}

fn close(name: &str, got: f64, expected: f64, tolerance: f64, worst: &mut f64) {
    let diff = (got - expected).abs();
    *worst = worst.max(diff);
    assert!(diff <= tolerance, "{name}: {got} against {expected} (difference {diff})");
}

#[test]
fn ois_asset_swaps_against_gaussquant() {
    let evaluation = date(2026, 10, 6);
    let rows = golden_rows("asw_bonds.csv");
    assert_eq!(rows.len(), 80);
    let mut worst = 0.0;
    for r in &rows {
        let n = |i: usize| number(&r[i]);
        let bond = bond_of(&r[0], n(1), n(2) as i32);
        let terms = OisAssetSwapTerms {
            clean_price: n(3),
            payment_frequency: n(4) as u32,
            payment_lag: n(5) as u32,
            spread: n(6) / 1e4,
        };
        let curve = curve_of(&r[0]);
        let v = ois_asset_swap(&bond, &curve, &index_of(&r[0]), &terms, evaluation).unwrap();
        assert_eq!(v.effective_date.serial() as i64, n(16) as i64, "effective date");
        assert_eq!(v.maturity_date.serial() as i64, n(17) as i64, "maturity date");
        close("accrued", v.accrued, n(7), 1e-11, &mut worst);
        close("fair spread bp", v.fair_spread * 1e4, n(8), 1e-8, &mut worst);
        close("annuity", v.spread_annuity, n(9), 1e-10, &mut worst);
        close("npv", v.npv, n(10), 1e-9, &mut worst);
        close("bond leg", v.bond_leg_pv, n(11), 1e-9, &mut worst);
        close("overnight leg", v.overnight_leg_pv, n(12), 1e-9, &mut worst);
        close("principal", v.principal_pv, n(13), 1e-10, &mut worst);
        close("upfront", v.upfront_pv, n(14), 1e-10, &mut worst);
        close("fair price", v.fair_clean_price, n(15), 1e-9, &mut worst);
    }
    println!("OIS asset swaps: worst absolute difference {worst:e}");
}

#[test]
fn yen_asset_swaps_against_gaussquant() {
    let evaluation = date(2026, 10, 6);
    let rows = golden_rows("asw_jpy.csv");
    assert_eq!(rows.len(), 45);
    let yen = curve_of("JPY");
    let mut worst = 0.0;
    for r in &rows {
        let n = |i: usize| number(&r[i]);
        let bond = bond_of(&r[0], n(1), n(2) as i32);
        let terms = OisAssetSwapTerms { clean_price: n(3), payment_frequency: 2, payment_lag: 0, spread: 0.0 };
        let basis = YenBasisTerms { fx_spot: n(4), basis: n(5) / 1e4, spread: n(6) / 1e4 };
        let v = yen_asset_swap(&bond, &curve_of(&r[0]), &index_of(&r[0]), &terms, &yen, &tonar(), &terms, &basis, evaluation).unwrap();
        close("native bp", v.native_fair_spread * 1e4, n(7), 1e-8, &mut worst);
        // the root is searched to 1e-12 by both, on different brackets
        close("yen bp", v.yen_fair_spread * 1e4, n(8), 1e-5, &mut worst);
        close("simple sum", v.simple_sum * 1e4, n(9), 1e-8, &mut worst);
        close("z bp", v.calibrated_discount_spread * 1e4, n(15), 1e-5, &mut worst);
        let scale = n(14);
        close("yen npv per annuity", v.npv / scale, n(10) / scale, 1e-9, &mut worst);
        close("hedged per annuity", v.hedged_bond_pv / scale, n(11) / scale, 1e-9, &mut worst);
        close("base leg per annuity", v.yen_base_leg_pv / scale, n(12) / scale, 1e-9, &mut worst);
        close("upfront per annuity", v.upfront_pv / scale, n(13) / scale, 1e-9, &mut worst);
        close("annuity", v.spread_annuity, scale, 1e-7, &mut worst);
    }
    println!("Yen asset swaps: worst absolute difference {worst:e}");
}

#[test]
fn future_basis_against_quantlib_accrued_amounts() {
    let rows = golden_rows("asw_futures.csv");
    assert_eq!(rows.len(), 60);
    let mut worst = 0.0;
    for r in &rows {
        let n = |i: usize| number(&r[i]);
        let bond = FixedRateBond::new(FixedRateBondSpec {
            settlement_days: 0,
            face_amount: 100.0,
            schedule: ScheduleSpec {
                effective_date: date(2026, 6, 20),
                termination_date: date(2026 + n(1) as i32, 6, 20),
                tenor: Period::new(6, TimeUnit::Months),
                calendar: HolidayCalendar::from(names::calendar("japan").unwrap()),
                convention: B::Unadjusted,
                termination_convention: B::Unadjusted,
                rule: DateGenerationRule::Backward,
                end_of_month: false,
                first_date: None,
                next_to_last_date: None,
            },
            coupon_rate: n(0),
            day_count: names::day_count("actual_365_fixed").unwrap(),
            payment_convention: B::Following,
            redemption: 100.0,
            issue_date: None,
            payment_calendar: None,
        })
        .unwrap();
        let v = future_basis(
            &bond,
            &FutureBasisTerms {
                clean_price: n(2),
                futures_price: n(3),
                conversion_factor: n(4),
                funding_rate: n(5),
                valuation_date: common::date(&r[6]),
                delivery_date: common::date(&r[7]),
            },
        )
        .unwrap();
        close("accrued start", v.accrued_start, n(8), 1e-12, &mut worst);
        close("accrued end", v.accrued_end, n(9), 1e-12, &mut worst);
        close("income", v.coupon_income, n(10), 1e-12, &mut worst);
        close("gross", v.gross_basis, n(11), 1e-11, &mut worst);
        close("carry", v.carry, n(12), 1e-11, &mut worst);
        close("net", v.net_basis, n(13), 1e-11, &mut worst);
        close("repo", v.implied_repo, n(14), 1e-12, &mut worst);
    }
    println!("Future basis: worst absolute difference {worst:e}");
}

fn spec_of(quotes: &[f64]) -> OisCurveSpec {
    let tenors = [1, 2, 3, 5, 7, 10, 15];
    OisCurveSpec {
        evaluation_date: date(2026, 10, 6),
        reference_date: date(2026, 10, 6),
        index: tonar(),
        curve_day_count: names::day_count("actual_365_fixed").unwrap(),
        instruments: tenors
            .iter()
            .zip(quotes)
            .map(|(&y, &rate)| OisCurveInstrument::Ois {
                tenor: Period::new(y, TimeUnit::Years),
                rate,
                settlement_days: 2,
                payment_frequency: 1,
                payment_lag: 0,
            })
            .collect(),
        interpolation: OisInterpolation::MonotonicLogCubic,
        accuracy: 1e-12,
    }
}

#[test]
fn quote_sensitivities_agree_with_rebuilding_the_curve() {
    let quotes = [0.006, 0.007, 0.0085, 0.011, 0.0125, 0.0145, 0.016];
    let evaluation = date(2026, 10, 6);
    let bond = bond_of("JPY", 0.015, 7);
    let terms = OisAssetSwapTerms { clean_price: 102.0, payment_frequency: 2, payment_lag: 0, spread: 0.0 };
    let ois = build_ois_curve(&spec_of(&quotes)).unwrap();
    let rows = asset_swap_quote_sensitivity(&bond, &ois, &terms, evaluation).unwrap();
    assert_eq!(rows.len(), quotes.len());
    let spread_with = |q: &[f64]| {
        let curve = build_ois_curve(&spec_of(q)).unwrap().curve;
        ois_asset_swap(&bond, &curve, &tonar(), &terms, evaluation).unwrap().fair_spread
    };
    for row in &rows {
        let h = 1e-6;
        let (mut up, mut down) = (quotes, quotes);
        up[row.instrument] += h;
        down[row.instrument] -= h;
        let numeric = (spread_with(&up) - spread_with(&down)) / (2.0 * h);
        assert!((row.d_fair_spread - numeric).abs() < 1e-5, "{}: {} {numeric}", row.instrument, row.d_fair_spread);
    }
    // a parallel rise in the quotes moves the spread by about the coupon-free part: bounded and finite
    let total: f64 = rows.iter().map(|r| r.d_fair_spread).sum();
    assert!(total.is_finite());
}
