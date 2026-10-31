//! Compares TONAR OIS curves with golden values from the QuantLib R package Version 1.42,
//! whose exact C++ commit is unknown (`tools/golden/p3_tonar_ois.R`). Dates must match exactly
//! and errors by cause. Node discount factors, discounts and implied rates must match within
//! a relative 1e-12 (specification 7.3), and the curve must reprice each quote within 1e-9.

mod common;

use common::{date, day_count, golden_rows, Stats};
use riemann::ois::{
    build_ois_curve, tonar, OisCurve, OisCurveInstrument, OisCurveSpec, OisInterpolation,
};
use riemann::{Error, Period, TimeUnit};
use std::collections::BTreeMap;

fn tenor(label: &str) -> Period {
    let (n, unit) = label.split_at(label.len() - 1);
    let unit = match unit {
        "W" => TimeUnit::Weeks,
        "M" => TimeUnit::Months,
        "Y" => TimeUnit::Years,
        other => panic!("unknown unit {other}"),
    };
    Period::new(n.parse().unwrap(), unit)
}

fn spec(r: &[String]) -> OisCurveSpec {
    let instruments = r[8]
        .split(' ')
        .map(|item| {
            let (label, rate) = item.split_once(':').unwrap();
            let rate = rate.parse().unwrap();
            if label == "ON" {
                OisCurveInstrument::OvernightDeposit { rate }
            } else {
                OisCurveInstrument::Ois {
                    tenor: tenor(label),
                    rate,
                    settlement_days: r[5].parse().unwrap(),
                    payment_frequency: r[7].parse().unwrap(),
                    payment_lag: r[6].parse().unwrap(),
                }
            }
        })
        .collect();
    OisCurveSpec {
        evaluation_date: date(&r[1]),
        reference_date: date(&r[2]),
        index: tonar(),
        curve_day_count: day_count(&r[3]),
        instruments,
        interpolation: OisInterpolation::LogLinear,
        accuracy: 1.0e-12,
    }
}

/// Maps a QuantLib error message to the matching Riemann error.
fn same_cause(status: &str, got: &Error) -> bool {
    let text = got.to_string();
    let key = if status.contains("root not bracketed") {
        "root not bracketed"
    } else if status.contains("more than one instrument") {
        "more than one instrument"
    } else if status.contains("all instruments expired") {
        "all instruments expired"
    } else if status.contains("negative time") {
        "negative time"
    } else {
        return false;
    };
    text.contains(key)
}

fn group(rows: &[Vec<String>]) -> BTreeMap<&str, Vec<&Vec<String>>> {
    let mut map: BTreeMap<&str, Vec<&Vec<String>>> = BTreeMap::new();
    for r in rows {
        map.entry(r[0].as_str()).or_default().push(r);
    }
    map
}

#[test]
fn tonar_curves_match_quantlib_r_package() {
    let cases = golden_rows("ois_tonar_cases.csv");
    let instruments_rows = golden_rows("ois_tonar_instruments.csv");
    let nodes_rows = golden_rows("ois_tonar_nodes.csv");
    let discounts_rows = golden_rows("ois_tonar_discounts.csv");
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
    let (mut max_repricing_ql, mut max_repricing) = (0.0_f64, 0.0_f64);

    for r in &cases {
        let case = r[0].as_str();
        let status = r[9].as_str();
        let result = build_ois_curve(&spec(r));
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
                if !same_cause(s, &e) {
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
                    max_repricing_ql = max_repricing_ql.max((golden - q.rate).abs());
                    max_repricing = max_repricing.max((v - q.rate).abs());
                    if !s_implied.close(implied, v, 1e-12) {
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
            if !s_nodes.close(&g[2], *v, 1e-12) {
                bad.push(format!(
                    "case {case} node {d}: QuantLib {}, Riemann {v}",
                    g[2]
                ));
            }
        }

        for g in discounts.get(case).into_iter().flatten() {
            let got = curve.discount(date(&g[1]));
            errors_in_discounts += usize::from(g[2] == "ERROR");
            let ok = match (g[2].as_str(), &got) {
                // the golden file has no messages: before the reference date the time is
                // negative, otherwise the date is past the curve without extrapolation
                ("ERROR", Err(Error::InvalidInput { reason })) => {
                    date(&g[1]) < date(&r[2]) && reason.contains("negative time")
                }
                ("ERROR", Err(Error::OutsideCurveRange { .. })) => {
                    date(&g[1]) >= date(&r[2]) && r[4] == "0"
                }
                ("ERROR", _) | (_, Err(_)) => false,
                (golden, Ok(v)) => s_discounts.close(golden, *v, 1e-12),
            };
            if !ok {
                bad.push(format!(
                    "case {case} date {}: QuantLib {}, Riemann {got:?}",
                    g[1], g[2]
                ));
            }
        }
    }

    eprintln!("cases {} built {built}; statuses {causes:?}", cases.len());
    eprintln!("instrument dates checked {date_checks}");
    for (name, s) in [
        ("nodes", &s_nodes),
        ("discounts", &s_discounts),
        ("implied", &s_implied),
    ] {
        eprintln!(
            "{name:10} {:6} exact {:6} inexact (max rel {:e}, max abs {:e})",
            s.exact, s.inexact, s.max_rel, s.max_abs
        );
    }
    eprintln!("discount errors matched {errors_in_discounts}");
    eprintln!("max |implied - quote|: QuantLib {max_repricing_ql:e}, Riemann {max_repricing:e}");
    assert!(max_repricing <= 1e-9, "repricing error {max_repricing:e}");
    assert!(
        bad.is_empty(),
        "{} mismatches; first:\n{}",
        bad.len(),
        bad[..bad.len().min(10)].join("\n")
    );
}
