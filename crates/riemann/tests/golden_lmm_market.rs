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
    let mut worst = [0.0f64; 4];
    let mut counts = [0usize; 4];
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
        for (a, b, v) in of("swap_rate") {
            check(2, compute_lmm_swap_rate(&spec, a, b, &forwards).unwrap().0, v, format!("swap rate {a},{b}"));
        }
        for (a, b, v) in of("discount_ratio") {
            check(3, spec.discount(a) / spec.discount(b), v, format!("discount ratio {a},{b}"));
        }
    }
    assert_eq!(counts, [93, 93, 58, 58]);
    println!("worst relative differences: correlation {:e}, drift {:e}, swap rate {:e}, discount ratio {:e}", worst[0], worst[1], worst[2], worst[3]);
}
