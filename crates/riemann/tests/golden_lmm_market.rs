//! The forward market model against the QuantLib R package's market-model framework
//! (tools/golden/p13_lmm_market.R): the exponential correlation, the spot-measure drift and the swap
//! rates and discount ratios of a curve state. All inputs are synthetic.

mod common;

use common::golden_rows;
use riemann::lmm_market::{compute_lmm_drift, compute_lmm_swap_rate, create_exponential_correlation, LmmSpec};

#[test]
fn market_model_pieces_match_quantlib() {
    let rows = golden_rows("lmm_market.csv");
    let cases: std::collections::BTreeSet<String> = rows.iter().map(|r| r[0].clone()).collect();
    assert_eq!(cases.len(), 6);
    let mut worst = [0.0f64; 5];
    let mut counts = [0usize; 5];
    for case in &cases {
        let of = |kind: &str| -> Vec<(usize, usize, f64)> {
            rows.iter()
                .filter(|r| &r[0] == case && r[1] == kind)
                .map(|r| (r[2].parse().unwrap(), r[3].parse().unwrap(), r[4].parse().unwrap()))
                .collect()
        };
        let values = |kind: &str| -> Vec<f64> { of(kind).iter().map(|x| x.2).collect() };
        let times = values("time");
        let forwards = values("forward");
        let beta = values("beta")[0];
        let n = forwards.len();
        let spec = LmmSpec {
            correlation: create_exponential_correlation(&times[..n], beta).unwrap(),
            times,
            forwards: forwards.clone(),
            volatilities: values("volatility"),
            discount_start: 1.0,
            backward_looking: false,
            model: riemann::options::ForwardModel::Black,
            volatility_grid: None,
            factors: None,
            measure: riemann::lmm_market::Measure::Spot,
            discretization: riemann::lmm_market::Discretization::Euler,
        };
        let mut check = |slot: usize, ours: f64, theirs: f64, what: String| {
            let error = (ours - theirs).abs() / theirs.abs().max(1e-3);
            assert!(error < 1e-12, "case {case} {what}: {ours} against {theirs}");
            worst[slot] = worst[slot].max(error);
            counts[slot] += 1;
        };
        for (i, j, v) in of("correlation") {
            check(0, spec.correlation[i * n + j], v, format!("correlation {i},{j}"));
        }
        for (alive, k, v) in of("drift") {
            let drift = compute_lmm_drift(&spec, alive, &forwards).unwrap();
            // QuantLib leaves the drifts of fixed rates at 0
            check(1, drift[k], v, format!("drift alive {alive} rate {k}"));
        }
        let terminal = LmmSpec { measure: riemann::lmm_market::Measure::Terminal, ..spec.clone() };
        for (alive, k, v) in of("drift_terminal") {
            let drift = compute_lmm_drift(&terminal, alive, &forwards).unwrap();
            check(4, drift[k], v, format!("terminal drift alive {alive} rate {k}"));
        }
        for (a, b, v) in of("swap_rate") {
            check(2, compute_lmm_swap_rate(&spec, a, b, &forwards).unwrap().0, v, format!("swap rate {a},{b}"));
        }
        for (a, b, v) in of("discount_ratio") {
            check(3, spec.discount(a) / spec.discount(b), v, format!("discount ratio {a},{b}"));
        }
    }
    assert_eq!(counts, [93, 93, 58, 58, 93]);
    println!(
        "worst relative differences: correlation {:e}, drift {:e}, swap rate {:e}, discount ratio {:e}, terminal drift {:e}",
        worst[0], worst[1], worst[2], worst[3], worst[4]
    );
}

/// Fewer factors against QuantLib (tools/golden/p13_lmm_factors.R): the eigenvalues of the correlation,
/// the reduced correlation of rankReducedSqrt's pseudo-root, and the drifts with that pseudo-root
/// (computePlain and computeReduced).
#[test]
fn reduced_factors_match_quantlib() {
    use riemann::lmm_market::create_reduced_correlation;
    use riemann::math::linear::compute_symmetric_eigen;
    let rows = golden_rows("lmm_factors.csv");
    let cases: std::collections::BTreeSet<String> = rows.iter().map(|r| r[0].clone()).collect();
    assert_eq!(cases.len(), 6);
    let mut worst = [0.0f64; 4];
    let mut counts = [0usize; 4];
    for case in &cases {
        let of = |factors: usize, kind: &str| -> Vec<(usize, usize, f64)> {
            rows.iter()
                .filter(|r| &r[0] == case && r[1] == factors.to_string() && r[2] == kind)
                .map(|r| (r[3].parse().unwrap(), r[4].parse().unwrap(), r[5].parse().unwrap()))
                .collect()
        };
        let values = |kind: &str| -> Vec<f64> { of(0, kind).iter().map(|x| x.2).collect() };
        let times = values("time");
        let forwards = values("forward");
        let n = forwards.len();
        let full = create_exponential_correlation(&times[..n], values("beta")[0]).unwrap();
        let (eigen, _) = compute_symmetric_eigen(&full.chunks(n).map(<[f64]>::to_vec).collect::<Vec<_>>()).unwrap();
        for (i, _, v) in of(0, "eigenvalue") {
            let error = (eigen[i] - v).abs();
            assert!(error < 1e-13, "case {case} eigenvalue {i}: {} against {v}", eigen[i]);
            worst[0] = worst[0].max(error);
            counts[0] += 1;
        }
        let mut factor_counts: Vec<usize> = vec![1, 2, 3, n];
        factor_counts.dedup();
        for m in factor_counts {
            let reduced = create_reduced_correlation(&full, n, m).unwrap();
            let spec = LmmSpec {
                correlation: reduced.clone(),
                times: times.clone(),
                forwards: forwards.clone(),
                volatilities: values("volatility"),
                discount_start: 1.0,
                backward_looking: false,
                model: riemann::options::ForwardModel::Black,
                volatility_grid: None,
                factors: Some(m),
                measure: riemann::lmm_market::Measure::Spot,
                discretization: riemann::lmm_market::Discretization::Euler,
            };
            for (i, j, v) in of(m, "correlation") {
                let error = (reduced[i * n + j] - v).abs();
                assert!(error < 1e-12, "case {case} m {m} correlation {i},{j}: {} against {v}", reduced[i * n + j]);
                worst[1] = worst[1].max(error);
                counts[1] += 1;
            }
            for (slot, kind) in [(2usize, "drift_plain"), (3, "drift_reduced")] {
                for (alive, k, v) in of(m, kind) {
                    let drift = compute_lmm_drift(&spec, alive, &forwards).unwrap();
                    let error = (drift[k] - v).abs() / v.abs().max(1e-3);
                    assert!(error < 1e-12, "case {case} m {m} {kind} alive {alive} rate {k}: {} against {v}", drift[k]);
                    worst[slot] = worst[slot].max(error);
                    counts[slot] += 1;
                }
            }
        }
    }
    assert_eq!(counts, [42, 1200, 1200, 1200]);
    println!("counts {counts:?}; worst: eigenvalue {:e} (absolute), reduced correlation {:e} (absolute), drift plain {:e}, reduced {:e}", worst[0], worst[1], worst[2], worst[3]);
}
