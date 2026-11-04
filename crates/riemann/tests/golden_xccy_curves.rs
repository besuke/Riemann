//! Curves from FX swaps and cross-currency basis swaps against the QuantLib R package
//! (tools/golden/p12_xccy_curves.R). All inputs are synthetic.

mod common;

use common::golden_rows;
use riemann::curves::YieldCurve;
use riemann::interest_rate::{Compounding, InterestRate};
use riemann::ois::{create_estr_index, create_sofr_index, create_tonar_index, OvernightIndex};
use riemann::time::{Actual365Kind, BusinessDayConvention, Date, DayCount, HolidayCalendar, Period, TimeUnit};
use riemann::xccy::{bootstrap_xccy_curve, ResettingLeg, XccyCurveSpec, XccyInstrument, XccyLeg};

fn today() -> Date {
    Date::from_ymd(2026, 11, 4).unwrap()
}

fn act365() -> DayCount {
    DayCount::Actual365Fixed(Actual365Kind::Standard)
}

fn flat(rate: f64) -> YieldCurve {
    YieldCurve::flat(today(), InterestRate::new(rate, act365(), Compounding::Continuous, 0).unwrap())
}

fn index(name: &str) -> OvernightIndex {
    match name {
        "sofr" => create_sofr_index(),
        "tonar" => create_tonar_index(),
        "estr" => create_estr_index(),
        other => panic!("{other}"),
    }
}

fn tenor(s: &str) -> Period {
    let n: i32 = s[..s.len() - 1].parse().unwrap();
    Period::new(n, if s.ends_with('M') { TimeUnit::Months } else { TimeUnit::Years })
}

struct Case {
    id: u32,
    base: &'static str,
    quote: &'static str,
    rates: [f64; 3],
    base_is_collateral: bool,
    spot: f64,
    fx: &'static [(&'static str, f64)],
    basis: &'static [(&'static str, f64)],
    basis_on_base: bool,
    resetting: ResettingLeg,
}

const CASES: [Case; 5] = [
    Case {
        id: 1, base: "sofr", quote: "tonar", rates: [0.042, 0.005, 0.005], base_is_collateral: false, spot: 150.0,
        fx: &[("1M", -0.55), ("3M", -1.62), ("6M", -3.2)],
        basis: &[("1Y", -0.0035), ("2Y", -0.0042), ("5Y", -0.0050), ("10Y", -0.0048)], basis_on_base: false, resetting: ResettingLeg::None,
    },
    Case {
        id: 2, base: "sofr", quote: "tonar", rates: [0.042, 0.005, 0.005], base_is_collateral: false, spot: 150.0, fx: &[],
        basis: &[("1Y", -0.0035), ("3Y", -0.0045), ("7Y", -0.0050)], basis_on_base: false, resetting: ResettingLeg::Base,
    },
    Case {
        id: 3, base: "estr", quote: "sofr", rates: [0.022, 0.042, 0.042], base_is_collateral: false, spot: 1.08, fx: &[("3M", 0.0052)],
        basis: &[("2Y", -0.0012), ("5Y", -0.0015), ("10Y", -0.0010)], basis_on_base: true, resetting: ResettingLeg::None,
    },
    Case {
        id: 4, base: "estr", quote: "sofr", rates: [0.022, 0.042, 0.042], base_is_collateral: false, spot: 1.08, fx: &[],
        basis: &[("2Y", -0.0012), ("5Y", -0.0015)], basis_on_base: true, resetting: ResettingLeg::Base,
    },
    Case {
        id: 5, base: "sofr", quote: "tonar", rates: [0.042, 0.005, 0.041], base_is_collateral: true, spot: 150.0, fx: &[("6M", -3.1)],
        basis: &[("1Y", -0.0030), ("5Y", -0.0045)], basis_on_base: false, resetting: ResettingLeg::Quote,
    },
];

fn spec(case: &Case) -> XccyCurveSpec {
    let (base, quote) = (index(case.base), index(case.quote));
    let far = Date::from_ymd(2040, 1, 1).unwrap();
    let calendar: HolidayCalendar = base.calendar.joined_with(&quote.calendar, today(), far).unwrap();
    let mut instruments: Vec<XccyInstrument> = case
        .fx
        .iter()
        .map(|&(t, points)| XccyInstrument::FxSwap { tenor: tenor(t), fixing_days: 2, forward_points: points })
        .collect();
    instruments.extend(case.basis.iter().map(|&(t, basis)| XccyInstrument::BasisSwap {
        tenor: tenor(t),
        fixing_days: 2,
        payment_frequency: 4,
        basis,
        basis_on_base_leg: case.basis_on_base,
        resetting: case.resetting,
    }));
    XccyCurveSpec {
        evaluation_date: today(),
        day_count: act365(),
        base: XccyLeg { index: base, forecast: flat(case.rates[0]) },
        quote: XccyLeg { index: quote, forecast: flat(case.rates[1]) },
        base_is_collateral: case.base_is_collateral,
        collateral_curve: flat(case.rates[2]),
        fx_spot: case.spot,
        calendar,
        convention: BusinessDayConvention::ModifiedFollowing,
        end_of_month: false,
        accuracy: 1e-12,
        instruments,
    }
}

#[test]
fn curves_match_quantlib() {
    let rows = golden_rows("xccy_curves.csv");
    let mut worst = 0.0f64;
    for case in &CASES {
        let built = bootstrap_xccy_curve(&spec(case)).unwrap_or_else(|e| panic!("case {}: {e}", case.id));
        for r in &built.quote_repricing {
            assert!((r.implied_quote - r.quote).abs() < 1e-9, "case {}: {r:?}", case.id);
        }
        let expected: Vec<(i64, f64)> = rows
            .iter()
            .filter(|r| r[0] == case.id.to_string())
            .map(|r| (r[1].parse().unwrap(), r[2].parse().unwrap()))
            .collect();
        assert_eq!(expected.len(), case.fx.len() + case.basis.len() + 1, "case {}", case.id);
        for (serial, discount) in expected {
            let d = Date::from_serial(serial).unwrap();
            let ours = built.curve.discount(d).unwrap_or_else(|e| panic!("case {} at {d}: {e}", case.id));
            let error = (ours - discount).abs();
            assert!(error < 1e-10, "case {} at {d}: {ours} against {discount}", case.id);
            worst = worst.max(error);
        }
    }
    println!("worst difference in discount factors {worst:e}");
}
