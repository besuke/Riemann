//! Options against the QuantLib R package Version 1.42 (tools/golden/p6_options.R). Inputs are
//! synthetic. Tolerances are relative, plus an absolute term for values near 0.

mod common;

use common::golden_rows;
use riemann::options::{
    barrier_greeks, binomial_greeks, european_greeks, forward_greeks, implied_volatility,
    BarrierType, Exercise, ForwardInputs, ForwardModel, OptionInputs, OptionType,
};

/// QuantLib's drift comes from a forward rate over 0.0001 years, which carries rounding noise of
/// about 1e-12: docs/spec_options.md.
const TREE_RELATIVE: f64 = 1e-9;
const TREE_ABSOLUTE: f64 = 2e-9;

fn number(s: &str) -> f64 {
    s.parse().unwrap()
}

fn kind(s: &str) -> OptionType {
    match s {
        "call" => OptionType::Call,
        "put" => OptionType::Put,
        other => panic!("{other}"),
    }
}

fn check(name: &str, got: f64, expected: f64, relative: f64, absolute: f64, worst: &mut f64) {
    let diff = (got - expected).abs();
    *worst = worst.max(diff / (1.0 + expected.abs()));
    assert!(
        diff <= relative * expected.abs() + absolute,
        "{name}: {got} against {expected} (difference {diff})"
    );
}

#[test]
fn european_values_and_greeks() {
    let rows = golden_rows("option_european.csv");
    assert_eq!(rows.len(), 300);
    let mut worst = 0.0;
    for r in &rows {
        let n = |i: usize| number(&r[i]);
        let inputs = OptionInputs {
            spot: n(1),
            strike: n(2),
            rate: n(3),
            dividend_yield: n(4),
            volatility: n(5),
            time: n(6) / 365.0,
        };
        let g = european_greeks(kind(&r[0]), &inputs).unwrap();
        check("value", g.value, n(7), 1e-12, 1e-12, &mut worst);
        check("delta", g.delta, n(8), 1e-12, 1e-12, &mut worst);
        check("gamma", g.gamma, n(9), 1e-11, 1e-13, &mut worst);
        check("vega", g.vega, n(10), 1e-11, 1e-11, &mut worst);
        check("rho", g.rho, n(11), 1e-11, 1e-11, &mut worst);
        check("dividend rho", g.dividend_rho, n(12), 1e-11, 1e-11, &mut worst);
        check("theta", g.theta, n(13), 1e-11, 1e-11, &mut worst);
        check("strike sensitivity", g.strike_sensitivity, n(14), 1e-11, 1e-11, &mut worst);
    }
    println!("European: worst difference {worst:e}");
}

#[test]
fn barrier_values() {
    let rows = golden_rows("option_barrier.csv");
    assert_eq!(rows.len(), 320);
    let mut worst = 0.0;
    for r in &rows {
        let n = |i: usize| number(&r[i]);
        let barrier_type = match r[1].as_str() {
            "down_in" => BarrierType::DownIn,
            "up_in" => BarrierType::UpIn,
            "down_out" => BarrierType::DownOut,
            "up_out" => BarrierType::UpOut,
            other => panic!("{other}"),
        };
        let inputs = OptionInputs {
            spot: n(2),
            strike: n(3),
            rate: n(6),
            dividend_yield: n(7),
            volatility: n(8),
            time: n(9) / 365.0,
        };
        let g = barrier_greeks(kind(&r[0]), barrier_type, n(4), n(5), &inputs).unwrap();
        check("barrier value", g.value, n(10), 1e-10, 1e-10, &mut worst);
    }
    println!("Barrier: worst difference {worst:e}");
}

#[test]
fn tree_values_delta_and_gamma() {
    let rows = golden_rows("option_tree.csv");
    assert_eq!(rows.len(), 200);
    let (mut worst, mut short_grid) = (0.0, 0);
    for r in &rows {
        let n = |i: usize| number(&r[i]);
        let exercise = match r[1].as_str() {
            "american" => Exercise::American,
            "european" => Exercise::European,
            other => panic!("{other}"),
        };
        let inputs = OptionInputs {
            spot: n(3),
            strike: n(4),
            rate: n(5),
            dividend_yield: n(6),
            volatility: n(7),
            time: n(8) / 365.0,
        };
        let g = binomial_greeks(kind(&r[0]), exercise, &inputs, n(2) as usize).unwrap();
        // QuantLib's last grid time, dt * steps, can round below the maturity. An American option
        // then misses its payoff at maturity in QuantLib, and is worth less: docs/spec_options.md
        if exercise == Exercise::American && (inputs.time / n(2)) * n(2) < inputs.time {
            short_grid += 1;
            assert!(g.value >= n(9) - 1e-9, "{} against {}", g.value, n(9));
            continue;
        }
        check("tree value", g.value, n(9), TREE_RELATIVE, TREE_ABSOLUTE, &mut worst);
        check("tree delta", g.delta, n(10), TREE_RELATIVE * 10.0, TREE_ABSOLUTE, &mut worst);
        check("tree gamma", g.gamma, n(11), TREE_RELATIVE * 100.0, TREE_ABSOLUTE, &mut worst);
    }
    assert_eq!(short_grid, 11);
    println!("Tree: worst difference {worst:e}");
}

#[test]
fn forward_formulas() {
    let rows = golden_rows("option_forward.csv");
    assert_eq!(rows.len(), 400);
    let mut worst = 0.0;
    for r in &rows {
        let n = |i: usize| number(&r[i]);
        let model = match r[0].as_str() {
            "black" => ForwardModel::Black,
            "bachelier" => ForwardModel::Bachelier,
            other => panic!("{other}"),
        };
        let inputs = ForwardInputs {
            forward: n(2),
            strike: n(3),
            volatility: n(4),
            time: n(5),
            discount: n(6),
        };
        let g = forward_greeks(model, kind(&r[1]), &inputs).unwrap();
        check("forward value", g.value, n(7), 1e-11, 1e-11, &mut worst);
    }
    println!("Forward: worst difference {worst:e}");
}

#[test]
fn implied_volatilities() {
    let rows = golden_rows("option_implied.csv");
    assert_eq!(rows.len(), 100);
    let mut worst = 0.0;
    for r in &rows {
        let n = |i: usize| number(&r[i]);
        let inputs = OptionInputs {
            spot: n(1),
            strike: n(2),
            rate: n(3),
            dividend_yield: n(4),
            volatility: n(5),
            time: n(6) / 365.0,
        };
        let v = implied_volatility(kind(&r[0]), n(7), &inputs, 1e-10, 200).unwrap();
        // both search to 1e-10 and neither iterates identically, so the bound is not tighter
        check("implied volatility", v, n(8), 0.0, 1e-8, &mut worst);
        check("true volatility", v, n(5), 0.0, 1e-7, &mut worst);
    }
    println!("Implied: worst difference {worst:e}");
}
