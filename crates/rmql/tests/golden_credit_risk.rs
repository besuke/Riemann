//! The one-factor Gaussian credit portfolio against tests/golden/credit_risk.csv (tools/golden/p14_credit_risk.R):
//! LagrangeFinance I-10's 100 obligors, the exact distribution by another quadrature in R, the manuscript's Monte
//! Carlo, the ASRF formula and the Basel IRB capital.

mod common;

use common::golden_rows;
use rmql::credit_risk::{
    compute_asrf_var, compute_conditional_pd, compute_credit_loss_distribution, compute_distribution_risk, compute_expected_credit_loss,
    compute_irb_capital, simulate_credit_losses, Obligor,
};
use rmql::market_risk::compute_loss_risk;

struct Golden(Vec<(String, String, usize, String, f64)>);

impl Golden {
    fn load() -> Golden {
        Golden(golden_rows("credit_risk.csv").into_iter().map(|r| (r[0].clone(), r[1].clone(), r[2].parse().unwrap(), r[3].clone(), r[4].parse().unwrap())).collect())
    }
    fn vector(&self, case: &str, kind: &str, j: &str) -> Vec<f64> {
        let mut v: Vec<(usize, f64)> = self.0.iter().filter(|r| r.0 == case && r.1 == kind && r.3 == j).map(|r| (r.2, r.4)).collect();
        v.sort_by_key(|x| x.0);
        v.into_iter().map(|x| x.1).collect()
    }
    fn scalar(&self, case: &str, kind: &str, j: &str) -> f64 {
        let v = self.vector(case, kind, j);
        assert_eq!(v.len(), 1, "{case} {kind} {j}");
        v[0]
    }
}

fn portfolio(g: &Golden, rho: f64) -> Vec<Obligor> {
    g.vector("lf_i10", "pd", "").into_iter().map(|pd| Obligor { pd, ead: 100.0, lgd: 0.6, rho }).collect()
}

const RHOS: [f64; 4] = [0.0, 0.1, 0.2, 0.4];

#[test]
fn conditional_pd_and_expected_loss() {
    let g = Golden::load();
    for r in g.0.iter().filter(|r| r.1 == "conditional_pd") {
        let z: f64 = r.3.parse().unwrap();
        let p = compute_conditional_pd(0.02, 0.2, z);
        // QuantLib's normal distribution function (kept for compatibility) loses relative accuracy in the far tail:
        // the argument reaches -4.3 at z = 4, where it differs from R's pnorm by 2.8e-12 relative
        assert!((p - r.4).abs() / r.4 < 1e-11, "z = {z}: {p} vs {}", r.4);
    }
    let el = compute_expected_credit_loss(&portfolio(&g, 0.2));
    assert!((el - g.scalar("lf_i10", "measure", "expected_loss")).abs() < 1e-12);
}

#[test]
fn the_exact_distribution_matches_the_other_quadrature() {
    let g = Golden::load();
    let mut worst = 0.0_f64;
    for rho in RHOS {
        let tag = format!("rho={rho}");
        let d = compute_credit_loss_distribution(&portfolio(&g, rho), 60.0, 200).unwrap();
        let reference = g.vector("lf_i10", "distribution", &tag);
        assert_eq!(d.probability.len(), reference.len());
        for (k, (a, b)) in d.probability.iter().zip(&reference).enumerate() {
            worst = worst.max((a - b).abs());
            assert!((a - b).abs() < 1e-13, "{tag} P(L = {}): {a} vs {b}", 60 * k);
        }
        let s = |name: &str| g.scalar("lf_i10", "exact", &format!("{name}:{tag}"));
        assert!((d.probability.iter().sum::<f64>() - 1.0).abs() < 1e-13);
        assert!((d.mean() - s("mean")).abs() < 1e-9 && (d.standard_deviation() - s("sd")).abs() < 1e-8);
        for alpha in [0.95, 0.99, 0.999] {
            let r = compute_distribution_risk(&d, alpha).unwrap();
            assert_eq!(r.var, s(&format!("var@{alpha}")), "{tag} VaR@{alpha}");
            assert!((r.es - s(&format!("es@{alpha}"))).abs() < 1e-7, "{tag} ES@{alpha}: {} vs {}", r.es, s(&format!("es@{alpha}")));
            assert!((r.tail_mean_es - s(&format!("tail_mean_es@{alpha}"))).abs() < 1e-7);
        }
        for alpha in [0.99, 0.999] {
            let asrf = compute_asrf_var(&portfolio(&g, rho), alpha).unwrap();
            let reference = g.scalar("lf_i10", "asrf", &format!("var@{alpha}:{tag}"));
            assert!((asrf - reference).abs() / reference < 1e-13, "{tag} ASRF@{alpha}");
        }
    }
    eprintln!("largest difference of a probability from the trapezoidal rule: {worst:e}");
}

