//! Hull-White and SABR against QuantLib and LagrangeFinance (tools/golden/p9_hull_white_sabr.R).
//! All inputs are synthetic.

mod common;

use common::golden_rows;
use riemann::curves::{LogDiscountInterpolation, YieldCurve};
use riemann::hull_white::{MIN_FITTED_MEAN_REVERSION, calibrate_hull_white, CalibrationInstrument, CalibrationQuote, CalibrationSettings, CalibrationStop, compute_bermudan_swaption, compute_callable_bond, compute_jamshidian_swaption, CallableFlow, GridSettings, HullWhite, Quadrature, SwaptionFlows};
use riemann::interest_rate::{Compounding, InterestRate};
use riemann::names;
use riemann::sabr::{compute_sabr_volatility, SabrParameters, SabrVolatilityType};
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
        names::parse_day_count("actual_365_fixed").unwrap(),
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
        let v = compute_jamshidian_swaption(&model, &flows).unwrap();
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
        let v = compute_bermudan_swaption(&flows, &exercise, n(3), n(4), &curve, &settings).unwrap();
        worst = worst.max((v - n(6)).abs());
        tree_change = tree_change.max((n(6) - n(5)).abs());
        assert!((v - n(6)).abs() < 3e-4, "{v} against {} ({r:?})", n(6));
    }
    println!("Bermudan: worst difference from the tree {worst:e}; the tree's own change from 500 to 1000 steps {tree_change:e}");
}

#[test]
fn callable_bonds_against_lagrangefinance() {
    let today = Date::from_ymd(2026, 11, 2).unwrap();
    let dc = names::parse_day_count("actual_365_fixed").unwrap();
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
        let v = compute_callable_bond(&flows, n(5), n(6), &curve, &settings).unwrap();
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
        let v = compute_sabr_volatility(SabrVolatilityType::Lognormal, n(1), n(2), n(3), &p).unwrap();
        let diff = (v - n(8)).abs() / (1.0 + n(8).abs());
        worst = worst.max(diff);
        assert!(diff < 1e-12, "{v} against {} ({r:?})", n(8));
    }
    println!("SABR: worst difference {worst:e}");
}

