//! Compares OIS curves with golden values from the QuantLib R package Version 1.42, whose exact
//! C++ commit is unknown (`tools/golden/p3_tonar_ois.R` for log-linear TONAR,
//! `tools/golden/p3_tonar_log_cubic.R` and `tools/golden/p3_ois_index_log_cubic.R` for monotonic
//! log-cubic). Dates must match exactly and
//! errors by cause. Node discount factors, discounts and implied rates must match within the
//! relative tolerance of each test, and the curve must reprice each quote within 1e-9.

mod common;

use common::{date, day_count, golden_rows, ois_curve_spec, Stats};
use riemann::interest_rate::Compounding;
use riemann::ois::{
    build_ois_curve, estr, sofr, sonia, tonar, OisCurve, OisInterpolation, OvernightIndex,
};
use riemann::Error;
use std::collections::{BTreeMap, BTreeSet};

/// Maps a QuantLib error message to the matching Riemann error.
fn same_cause(row: &[String], got: &Error) -> bool {
    let (status, text) = (row[9].as_str(), got.to_string());
    let keys = [
        "root not bracketed",
        "more than one instrument",
        "all instruments expired",
        "negative time",
        "convergence not reached",
        "invalid value",
        "maximum number of function evaluations",
    ];
    let Some(key) = keys.iter().find(|k| status.contains(**k)) else {
        return false;
    };
    // the log-cubic golden file also tells where the bootstrap stopped
    let same_place = match (row.get(10), row.get(11)) {
        (Some(iteration), Some(alive)) if iteration != "0" => text.contains(&format!(
            "iteration {iteration}: failed at alive instrument {alive},"
        )),
        _ => true,
    };
    text.contains(key) && same_place
}

fn group(rows: &[Vec<String>]) -> BTreeMap<&str, Vec<&Vec<String>>> {
    let mut map: BTreeMap<&str, Vec<&Vec<String>>> = BTreeMap::new();
    for r in rows {
        map.entry(r[0].as_str()).or_default().push(r);
    }
    map
}

/// Compares a rate or an error; the size of the difference is kept in `stats`.
fn check_rate(
    name: &str,
    golden: &str,
    got: Result<f64, Error>,
    stats: &mut Stats,
    bad: &mut Vec<String>,
    case: &str,
    d: &str,
) {
    match (golden, got) {
        ("ERROR", Err(_)) => {}
        ("ERROR", Ok(v)) => bad.push(format!(
            "case {case} date {d} {name}: QuantLib error, Riemann {v}"
        )),
        (g, Err(e)) => bad.push(format!(
            "case {case} date {d} {name}: QuantLib {g}, Riemann {e}"
        )),
        (g, Ok(v)) => {
            stats.close(g, v, f64::INFINITY);
        }
    }
}

/// `relative` applies to discount factors. Rates are compared by absolute difference where
/// the field says so, because a relative test on a small short rate only measures its size.
struct Tolerances {
    relative: f64,
    discount_absolute: f64,
    implied_relative: f64,
    implied_absolute: f64,
    zero: f64,
    forward_1d: f64,
    forward_instantaneous: f64,
    /// Curves whose repriced quotes miss the input by more than 1e-9. They are listed one by one:
    /// agreeing with QuantLib does not make them meet the reproduction criterion.
    reproduction_misses: &'static [&'static str],
}

