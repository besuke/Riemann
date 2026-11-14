//! Market VaR, ES and backtests against tests/golden/market_risk.csv (tools/golden/p13_market_risk.R):
//! LagrangeFinance I-9's equal-weight portfolio, R's quantile(), pchisq() and pbinom().

mod common;

use common::golden_rows;
use riemann::market_risk::{
    compute_loss_risk, compute_monte_carlo_risk, compute_rolling_var, compute_traffic_light, compute_var_backtest, MonteCarloSettings,
    RollingMethod, TrafficLight, VarBacktest,
};

struct Golden {
    rows: Vec<(String, String, usize, String, f64)>,
}

impl Golden {
    fn load() -> Golden {
        Golden {
            rows: golden_rows("market_risk.csv")
                .into_iter()
                .map(|r| (r[0].clone(), r[1].clone(), r[2].parse().unwrap(), r[3].clone(), r[4].parse().unwrap()))
                .collect(),
        }
    }
    fn vector(&self, case: &str, kind: &str, j: &str) -> Vec<f64> {
        let mut v: Vec<(usize, f64)> = self.rows.iter().filter(|r| r.0 == case && r.1 == kind && r.3 == j).map(|r| (r.2, r.4)).collect();
        v.sort_by_key(|x| x.0);
        v.into_iter().map(|x| x.1).collect()
    }
    fn scalar(&self, case: &str, kind: &str, j: &str) -> f64 {
        let v = self.vector(case, kind, j);
        assert_eq!(v.len(), 1, "{case} {kind} {j}");
        v[0]
    }
    fn matrix(&self, case: &str, kind: &str, n: usize) -> Vec<Vec<f64>> {
        let mut m = vec![vec![0.0; n]; n];
        for r in self.rows.iter().filter(|r| r.0 == case && r.1 == kind) {
            m[r.2][r.3.parse::<usize>().unwrap()] = r.4;
        }
        m
    }
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs().max(1e-300)
}

fn check_backtest(b: &VarBacktest, g: &Golden, kind: &str, prefix: &str) {
    let s = |name: &str| g.scalar("lf_i9", kind, &format!("{prefix}{name}"));
    assert_eq!(b.observations as f64, s("observations"));
    assert_eq!(b.exceptions as f64, s("exceptions"));
    assert_eq!([b.n00, b.n01, b.n10, b.n11].map(|v| v as f64), [s("n00"), s("n01"), s("n10"), s("n11")]);
    for (name, value) in [
        ("exception_rate", b.exception_rate),
        ("kupiec_lr", b.kupiec_lr),
        ("kupiec_p_value", b.kupiec_p_value),
        ("independence_lr", b.independence_lr),
        ("independence_p_value", b.independence_p_value),
        ("conditional_coverage_lr", b.conditional_coverage_lr),
        ("conditional_coverage_p_value", b.conditional_coverage_p_value),
    ] {
        assert!(rel(value, s(name)) < 1e-12, "{kind} {prefix}{name}: {value} vs {}", s(name));
    }
}

#[test]
fn var_and_es_of_the_manuscript_portfolio() {
    let g = Golden::load();
    let losses = g.vector("lf_i9", "loss", "");
    assert_eq!(losses.len(), 71);
    for alpha in [0.95, 0.975, 0.99] {
        let r = compute_loss_risk(&losses, alpha).unwrap();
        for (name, value) in [
            ("gaussian_var", r.gaussian_var),
            ("gaussian_es", r.gaussian_es),
            ("var_type7", r.historical_var),
            ("var_type1", r.historical_var_type1),
            ("tail_mean_es", r.tail_mean_es),
            ("ru_es", r.historical_es),
        ] {
            let reference = g.scalar("lf_i9", "measure", &format!("{name}@{alpha}"));
            assert!(rel(value, reference) < 1e-13, "{name}@{alpha}: {value} vs {reference}");
        }
    }
}

#[test]
fn backtests_static_and_rolling() {
    let g = Golden::load();
    let losses = g.vector("lf_i9", "loss", "");
    let var = compute_loss_risk(&losses, 0.95).unwrap().historical_var;
    check_backtest(&compute_var_backtest(&losses, &[var], 0.95).unwrap(), &g, "static_backtest", "");
    let window = g.scalar("lf_i9", "parameter", "window") as usize;
    for (name, method) in [("historical", RollingMethod::Historical), ("gaussian", RollingMethod::Gaussian)] {
        let v = compute_rolling_var(&losses, window, 0.95, method).unwrap();
        let reference = g.vector("lf_i9", "rolling_var", name);
        assert_eq!(v.len(), reference.len());
        for (a, b) in v.iter().zip(&reference) {
            assert!(rel(*a, *b) < 1e-13, "{name} rolling VaR {a} vs {b}");
        }
        check_backtest(&compute_var_backtest(&losses[window..], &v, 0.95).unwrap(), &g, "rolling_backtest", &format!("{name}:"));
    }
}