/// QuantLib's `CalibratedModel::calibrate` on swaptions (tools/golden/p11_hull_white_calibration.R).
#[test]
fn calibration_against_quantlib() {
    let curve = curve();
    let cases = golden_rows("hw_calibration_cases.csv");
    let instruments = golden_rows("hw_calibration_instruments.csv");
    assert_eq!(cases.len(), 60);
    let (mut worst_ql_model, mut worst_ql_model_small_a) = (0.0f64, 0.0f64);
    let (mut worst_a, mut worst_sigma, mut worst_tight_a, mut worst_tight_sigma) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let (mut worst_fixed_sigma, mut worst_limit_sigma, mut worst_excess) = (0.0f64, 0.0f64, f64::NEG_INFINITY);
    let (mut interior, mut limit_cases, mut at_limit) = (0, 0, 0);
    // QuantLib's objective and each of its model prices against the fit, by kind of case
    let (mut interior_objective, mut interior_price) = (0.0f64, 0.0f64);
    let (mut fixed_objective, mut fixed_price) = (0.0f64, 0.0f64);
    let (mut limit_objective, mut limit_price, mut worst_floor) = (0.0f64, 0.0f64, 0.0f64);
    for c in &cases {
        let mode = c[1].as_str();
        let fix = mode == "price_fixed_a";
        let rows: Vec<_> = instruments.iter().filter(|r| r[0] == c[0]).collect();
        assert_eq!(rows.len(), number(&c[9]) as usize);
        let quotes: Vec<CalibrationQuote> = rows
            .iter()
            .map(|r| CalibrationQuote {
                instrument: CalibrationInstrument::Swaption(swaption_flows(r[1] == "1", &r[2], number(&r[3]))),
                price: number(&r[4]),
                weight: if mode == "relative" { 1.0 / number(&r[4]).powi(2) } else { 1.0 },
            })
            .collect();
        // QuantLib's prices at its own parameters, as a check of the instruments
        let (ql_a, ql_sigma) = (number(&c[5]), number(&c[6]));
        let model = HullWhite { mean_reversion: ql_a, volatility: ql_sigma, curve: &curve };
        for (q, r) in quotes.iter().zip(&rows) {
            let CalibrationInstrument::Swaption(flows) = &q.instrument else { unreachable!() };
            let difference = (compute_jamshidian_swaption(&model, flows).unwrap() - number(&r[5])).abs();
            if ql_a > 1e-3 {
                worst_ql_model = worst_ql_model.max(difference);
            } else {
                worst_ql_model_small_a = worst_ql_model_small_a.max(difference);
            }
        }
        let settings = CalibrationSettings {
            initial_mean_reversion: number(&c[3]),
            initial_volatility: number(&c[4]),
            fix_mean_reversion: fix,
            max_iterations: 500,
        };
        let fit = calibrate_hull_white(&quotes, &curve, &settings).unwrap();
        assert_ne!(fit.stop, CalibrationStop::MaxIterations, "case {}", c[0]);
        // The objective of any parameters, all priced by Riemann
        let objective_at = |a: f64, sigma: f64| -> f64 {
            let model = HullWhite { mean_reversion: a, volatility: sigma, curve: &curve };
            quotes
                .iter()
                .map(|q| {
                    let CalibrationInstrument::Swaption(flows) = &q.instrument else { unreachable!() };
                    q.weight * (compute_jamshidian_swaption(&model, flows).unwrap() - q.price).powi(2)
                })
                .sum()
        };
        let objective_gap = (number(&c[7]) / fit.objective - 1.0).abs();
        let price_gap = rows.iter().zip(&fit.model_prices).map(|(r, p)| (number(&r[5]) - p).abs()).fold(0.0, f64::max);
        // not worse than QuantLib's parameters, on the same prices
        let at_ql = objective_at(ql_a, ql_sigma);
        worst_excess = worst_excess.max((fit.objective - at_ql) / at_ql);
        // a local minimum: moving either free parameter by 0.1% does not lower the objective
        for (da, ds) in [(1.001, 1.0), (0.999, 1.0), (1.0, 1.001), (1.0, 0.999)] {
            if (fix || fit.at_lower_limit) && da != 1.0 {
                continue;
            }
            let moved = objective_at(fit.mean_reversion * da, fit.volatility * ds);
            assert!(moved >= fit.objective * (1.0 - 1e-10), "case {}: {moved} {}", c[0], fit.objective);
        }
        if fix {
            worst_fixed_sigma = worst_fixed_sigma.max((fit.volatility - ql_sigma).abs());
            fixed_objective = fixed_objective.max(objective_gap);
            fixed_price = fixed_price.max(price_gap);
        } else if number(&c[10]) > 1e-3 {
            // well inside the positive range, where the minimum is not at the limit of a
            worst_a = worst_a.max((fit.mean_reversion - ql_a).abs());
            worst_sigma = worst_sigma.max((fit.volatility - ql_sigma).abs());
            worst_tight_a = worst_tight_a.max((fit.mean_reversion - number(&c[10])).abs());
            worst_tight_sigma = worst_tight_sigma.max((fit.volatility - number(&c[11])).abs());
            interior_objective = interior_objective.max(objective_gap);
            interior_price = interior_price.max(price_gap);
            interior += 1;
        } else {
            at_limit += usize::from(fit.at_lower_limit);
            worst_limit_sigma = worst_limit_sigma.max((fit.volatility - number(&c[11])).abs());
            limit_objective = limit_objective.max(objective_gap);
            limit_price = limit_price.max(price_gap);
            // the limit is a real boundary minimum: with the volatility refitted, the objective rises from a = 0
            let refit = |a: f64| {
                let s = CalibrationSettings { initial_mean_reversion: a, fix_mean_reversion: true, initial_volatility: fit.volatility, max_iterations: 500 };
                calibrate_hull_white(&quotes, &curve, &s).unwrap().objective
            };
            let at_zero = refit(0.0);
            assert!(refit(1e-3) > at_zero && refit(1e-2) > refit(1e-3), "case {}", c[0]);
            worst_floor = worst_floor.max(refit(MIN_FITTED_MEAN_REVERSION) / at_zero - 1.0);
            limit_cases += 1;
        }
    }
    println!("instrument {worst_ql_model:e} small a {worst_ql_model_small_a:e} | {interior} interior: a {worst_a:e} sigma {worst_sigma:e} tight a {worst_tight_a:e} sigma {worst_tight_sigma:e} | {limit_cases} limit, {at_limit} at the limit: tight sigma {worst_limit_sigma:e} | fixed {worst_fixed_sigma:e} | excess {worst_excess:e}");
    println!("objective gap: interior {interior_objective:e} fixed {fixed_objective:e} limit {limit_objective:e}; price gap: interior {interior_price:e} fixed {fixed_price:e} limit {limit_price:e}; floor {worst_floor:e}");
    assert!(worst_ql_model < 1e-8, "{worst_ql_model}");
    assert_eq!((interior, limit_cases, at_limit), (11, 29, 29));
    assert!(worst_a < 5e-5, "{worst_a}");
    assert!(worst_sigma < 5e-6, "{worst_sigma}");
    assert!(worst_fixed_sigma < 1e-8, "{worst_fixed_sigma}");
    assert!(worst_excess < 1e-9, "{worst_excess}");
    assert!(interior_objective < 5e-7 && fixed_objective < 5e-7, "{interior_objective} {fixed_objective}");
    assert!(interior_price < 2e-6 && fixed_price < 1e-7, "{interior_price} {fixed_price}");
    assert!(worst_floor < 1e-6, "{worst_floor}");
}