fn check_curves(
    prefix: &str,
    index: OvernightIndex,
    interpolation: OisInterpolation,
    tolerances: Tolerances,
) {
    let cases = golden_rows(&format!("{prefix}_cases.csv"));
    let instruments_rows = golden_rows(&format!("{prefix}_instruments.csv"));
    let nodes_rows = golden_rows(&format!("{prefix}_nodes.csv"));
    let discounts_rows = golden_rows(&format!("{prefix}_discounts.csv"));
    let (instruments, nodes, discounts) = (
        group(&instruments_rows),
        group(&nodes_rows),
        group(&discounts_rows),
    );

    let mut bad = Vec::new();
    let mut causes: BTreeMap<String, usize> = BTreeMap::new();
    let (mut built, mut date_checks, mut errors_in_discounts) = (0, 0, 0);
    let (mut s_nodes, mut s_discounts, mut s_implied) =
        (Stats::default(), Stats::default(), Stats::default());
    let (mut s_zero, mut s_forward, mut s_instantaneous) =
        (Stats::default(), Stats::default(), Stats::default());
    let (mut max_repricing_ql, mut max_repricing) = (0.0_f64, 0.0_f64);
    let (mut quantlib_miss_quotes, mut riemann_miss_quotes) = (0, 0);
    let (mut quantlib_miss_curves, mut riemann_miss_curves) = (BTreeSet::new(), BTreeSet::new());
    // how many values the criteria used before P3-2 would have rejected (docs/spec_ois.md 10.4)
    let (mut nodes_over, mut discounts_over, mut implied_over) = (0, 0, 0);

    for r in &cases {
        let case = r[0].as_str();
        let status = r[9].as_str();
        let result = build_ois_curve(&ois_curve_spec(r, index.clone(), interpolation));
        *causes
            .entry(status.split(':').next().unwrap().chars().take(40).collect())
            .or_default() += 1;
        let OisCurve {
            curve,
            quote_repricing,
        } = match (status, result) {
            ("ok", Ok(c)) => c,
            ("ok", Err(e)) => {
                bad.push(format!(
                    "case {case}: QuantLib built the curve, Riemann failed: {e}"
                ));
                continue;
            }
            (s, Err(e)) => {
                if !same_cause(r, &e) {
                    bad.push(format!("case {case}: QuantLib `{s}`, Riemann `{e}`"));
                }
                continue;
            }
            (s, Ok(_)) => {
                bad.push(format!(
                    "case {case}: QuantLib `{s}`, Riemann built the curve"
                ));
                continue;
            }
        };
        built += 1;
        let curve = curve.with_extrapolation(r[4] == "1");

        for g in &instruments[case] {
            let q = &quote_repricing[g[1].parse::<usize>().unwrap()];
            let dates_match = q.start_date == date(&g[2])
                && q.maturity_date == date(&g[3])
                && q.pillar_date == date(&g[4])
                && q.latest_relevant_date == date(&g[5])
                && q.used_in_bootstrap == (g[6] == "1");
            date_checks += 1;
            if !dates_match {
                bad.push(format!(
                    "case {case}: instrument dates: golden {g:?}, Riemann {q:?}"
                ));
            }
            match (g[7].as_str(), q.implied_rate) {
                ("NA", None) => {}
                ("NA", Some(_)) | (_, None) => bad.push(format!(
                    "case {case}: implied rate: golden {g:?}, Riemann {q:?}"
                )),
                (implied, Some(v)) => {
                    let golden: f64 = implied.parse().unwrap();
                    let (quantlib_error, riemann_error) =
                        ((golden - q.rate).abs(), (v - q.rate).abs());
                    max_repricing_ql = max_repricing_ql.max(quantlib_error);
                    max_repricing = max_repricing.max(riemann_error);
                    if quantlib_error > 1e-9 {
                        quantlib_miss_quotes += 1;
                        quantlib_miss_curves.insert(case.to_string());
                    }
                    if riemann_error > 1e-9 {
                        riemann_miss_quotes += 1;
                        riemann_miss_curves.insert(case.to_string());
                    }
                    implied_over += usize::from((v - golden).abs() > 1e-12 * golden.abs());
                    if !s_implied.close(implied, v, tolerances.implied_relative) {
                        bad.push(format!(
                            "case {case}: implied rate: QuantLib {implied}, Riemann {v}"
                        ));
                    }
                }
            }
        }

        let (node_dates, node_values) = curve.discount_nodes().expect("a discount curve");
        let golden_nodes = &nodes[case];
        if node_dates.len() != golden_nodes.len() {
            bad.push(format!(
                "case {case}: {} nodes, QuantLib {}",
                node_dates.len(),
                golden_nodes.len()
            ));
            continue;
        }
        for ((d, v), g) in node_dates.iter().zip(node_values).zip(golden_nodes) {
            if *d != date(&g[1]) {
                bad.push(format!(
                    "case {case}: node date {d}, QuantLib {}",
                    date(&g[1])
                ));
            }
            let golden: f64 = g[2].parse().unwrap();
            nodes_over += usize::from((v - golden).abs() > 1e-12 * golden.abs());
            if !s_nodes.close(&g[2], *v, tolerances.relative) {
                bad.push(format!(
                    "case {case} node {d}: QuantLib {}, Riemann {v}",
                    g[2]
                ));
            }
        }

        let dc = day_count(&r[3]);
        for g in discounts.get(case).into_iter().flatten() {
            let d = date(&g[1]);
            let got = curve.discount(d);
            errors_in_discounts += usize::from(g[2] == "ERROR");
            let ok = match (g[2].as_str(), &got) {
                // the golden file has no messages: before the reference date the time is
                // negative, otherwise the date is past the curve without extrapolation
                ("ERROR", Err(Error::InvalidInput { reason })) => {
                    d < date(&r[2]) && reason.contains("negative time")
                }
                ("ERROR", Err(Error::OutsideCurveRange { .. })) => d >= date(&r[2]) && r[4] == "0",
                ("ERROR", _) | (_, Err(_)) => false,
                (golden, Ok(v)) => {
                    let value: f64 = golden.parse().unwrap();
                    discounts_over += usize::from((v - value).abs() > 1e-12 * value.abs());
                    s_discounts.close(golden, *v, tolerances.relative)
                }
            };
            if !ok {
                bad.push(format!(
                    "case {case} date {}: QuantLib {}, Riemann {got:?}",
                    g[1], g[2]
                ));
            }
            if g.len() > 3 {
                let rate =
                    |x: Result<riemann::interest_rate::InterestRate, Error>| x.map(|i| i.rate);
                let next = date(&(g[1].parse::<i64>().unwrap() + 1).to_string());
                let zero = rate(curve.zero_rate(d, &dc, Compounding::Continuous, 1));
                check_rate("zero", &g[3], zero, &mut s_zero, &mut bad, case, &g[1]);
                let forward = rate(curve.forward_rate(d, next, &dc, Compounding::Simple, 1));
                check_rate(
                    "forward",
                    &g[4],
                    forward,
                    &mut s_forward,
                    &mut bad,
                    case,
                    &g[1],
                );
                let instantaneous = rate(curve.forward_rate(d, d, &dc, Compounding::Continuous, 1));
                check_rate(
                    "forward at a date",
                    &g[5],
                    instantaneous,
                    &mut s_instantaneous,
                    &mut bad,
                    case,
                    &g[1],
                );
            }
        }
    }

    eprintln!("cases {} built {built}; statuses {causes:?}", cases.len());
    eprintln!("instrument dates checked {date_checks}");
    for (name, s) in [
        ("nodes", &s_nodes),
        ("discounts", &s_discounts),
        ("implied", &s_implied),
        ("zero", &s_zero),
        ("forward 1d", &s_forward),
        ("forward at", &s_instantaneous),
    ] {
        eprintln!(
            "{name:10} {:6} exact {:6} inexact (max rel {:e}, max abs {:e})",
            s.exact, s.inexact, s.max_rel, s.max_abs
        );
    }
    eprintln!("discount errors matched {errors_in_discounts}");
    eprintln!("max |implied - quote|: QuantLib {max_repricing_ql:e}, Riemann {max_repricing:e}");
    eprintln!("reproduction over 1e-9: QuantLib {quantlib_miss_quotes} quotes in {quantlib_miss_curves:?}, Riemann {riemann_miss_quotes} quotes in {riemann_miss_curves:?}");
    eprintln!("over a relative 1e-12: nodes {nodes_over}, discounts {discounts_over}, implied {implied_over}");
    assert!(
        s_nodes.max_abs <= tolerances.discount_absolute,
        "nodes {:e}",
        s_nodes.max_abs
    );
    assert!(
        s_discounts.max_abs <= tolerances.discount_absolute,
        "discounts {:e}",
        s_discounts.max_abs
    );
    let expected: BTreeSet<String> = tolerances
        .reproduction_misses
        .iter()
        .map(|c| c.to_string())
        .collect();
    assert_eq!(
        riemann_miss_curves, expected,
        "curves missing the reproduction criterion, Riemann"
    );
    assert_eq!(
        quantlib_miss_curves, expected,
        "curves missing the reproduction criterion, QuantLib"
    );
    assert!(
        s_implied.max_abs <= tolerances.implied_absolute,
        "implied {:e}",
        s_implied.max_abs
    );
    assert!(
        s_zero.max_abs <= tolerances.zero,
        "zero rate {:e}",
        s_zero.max_abs
    );
    assert!(
        s_forward.max_abs <= tolerances.forward_1d,
        "forward {:e}",
        s_forward.max_abs
    );
    assert!(
        s_instantaneous.max_abs <= tolerances.forward_instantaneous,
        "forward at a date {:e}",
        s_instantaneous.max_abs
    );
    assert!(
        bad.is_empty(),
        "{} mismatches; first:\n{}",
        bad.len(),
        bad[..bad.len().min(10)].join("\n")
    );
}

