//! Compares bond prices, yields and yield risk with golden values from the QuantLib R
//! binding (`tools/golden/p2_bond_yields.R`). Closed-form values must match exactly or within
//! a relative 1e-15 (simple compounding) or 1e-12 (compounded and continuous, which use pow
//! and exp); solved yields exactly or within the solver accuracy (1e-10, absolute).
//! Errors must match. Inexact matches are counted and reported.

mod common;

use common::{bond_spec, date, day_count, golden_rows, Stats};
use riemann::cashflow_analysis::{DurationType, YieldConvention, YieldSolver};
use riemann::instruments::{FixedRateBond, PriceType};
use riemann::interest_rate::Compounding;
use riemann::Error;
use std::collections::BTreeMap;

fn check(stats: &mut Stats, golden: &str, got: Result<f64, Error>, rel: f64) -> bool {
    match (golden, got) {
        ("ERROR", Err(_)) => true,
        ("ERROR", Ok(_)) | (_, Err(_)) => false,
        (g, Ok(v)) => stats.close(g, v, rel),
    }
}

#[test]
fn bond_yields_match_quantlib() {
    let bonds: BTreeMap<String, FixedRateBond> = golden_rows("bond_cases.csv")
        .into_iter()
        .filter(|c| c[19] == "1")
        .map(|c| (c[0].clone(), FixedRateBond::new(bond_spec(&c)).expect("built in P2-1")))
        .collect();
    let rows = golden_rows("bond_yields.csv");

    let (mut closed, mut solved) = (Stats::default(), Stats::default());
    let mut bad = Vec::new();
    let mut bad_by_compounding: BTreeMap<String, usize> = BTreeMap::new();
    for r in &rows {
        let bond = &bonds[&r[0]];
        let settlement = bond.settlement_date(date(&r[1])).unwrap();
        if settlement != date(&r[2]) {
            bad.push(format!("case {}: settlement QuantLib {} Riemann {settlement}", r[0], date(&r[2])));
            continue;
        }
        let compounding = match r[5].as_str() {
            "Simple" => Compounding::Simple,
            "Compounded" => Compounding::Compounded,
            "Continuous" => Compounding::Continuous,
            other => panic!("unknown compounding {other}"),
        };
        let convention = YieldConvention { day_count: day_count(&r[4]), compounding, frequency: r[6].parse().unwrap() };
        let y = convention.rate(r[3].parse().unwrap()).unwrap();
        // Specification 7.3: arithmetic-only results (simple compounding) within 1e-15;
        // results that go through pow/exp within 1e-12, because the platform math libraries
        // differ in the last bit (R's MinGW libm vs MSVC: about 1 in 300 pow/exp results).
        let rel = if compounding == Compounding::Simple { 1e-15 } else { 1e-12 };
        let checks = [
            ("clean", check(&mut closed, &r[7], bond.clean_price(&y, settlement), rel)),
            ("accrued", check(&mut closed, &r[8], bond.accrued_amount(settlement), rel)),
            ("simple", check(&mut closed, &r[9], bond.duration(&y, DurationType::Simple, settlement), rel)),
            ("modified", check(&mut closed, &r[10], bond.duration(&y, DurationType::Modified, settlement), rel)),
            ("macaulay", check(&mut closed, &r[11], bond.duration(&y, DurationType::Macaulay, settlement), rel)),
            ("convexity", check(&mut closed, &r[12], bond.convexity(&y, settlement), rel)),
            ("bpv", check(&mut closed, &r[13], bond.basis_point_value(&y, settlement), rel)),
            ("yvbp", check(&mut closed, &r[14], bond.yield_value_basis_point(&y, settlement), rel)),
        ];
        for (name, ok) in checks {
            if !ok {
                bad.push(format!("case {} {name}: row {r:?}", r[0]));
                *bad_by_compounding.entry(format!("{} {} {name}", r[5], r[6])).or_default() += 1;
            }
        }
        let price_type = if r[16] == "Clean" { PriceType::Clean } else { PriceType::Dirty };
        let got = bond.yield_from_price(r[15].parse().unwrap(), price_type, &convention, settlement, YieldSolver::default());
        let ok = match (r[17].as_str(), &got) {
            ("ERROR", Err(_)) => true,
            ("ERROR", Ok(_)) | (_, Err(_)) => false,
            (g, Ok(v)) => {
                let q: f64 = g.parse().unwrap();
                if q == *v {
                    solved.exact += 1;
                    true
                } else {
                    solved.inexact += 1;
                    solved.max_abs = solved.max_abs.max((q - v).abs());
                    (q - v).abs() <= 1e-10
                }
            }
        };
        if !ok {
            bad.push(format!("case {} yield: QuantLib {}, Riemann {got:?}", r[0], r[17]));
        }
    }
    eprintln!(
        "rows {}; closed-form values: {} exact, {} inexact (max rel {:e}); yields: {} exact, {} inexact (max abs {:e})",
        rows.len(),
        closed.exact,
        closed.inexact,
        closed.max_rel,
        solved.exact,
        solved.inexact,
        solved.max_abs
    );
    assert!(
        bad.is_empty(),
        "{} mismatches by compounding/frequency/value: {bad_by_compounding:?}; first:\n{}",
        bad.len(),
        bad[..bad.len().min(8)].join("\n")
    );
}
