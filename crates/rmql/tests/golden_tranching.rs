//! Credit tranches and CMO waterfalls against tests/golden/tranching.csv (tools/golden/p15_tranching.R).

mod common;

use common::golden_rows;
use rmql::credit_risk::{compute_credit_loss_distribution, Obligor};
use rmql::tranche::{
    compute_lhp_expected_tranche_loss, compute_pac_schedule, compute_pac_support_cmo, compute_sequential_cmo, compute_tranche_legs,
    compute_tranche_loss, compute_tranche_risk, CmoTranche, HazardObligor,
};

struct Golden(Vec<(String, String, usize, String, f64)>);

impl Golden {
    fn load() -> Golden {
        Golden(golden_rows("tranching.csv").into_iter().map(|r| (r[0].clone(), r[1].clone(), r[2].parse().unwrap(), r[3].clone(), r[4].parse().unwrap())).collect())
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

const TRANCHES: [(&str, f64, f64); 4] = [("equity", 0.0, 0.03), ("mezzanine", 0.03, 0.07), ("junior", 0.07, 0.10), ("senior", 0.10, 1.0)];

fn pds() -> Vec<f64> {
    (0..100).map(|i| 0.005 + (0.05 - 0.005) * i as f64 / 99.0).collect()
}

fn max_diff(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max)
}

#[test]
fn tranche_losses_of_the_manuscript_example() {
    let g = Golden::load();
    let l: Vec<f64> = (0..=150).map(|k| k as f64 / 1000.0).collect();
    for (name, a, d) in TRANCHES {
        let ours: Vec<f64> = l.iter().map(|x| compute_tranche_loss(*x, a, d)).collect();
        assert!(max_diff(&ours, &g.vector("example", "tranche_loss", name)) < 1e-16, "{name}");
    }
}

#[test]
fn tranche_risk_on_the_exact_distribution_and_the_large_pool() {
    let g = Golden::load();
    let (mut worst_etl, mut worst_probability, mut worst_lhp) = (0.0_f64, 0.0_f64, 0.0_f64);
    for rho in [0.1, 0.2, 0.4] {
        let obligors: Vec<Obligor> = pds().into_iter().map(|pd| Obligor { pd, ead: 100.0, lgd: 0.6, rho }).collect();
        let d = compute_credit_loss_distribution(&obligors, 60.0, 200).unwrap();
        for (name, a, b) in TRANCHES {
            let r = compute_tranche_risk(&d, 10_000.0, a, b, 0.99).unwrap();
            let s = |m: &str| g.scalar("exact", "measure", &format!("{name}:{m}:rho={rho}"));
            worst_etl = worst_etl.max((r.expected_tranche_loss - s("expected_tranche_loss")).abs());
            worst_probability = worst_probability
                .max((r.hit_probability - s("hit_probability")).abs())
                .max((r.wipeout_probability - s("wipeout_probability")).abs());
            assert!((r.expected_tranche_loss - s("expected_tranche_loss")).abs() < 1e-15, "{name} rho {rho} ETL");
            assert!((r.hit_probability - s("hit_probability")).abs() < 1e-13, "{name} rho {rho} hit");
            assert!((r.wipeout_probability - s("wipeout_probability")).abs() < 1e-13, "{name} rho {rho} wipe-out");
            assert!((r.var - s("var@0.99")).abs() < 1e-17, "{name} rho {rho} VaR {} vs {}", r.var, s("var@0.99"));
            let lhp = compute_lhp_expected_tranche_loss(0.02, 0.6, rho, a, b, 50).unwrap();
            let reference = g.scalar("lhp", "expected_tranche_loss", &format!("{name}:rho={rho}"));
            worst_lhp = worst_lhp.max((lhp - reference).abs() / reference);
            assert!((lhp - reference).abs() / reference < 1e-11, "{name} rho {rho} LHP {lhp} vs {reference}");
        }
    }
    eprintln!("largest differences: ETL {worst_etl:e}, hit/wipe-out probability {worst_probability:e}, LHP ETL relative {worst_lhp:e}");
}

/// The tranches 0-3-7-10-100% split every portfolio loss exactly, so their expected losses add up to the
/// portfolio's expected loss rate, and their hit and wipe-out probabilities chain (a tranche is wiped out when the
/// next one is hit, or when the loss stops exactly at the boundary).
#[test]
fn the_tranches_allocate_the_whole_loss() {
    for rho in [0.1, 0.2, 0.4] {
        let obligors: Vec<Obligor> = pds().into_iter().map(|pd| Obligor { pd, ead: 100.0, lgd: 0.6, rho }).collect();
        let d = compute_credit_loss_distribution(&obligors, 60.0, 200).unwrap();
        let mut worst_scenario = 0.0_f64;
        for k in 0..d.probability.len() {
            let l = (k as f64 * 60.0) / 10_000.0;
            let parts: f64 = TRANCHES.iter().map(|(_, a, b)| compute_tranche_loss(l, *a, *b)).sum();
            worst_scenario = worst_scenario.max((parts - l).abs());
        }
        assert!(worst_scenario < 1e-16, "rho {rho}: a scenario's tranche losses differ from its loss by {worst_scenario:e}");
        let r: Vec<_> = TRANCHES.iter().map(|(_, a, b)| compute_tranche_risk(&d, 10_000.0, *a, *b, 0.99).unwrap()).collect();
        let etl: f64 = r.iter().map(|x| x.expected_tranche_loss).sum();
        let portfolio = d.mean() / 10_000.0;
        assert!((etl - portfolio).abs() < 1e-16, "rho {rho}: sum of ETL {etl} vs E[l] {portfolio}");
        // P(l >= D_k) - P(l > A_{k+1}) is the mass at the boundary itself, P(l = D_k), when D_k lies on the lattice
        for k in 0..3 {
            let boundary = TRANCHES[k].2;
            let at: f64 = (0..d.probability.len()).filter(|&j| (j as f64 * 60.0) / 10_000.0 == boundary).map(|j| d.probability[j]).sum();
            let gap = r[k].wipeout_probability - r[k + 1].hit_probability;
            assert!((gap - at).abs() < 1e-15, "rho {rho}: tranche {k}: {gap} vs P(l = {boundary}) = {at}");
        }
        eprintln!("rho {rho}: sum of ETL {etl}, E[l] {portfolio}, difference {:e}; largest scenario difference {worst_scenario:e}", (etl - portfolio).abs());
    }
}

#[test]
fn legs_and_fair_spreads() {
    let g = Golden::load();
    let obligors: Vec<HazardObligor> = pds().into_iter().map(|pd| HazardObligor { hazard: -(1.0 - pd).ln(), ead: 100.0, lgd: 0.6, rho: 0.2 }).collect();
    let times = g.vector("legs", "time", "");
    let (mut worst_etl, mut worst_spread) = (0.0_f64, 0.0_f64);
    let mut loss_legs = 0.0;
    for (name, a, d) in TRANCHES {
        let legs = compute_tranche_legs(&obligors, 60.0, &times, 0.01, a, d, 200).unwrap();
        loss_legs += legs.loss_leg;
        worst_etl = worst_etl.max(max_diff(&legs.expected_tranche_loss, &g.vector("legs", "expected_tranche_loss", name)));
        worst_spread = worst_spread.max((legs.fair_spread - g.scalar("legs", "measure", &format!("{name}:fair_spread"))).abs() / g.scalar("legs", "measure", &format!("{name}:fair_spread")));
        assert!(max_diff(&legs.expected_tranche_loss, &g.vector("legs", "expected_tranche_loss", name)) < 1e-14, "{name} ETL(t)");
        let s = |m: &str| g.scalar("legs", "measure", &format!("{name}:{m}"));
        assert!((legs.premium_per_unit_spread - s("premium_per_unit_spread")).abs() / s("premium_per_unit_spread") < 1e-12);
        assert!((legs.loss_leg - s("loss_leg")).abs() / s("loss_leg") < 1e-11, "{name} loss leg");
        assert!((legs.fair_spread - s("fair_spread")).abs() / s("fair_spread") < 1e-11, "{name} spread {} vs {}", legs.fair_spread, s("fair_spread"));
    }
    // the four loss legs add up to the discounted expected loss of the whole portfolio
    let whole = compute_tranche_legs(&obligors, 60.0, &times, 0.01, 0.0, 1.0, 200).unwrap().loss_leg;
    assert!((loss_legs - whole).abs() < 1e-16, "sum of loss legs {loss_legs} vs whole portfolio {whole}");
    eprintln!("largest differences: ETL(t) {worst_etl:e}, fair spread relative {worst_spread:e}; loss legs {loss_legs} vs whole {whole}");
}

#[test]
fn cmo_waterfalls() {
    let g = Golden::load();
    let mut worst = 0.0_f64;
    let mut track = |d: f64| { worst = worst.max(d); d };
    let collateral = |psa: &str| (g.vector("cmo", "collateral_principal", &format!("psa={psa}")), g.vector("cmo", "collateral_interest", &format!("psa={psa}")));
    let seq = [400.0, 300.0, 200.0, 100.0].map(|balance| CmoTranche { balance, coupon: 0.05 });
    for psa in ["1", "3"] {
        let (p, i) = collateral(psa);
        let c = compute_sequential_cmo(&p, &i, &seq, true).unwrap();
        let tag = format!("sequential:psa={psa}");
        for k in 0..4 {
            assert!(track(max_diff(&c.principal[k], &g.vector("cmo", "principal", &format!("{tag}:{k}")))) < 1e-12, "{tag} principal {k}");
            assert!(track(max_diff(&c.interest[k], &g.vector("cmo", "interest", &format!("{tag}:{k}")))) < 1e-12, "{tag} interest {k}");
        }
        assert!(track(max_diff(&c.accrued[3], &g.vector("cmo", "accrued", &format!("{tag}:3")))) < 1e-12);
        assert!(track(max_diff(&c.residual, &g.vector("cmo", "residual", &tag))) < 1e-12);
        assert!(track(max_diff(&c.weighted_average_life(), &g.vector("cmo", "wal", &tag))) < 1e-12);
    }
    let (low, _) = collateral("1");
    let (high, _) = collateral("3");
    let schedule = compute_pac_schedule(&low, &high).unwrap();
    assert!(track(max_diff(&schedule, &g.vector("cmo", "pac_schedule", ""))) < 1e-15);
    for psa in ["0.5", "1.5", "4"] {
        let (p, i) = collateral(psa);
        let c = compute_pac_support_cmo(&p, &i, &schedule, CmoTranche { balance: 700.0, coupon: 0.05 }, CmoTranche { balance: 300.0, coupon: 0.05 }).unwrap();
        let tag = format!("pac:psa={psa}");
        for k in 0..2 {
            assert!(track(max_diff(&c.principal[k], &g.vector("cmo", "principal", &format!("{tag}:{k}")))) < 1e-12, "{tag} principal {k}");
            assert!(track(max_diff(&c.interest[k], &g.vector("cmo", "interest", &format!("{tag}:{k}")))) < 1e-12, "{tag} interest {k}");
        }
        assert!(track(max_diff(&c.residual, &g.vector("cmo", "residual", &tag))) < 1e-12);
        let wal = c.weighted_average_life();
        assert!(track(max_diff(&wal, &g.vector("cmo", "wal", &tag))) < 1e-12);
        eprintln!("PAC at {psa} x PSA: WAL PAC {:.3}, support {:.3}", wal[0], wal[1]);
    }
    eprintln!("largest CMO difference (principal, interest, accrual, residual, WAL, schedule): {worst:e}");
}