#[test]
fn log_linear_curves_match_quantlib_r_package() {
    check_curves(
        "ois_tonar",
        tonar(),
        OisInterpolation::LogLinear,
        Tolerances {
            relative: 1e-12,
            discount_absolute: 1.0,
            implied_relative: 1e-12,
            implied_absolute: 1e-10,
            zero: 0.0,
            forward_1d: 0.0,
            forward_instantaneous: 0.0,
            reproduction_misses: &[],
        },
    );
}

/// Discount factors: each bootstrap stops within the accuracy of 1e-12, and the number of passes
/// can differ by one when the last change is on the threshold (specification 5.1), hence 2e-12.
/// Rates: absolute 1e-10 (specification 5.1). The forward at a date divides by 0.0001 years, so the
/// relative 1e-12 of discount factors becomes 1e-8.
fn monotonic_log_cubic_tolerances(reproduction_misses: &'static [&'static str]) -> Tolerances {
    Tolerances {
        relative: f64::INFINITY,
        discount_absolute: 2e-12,
        implied_relative: f64::INFINITY,
        implied_absolute: 1e-10,
        zero: 1e-10,
        forward_1d: 1e-10,
        forward_instantaneous: 1e-8,
        reproduction_misses,
    }
}

#[test]
fn tonar_monotonic_log_cubic_curves_match_quantlib_r_package() {
    check_curves(
        "ois_tonar_log_cubic",
        tonar(),
        OisInterpolation::MonotonicLogCubic,
        monotonic_log_cubic_tolerances(&[]),
    );
}

#[test]
fn estr_monotonic_log_cubic_curves_match_quantlib_r_package() {
    check_curves(
        "ois_estr_log_cubic",
        estr(),
        OisInterpolation::MonotonicLogCubic,
        monotonic_log_cubic_tolerances(&["125", "246"]),
    );
}

#[test]
fn sofr_monotonic_log_cubic_curves_match_quantlib_r_package() {
    check_curves(
        "ois_sofr_log_cubic",
        sofr(),
        OisInterpolation::MonotonicLogCubic,
        monotonic_log_cubic_tolerances(&["199"]),
    );
}

#[test]
fn sonia_monotonic_log_cubic_curves_match_quantlib_r_package() {
    check_curves(
        "ois_sonia_log_cubic",
        sonia(),
        OisInterpolation::MonotonicLogCubic,
        monotonic_log_cubic_tolerances(&[]),
    );
}