#[test]
fn basel_traffic_light() {
    let g = Golden::load();
    let cumulative = g.vector("basel", "traffic_light_cumulative", "");
    let zones = g.vector("basel", "traffic_light_zone", "");
    for k in 0..cumulative.len() {
        let (zone, p) = compute_traffic_light(k, 250, 0.99).unwrap();
        assert!(rel(p, cumulative[k]) < 1e-12, "k = {k}: {p} vs {}", cumulative[k]);
        let expected = [TrafficLight::Green, TrafficLight::Yellow, TrafficLight::Red][zones[k] as usize];
        assert_eq!(zone, expected, "k = {k}");
    }
    // the familiar boundaries: green up to 4, yellow 5 to 9, red from 10
    assert_eq!(compute_traffic_light(4, 250, 0.99).unwrap().0, TrafficLight::Green);
    assert_eq!(compute_traffic_light(9, 250, 0.99).unwrap().0, TrafficLight::Yellow);
    assert_eq!(compute_traffic_light(10, 250, 0.99).unwrap().0, TrafficLight::Red);
}

#[test]
fn monte_carlo_agrees_with_the_gaussian_values_and_the_manuscript() {
    let g = Golden::load();
    // univariate, as the manuscript: the portfolio's mean and standard deviation
    let losses = g.vector("lf_i9", "loss", "");
    let r = compute_loss_risk(&losses, 0.95).unwrap();
    let n = losses.len() as f64;
    let mean_return = -losses.iter().sum::<f64>() / n;
    let sd = (losses.iter().map(|l| (-l - mean_return).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
    let settings = MonteCarloSettings { scenarios: 100_000, seed: 20260918, batches: 20, threads: 0 };
    let mc = compute_monte_carlo_risk(&[1.0], &[mean_return], &[vec![sd * sd]], 0.95, &settings).unwrap();
    assert!((mc.var - r.gaussian_var).abs() < 4.0 * mc.var_standard_error, "{} vs {} (se {})", mc.var, r.gaussian_var, mc.var_standard_error);
    assert!((mc.es - r.gaussian_es).abs() < 4.0 * mc.es_standard_error);
    let manuscript = g.scalar("lf_i9", "measure", "manuscript_monte_carlo_var@0.95");
    // two independent estimates of the same quantile: their difference has standard error about sqrt(2) se
    assert!((mc.var - manuscript).abs() < 4.0 * 2f64.sqrt() * mc.var_standard_error, "{} vs manuscript {manuscript}", mc.var);

    // multivariate: I-8's 20 assets, equal weights
    let mean = g.vector("lf_i8_moments", "mean", "");
    let cov = g.matrix("lf_i8_moments", "covariance", mean.len());
    let w = vec![1.0 / 20.0; 20];
    for alpha in [0.95, 0.99] {
        let mc = compute_monte_carlo_risk(&w, &mean, &cov, alpha, &settings).unwrap();
        let var = g.scalar("lf_i8_moments", "measure", &format!("gaussian_var@{alpha}"));
        let es = g.scalar("lf_i8_moments", "measure", &format!("gaussian_es@{alpha}"));
        assert!((mc.var - var).abs() < 4.0 * mc.var_standard_error, "VaR@{alpha} {} vs {var}", mc.var);
        assert!((mc.es - es).abs() < 4.0 * mc.es_standard_error, "ES@{alpha} {} vs {es}", mc.es);
        eprintln!("multivariate @{alpha}: VaR {} (Gaussian {var}, se {}), ES {} (Gaussian {es}, se {})", mc.var, mc.var_standard_error, mc.es, mc.es_standard_error);
    }
    eprintln!("univariate: VaR {} (Gaussian {}, manuscript MC {manuscript}, se {})", mc.var, r.gaussian_var, mc.var_standard_error);
}

#[test]
fn rejects_bad_inputs() {
    assert!(compute_loss_risk(&[0.1], 0.95).is_err());
    assert!(compute_loss_risk(&[0.1, f64::NAN], 0.95).is_err());
    assert!(compute_var_backtest(&[0.1, 0.2, 0.3], &[0.1, 0.2], 0.95).is_err());
    assert!(compute_rolling_var(&[0.1, 0.2, 0.3], 3, 0.95, RollingMethod::Historical).is_err());
    assert!(compute_traffic_light(3, 2, 0.99).is_err());
}
