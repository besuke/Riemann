//! The American option approximations against the QuantLib R package
//! (tools/golden/p11_american_approximations.R). All inputs are synthetic. A case QuantLib refuses
//! (negative rates for Barone-Adesi-Whaley, a critical price that leaves the positive prices, the
//! double-boundary case of Bjerksund-Stensland) must be refused here too, and so must a case where
//! QuantLib returns NaN (a deliberate difference: an error rather than NaN).

mod common;

use common::golden_rows;
use riemann::american_approximations::{compute_american_approximation_value, AmericanApproximation};
use riemann::options::{OptionInputs, OptionType};

#[test]
fn approximations_match_quantlib() {
    let rows = golden_rows("american_approximations.csv");
    assert_eq!(rows.len(), 1440);
    let (mut worst, mut refused, mut nan) = (0.0f64, 0, 0);
    for r in &rows {
        let n = |i: usize| r[i].parse::<f64>().unwrap();
        let method = match r[0].as_str() {
            "barone_adesi_whaley" => AmericanApproximation::BaroneAdesiWhaley,
            "bjerksund_stensland" => AmericanApproximation::BjerksundStensland,
            other => panic!("{other}"),
        };
        let kind = if r[1] == "call" { OptionType::Call } else { OptionType::Put };
        let inputs = OptionInputs { spot: n(2), strike: n(3), rate: n(4), dividend_yield: n(5), volatility: n(6), time: n(7) };
        let value = compute_american_approximation_value(method, kind, &inputs);
        if r[8] == "NA" || r[8] == "NaN" {
            assert!(value.is_err(), "QuantLib gives {} for {r:?}, Riemann gives {value:?}", r[8]);
            if r[8] == "NA" { refused += 1 } else { nan += 1 }
            continue;
        }
        let expected = n(8);
        let value = value.unwrap_or_else(|e| panic!("{r:?}: {e}"));
        let error = (value - expected).abs() / expected.abs().max(1.0);
        assert!(error < 1e-11, "{r:?}: {value} against {expected}");
        worst = worst.max(error);
    }
    assert_eq!((refused, nan), (155, 15));
    println!("worst relative difference {worst:e}");
}
