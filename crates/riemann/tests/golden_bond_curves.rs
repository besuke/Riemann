//! Compares discount factors of yield curves and bond values on them with golden values from
//! the QuantLib R binding (`tools/golden/p2_bond_curves.R`). Values must match exactly or
//! within a relative 1e-15 (flat curves with simple compounding, arithmetic only) or 1e-12
//! (everything that goes through pow, exp or log). Errors must match, with the same cause.
//! Inexact matches are counted and reported.

mod common;

use common::{bond_spec, date, day_count, golden_rows, Stats};
use riemann::curves::YieldCurve;
use riemann::instruments::FixedRateBond;
use riemann::interest_rate::{Compounding, InterestRate};
use riemann::Error;
use std::collections::BTreeMap;

fn compounding(name: &str) -> Compounding {
    match name {
        "Simple" => Compounding::Simple,
        "Compounded" => Compounding::Compounded,
        "Continuous" => Compounding::Continuous,
        other => panic!("unknown compounding {other}"),
    }
}

fn nodes(s: &str) -> (Vec<riemann::Date>, Vec<f64>) {
    s.split(' ')
        .map(|n| {
            let (d, v) = n.split_once(':').unwrap();
            (date(d), v.parse::<f64>().unwrap())
        })
        .unzip()
}

/// Builds the curves of `curves.csv`; also returns the relative tolerance for each.
fn build_curves() -> BTreeMap<String, (YieldCurve, f64)> {
    let mut curves: BTreeMap<String, (YieldCurve, f64)> = BTreeMap::new();
    for r in golden_rows("curves.csv") {
        let (dc, comp, freq) = (
            day_count(&r[3]),
            compounding(&r[4]),
            r[5].parse::<u32>().unwrap(),
        );
        let (curve, rel) = match r[1].as_str() {
            "flat" => {
                let (d, v) = nodes(&r[7]);
                let rate = InterestRate::new(v[0], dc, comp, freq).unwrap();
                (
                    YieldCurve::flat(d[0], rate),
                    if comp == Compounding::Simple {
                        1e-15
                    } else {
                        1e-12
                    },
                )
            }
            "zero" => {
                let (d, v) = nodes(&r[7]);
                (YieldCurve::zero(d, v, dc, comp, freq).unwrap(), 1e-12)
            }
            "discount" => {
                let (d, v) = nodes(&r[7]);
                (YieldCurve::discount_factors(d, v, dc).unwrap(), 1e-12)
            }
            "spread" => {
                let base = curves
                    .remove(&r[8])
                    .expect("base curve precedes its spread")
                    .0;
                (
                    YieldCurve::zero_spreaded(base, r[9].parse().unwrap(), comp, freq).unwrap(),
                    1e-12,
                )
            }
            other => panic!("unknown curve kind {other}"),
        };
        assert_eq!(
            curve.reference_date(),
            date(&r[2]),
            "reference date of {}",
            r[0]
        );
        curves.insert(r[0].clone(), (curve.with_extrapolation(r[6] == "1"), rel));
    }
    curves
}

/// Every QuantLib error in these golden files is "time (…) is past max curve time (…)"
/// (checked from the messages on 2026-10-31), so the matching Riemann error is
/// `OutsideCurveRange`; any other error counts as a mismatch.
fn check(stats: &mut Stats, golden: &str, got: &Result<f64, Error>, rel: f64) -> bool {
    match (golden, got) {
        ("ERROR", Err(Error::OutsideCurveRange { .. })) => true,
        ("ERROR", _) | (_, Err(_)) => false,
        (g, Ok(v)) => stats.close(g, *v, rel),
    }
}

#[test]
fn curve_discounts_match_quantlib() {
    let curves = build_curves();
    let rows = golden_rows("curve_discounts.csv");
    let (mut stats, mut errors, mut bad) = (Stats::default(), 0, Vec::new());
    for r in &rows {
        let (curve, rel) = &curves[&r[0]];
        let got = curve.discount(date(&r[1]));
        errors += usize::from(r[2] == "ERROR");
        if !check(&mut stats, &r[2], &got, *rel) {
            bad.push(format!(
                "curve {} date {}: QuantLib {}, Riemann {got:?}",
                r[0], r[1], r[2]
            ));
        }
    }
    eprintln!(
        "discounts {} (QuantLib errors {errors}): {} exact, {} inexact (max rel {:e})",
        rows.len(),
        stats.exact,
        stats.inexact,
        stats.max_rel
    );
    assert!(
        bad.is_empty(),
        "{} mismatches; first:\n{}",
        bad.len(),
        bad[..bad.len().min(8)].join("\n")
    );
}

#[test]
fn bond_values_on_curves_match_quantlib() {
    let curves = build_curves();
    let bonds: BTreeMap<String, FixedRateBond> = golden_rows("bond_cases.csv")
        .into_iter()
        .filter(|c| c[19] == "1")
        .map(|c| {
            (
                c[0].clone(),
                FixedRateBond::new(bond_spec(&c)).expect("built in P2-1"),
            )
        })
        .collect();
    let rows = golden_rows("bond_curve_values.csv");
    let (mut stats, mut errors, mut bad) = (Stats::default(), 0, Vec::new());
    for r in &rows {
        let (curve, rel) = &curves[&r[2]];
        let value = bonds[&r[0]].value_on_curve(curve, date(&r[1]));
        errors += usize::from(r[3] == "ERROR");
        let pick = |f: fn(&riemann::instruments::BondCurveValue) -> f64| {
            value.as_ref().map(f).map_err(Clone::clone)
        };
        let checks = [
            ("npv", check(&mut stats, &r[3], &pick(|v| v.npv), *rel)),
            (
                "clean",
                check(&mut stats, &r[4], &pick(|v| v.clean_price), *rel),
            ),
            (
                "dirty",
                check(&mut stats, &r[5], &pick(|v| v.dirty_price), *rel),
            ),
        ];
        for (name, ok) in checks {
            if !ok {
                bad.push(format!(
                    "case {} {name}: row {r:?}, Riemann {value:?}",
                    r[0]
                ));
            }
        }
    }
    eprintln!(
        "bonds {} (QuantLib errors {errors}): values {} exact, {} inexact (max rel {:e})",
        rows.len(),
        stats.exact,
        stats.inexact,
        stats.max_rel
    );
    assert!(
        bad.is_empty(),
        "{} mismatches; first:\n{}",
        bad.len(),
        bad[..bad.len().min(8)].join("\n")
    );
}
