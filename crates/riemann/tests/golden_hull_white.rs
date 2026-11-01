//! Hull-White and SABR against QuantLib and LagrangeFinance (tools/golden/p9_hull_white_sabr.R).
//! All inputs are synthetic.

mod common;

use common::golden_rows;
use riemann::curves::{LogDiscountInterpolation, YieldCurve};
use riemann::hull_white::{bermudan_swaption, callable_bond, jamshidian_swaption, CallableFlow, GridSettings, HullWhite, Quadrature, SwaptionFlows};
use riemann::interest_rate::{Compounding, InterestRate};
use riemann::names;
use riemann::sabr::{sabr_volatility, SabrParameters, SabrVolatilityType};
use riemann::time::Date;

fn number(s: &str) -> f64 {
    s.parse().unwrap()
}

fn curve() -> YieldCurve {
    let today = Date::from_ymd(2026, 11, 2).unwrap();
    let rows = golden_rows("hw_curve.csv");
    YieldCurve::interpolated_discounts(
        rows.iter().map(|r| Date::from_serial(today.serial() as i64 + number(&r[0]) as i64).unwrap()).collect(),
        rows.iter().map(|r| number(&r[1])).collect(),
        names::day_count("actual_365_fixed").unwrap(),
        LogDiscountInterpolation::LogLinear,
    )
    .unwrap()
}

/// `days` are the dates of the schedule from today, joined by semicolons.
fn swaption_flows(payer: bool, days: &str, fixed_rate: f64) -> SwaptionFlows {
    let times: Vec<f64> = days.split(';').map(|d| number(d) / 365.0).collect();
    SwaptionFlows {
        payer,
        maturity: times[0],
        value_time: times[0],
        payment_times: times[1..].to_vec(),
        coupons: times.windows(2).map(|w| fixed_rate * (w[1] - w[0])).collect(),
        nominal: 1.0,
    }
}

#[test]
fn european_swaptions_against_quantlib() {
    let curve = curve();
    let rows = golden_rows("hw_swaption.csv");
    assert_eq!(rows.len(), 200);
    let mut worst = 0.0f64;
    for r in &rows {
        let n = |i: usize| number(&r[i]);
        let flows = swaption_flows(n(0) == 1.0, &r[1], n(2));
        let model = HullWhite { mean_reversion: n(3), volatility: n(4), curve: &curve };
        let v = jamshidian_swaption(&model, &flows).unwrap();
        worst = worst.max((v - n(5)).abs());
        // QuantLib finds the critical rate to 1e-8 only; repeating its search gives 5.4e-15
        assert!((v - n(5)).abs() < 1e-8, "{v} against {} ({r:?})", n(5));
    }
    println!("Swaptions: worst absolute difference {worst:e}");
}

#[test]
fn bermudan_swaptions_against_the_quantlib_tree() {
    let curve = curve();
    let rows = golden_rows("hw_bermudan.csv");
    assert_eq!(rows.len(), 20);
    let settings = GridSettings { points: 801, limit: 0.25, quadrature: Quadrature::Trapezoid(0.05) };
    let (mut worst, mut tree_change) = (0.0f64, 0.0f64);
    for r in &rows {
        let n = |i: usize| number(&r[i]);
        let flows = swaption_flows(n(0) == 1.0, &r[1], n(2));
        let mut exercise = vec![flows.maturity];
        exercise.extend(&flows.payment_times[..flows.payment_times.len() - 1]);
        let v = bermudan_swaption(&flows, &exercise, n(3), n(4), &curve, &settings).unwrap();
        worst = worst.max((v - n(6)).abs());
        tree_change = tree_change.max((n(6) - n(5)).abs());
        assert!((v - n(6)).abs() < 3e-4, "{v} against {} ({r:?})", n(6));
    }
    println!("Bermudan: worst difference from the tree {worst:e}; the tree's own change from 500 to 1000 steps {tree_change:e}");
}

#[test]
fn callable_bonds_against_lagrangefinance() {
    let today = Date::from_ymd(2026, 11, 2).unwrap();
    let dc = names::day_count("actual_365_fixed").unwrap();
    let rows = golden_rows("hw_callable.csv");
    assert_eq!(rows.len(), 20);
    let mut worst = 0.0f64;
    for r in &rows {
        let n = |i: usize| number(&r[i]);
        let (coupon_rate, call, first, maturity) = (n(0), n(1), n(2), n(3));
        let curve = YieldCurve::flat(today, InterestRate::new(n(4), dc.clone(), Compounding::Continuous, 0).unwrap());
        let periods = (maturity / 0.5).round() as usize;
        let flows: Vec<CallableFlow> = (1..=periods)
            .map(|i| {
                let time = i as f64 * 0.5;
                CallableFlow {
                    time,
                    coupon: 100.0 * coupon_rate * 0.5,
                    principal: if i == periods { 100.0 } else { 0.0 },
                    call_price: if time >= first && i < periods { Some(call) } else { None },
                }
            })
            .collect();
        let settings = GridSettings { points: n(7) as usize, limit: n(8), quadrature: Quadrature::GaussHermite(n(9) as usize) };
        let v = callable_bond(&flows, n(5), n(6), &curve, &settings).unwrap();
        worst = worst.max((v.bullet - n(10)).abs()).max((v.callable - n(11)).abs());
        assert!((v.bullet - n(10)).abs() < 1e-10, "bullet {} against {}", v.bullet, n(10));
        assert!((v.callable - n(11)).abs() < 1e-10, "callable {} against {}", v.callable, n(11));
    }
    println!("Callable: worst absolute difference {worst:e}");
}

#[test]
fn sabr_volatilities_against_quantlib() {
    let rows = golden_rows("sabr_volatility.csv");
    assert_eq!(rows.len(), 400);
    let mut worst = 0.0f64;
    for r in &rows {
        let n = |i: usize| number(&r[i]);
        let p = SabrParameters { alpha: n(4), beta: n(5), nu: n(6), rho: n(7) };
        let v = sabr_volatility(SabrVolatilityType::Lognormal, n(1), n(2), n(3), &p).unwrap();
        let diff = (v - n(8)).abs() / (1.0 + n(8).abs());
        worst = worst.max(diff);
        assert!(diff < 1e-12, "{v} against {} ({r:?})", n(8));
    }
    println!("SABR: worst difference {worst:e}");
}