#[test]
fn monte_carlo_agrees_with_the_exact_distribution_and_the_manuscript() {
    let g = Golden::load();
    let obligors = portfolio(&g, 0.2);
    let losses = simulate_credit_losses(&obligors, 100_000, 20260918, 0).unwrap();
    let n = losses.len() as f64;
    let mean = losses.iter().sum::<f64>() / n;
    let sd = (losses.iter().map(|l| (l - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
    let exact = |name: &str| g.scalar("lf_i10", "exact", &format!("{name}:rho=0.2"));
    let manuscript = |name: &str| g.scalar("lf_i10", "manuscript_mc", name);
    // the mean within 4 standard errors of the exact mean, and of the manuscript's (two independent estimates)
    assert!((mean - exact("mean")).abs() < 4.0 * sd / n.sqrt(), "{mean} vs {}", exact("mean"));
    assert!((mean - manuscript("mean")).abs() < 4.0 * 2f64.sqrt() * sd / n.sqrt());
    let d = compute_credit_loss_distribution(&obligors, 60.0, 200).unwrap();
    for alpha in [0.95, 0.99, 0.999] {
        // the lattice VaR: a sample quantile lands on the exact lattice point or a neighbour
        let mc = compute_loss_risk(&losses, alpha).unwrap();
        let exact_var = compute_distribution_risk(&d, alpha).unwrap().var;
        assert!((mc.historical_var_type1 - exact_var).abs() <= 60.0, "VaR@{alpha}: {} vs {exact_var}", mc.historical_var_type1);
        eprintln!(
            "VaR@{alpha}: exact {exact_var}, rmql MC {} (type 7) {} (type 1), manuscript {}; tail mean ES exact {}, MC {}, manuscript {}",
            mc.historical_var,
            mc.historical_var_type1,
            manuscript(&format!("var@{alpha}")),
            compute_distribution_risk(&d, alpha).unwrap().tail_mean_es,
            mc.tail_mean_es,
            manuscript(&format!("es@{alpha}"))
        );
    }
    // the manuscript's correlation sensitivity (20,000 scenarios each): its type-7 VaR is within one lattice step of the
    // exact one at 99%, and within two at 99.9%, where only 20 scenarios lie above it
    for rho in RHOS {
        let d = compute_credit_loss_distribution(&portfolio(&g, rho), 60.0, 200).unwrap();
        for alpha in [0.99, 0.999] {
            let exact_var = compute_distribution_risk(&d, alpha).unwrap().var;
            let m = g.scalar("lf_i10", "manuscript_sensitivity", &format!("var@{alpha}:rho={rho}"));
            let steps = if alpha == 0.99 { 1.0 } else { 2.0 };
            assert!((m - exact_var).abs() <= steps * 60.0, "rho {rho} VaR@{alpha}: manuscript {m} vs exact {exact_var}");
        }
    }
}

#[test]
fn basel_irb_capital() {
    let g = Golden::load();
    let pds = g.vector("irb", "input", "pd");
    for (name, maturity) in [("capital", 2.5), ("capital_m5", 5.0)] {
        let reference = g.vector("irb", "output", name);
        for (pd, k) in pds.iter().zip(&reference) {
            let c = compute_irb_capital(*pd, 0.45, maturity, 1.0).unwrap();
            assert!((c.capital - k).abs() / k < 1e-12, "PD {pd} M {maturity}: {} vs {k}", c.capital);
            assert!((c.risk_weighted_assets - 12.5 * c.capital).abs() < 1e-15);
        }
    }
    for (pd, (r, b)) in pds.iter().zip(g.vector("irb", "output", "correlation").iter().zip(g.vector("irb", "output", "maturity_adjustment"))) {
        let c = compute_irb_capital(*pd, 0.45, 2.5, 1.0).unwrap();
        assert!((c.correlation - r).abs() < 1e-15 && (c.maturity_adjustment - b).abs() < 1e-15);
    }
}

#[test]
fn rejects_bad_inputs() {
    let ok = Obligor { pd: 0.01, ead: 100.0, lgd: 0.6, rho: 0.2 };
    assert!(compute_credit_loss_distribution(&[ok], 7.0, 64).is_err()); // 60 is not a whole number of 7
    assert!(compute_credit_loss_distribution(&[Obligor { pd: 0.0, ..ok }], 60.0, 64).is_err());
    assert!(compute_credit_loss_distribution(&[Obligor { rho: 1.0, ..ok }], 60.0, 64).is_err());
    assert!(compute_irb_capital(0.0, 0.45, 2.5, 1.0).is_err());
}
