//! Constrained portfolio optimization against tests/golden/portfolio.csv (tools/golden/p12_portfolio.R):
//! quadprog, Rglpk, closed forms and GaussQuant on LagrangeFinance I-8 and synthetic returns.

mod common;

use common::golden_rows;
use riemann::portfolio::{
    compute_efficient_frontier, compute_historical_es, compute_optimal_portfolio, compute_portfolio_risk, OptimalPortfolio,
    PortfolioConstraints, PortfolioMethod, ReturnMatrix,
};
use std::collections::BTreeMap;

#[derive(Default)]
struct Case {
    returns: BTreeMap<(usize, usize), f64>,
    lower: BTreeMap<usize, f64>,
    upper: BTreeMap<usize, f64>,
    parameter: BTreeMap<String, f64>,
    vectors: BTreeMap<(String, String), BTreeMap<usize, f64>>, // (kind, j) -> i -> value
    scalars: BTreeMap<(String, String), f64>,                  // (kind, j)
}

fn cases() -> BTreeMap<String, Case> {
    let mut out: BTreeMap<String, Case> = BTreeMap::new();
    for row in golden_rows("portfolio.csv") {
        let c = out.entry(row[0].clone()).or_default();
        let i: usize = row[2].parse().unwrap();
        let v: f64 = row[4].parse().unwrap();
        match row[1].as_str() {
            "return" => {
                c.returns.insert((i, row[3].parse().unwrap()), v);
            }
            "lower" => {
                c.lower.insert(i, v);
            }
            "upper" => {
                c.upper.insert(i, v);
            }
            "parameter" => {
                c.parameter.insert(row[3].clone(), v);
            }
            "risk" | "gq_value" => {
                c.scalars.insert((row[1].clone(), row[3].clone()), v);
            }
            kind => {
                c.vectors.entry((kind.to_string(), row[3].clone())).or_default().insert(i, v);
            }
        }
    }
    out
}

impl Case {
    fn matrix(&self) -> (Vec<f64>, usize) {
        let assets = self.lower.len();
        let periods = self.returns.len() / assets;
        let mut v = vec![0.0; periods * assets];
        for (&(t, j), &x) in &self.returns {
            v[t * assets + j] = x;
        }
        (v, assets)
    }
    fn constraints(&self) -> PortfolioConstraints {
        PortfolioConstraints {
            weight_sum: self.parameter["weight_sum"],
            lower: self.lower.values().copied().collect(),
            upper: self.upper.values().copied().collect(),
        }
    }
    fn vector(&self, kind: &str, j: &str) -> Option<Vec<f64>> {
        self.vectors.get(&(kind.to_string(), j.to_string())).map(|m| m.values().copied().collect())
    }
    fn bounded(&self) -> bool {
        self.lower.values().chain(self.upper.values()).any(|v| v.is_finite())
    }
}

fn max_diff(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max)
}

fn methods(c: &Case) -> Vec<(&'static str, PortfolioMethod)> {
    let p = &c.parameter;
    let n = c.lower.len();
    let mut v = vec![
        ("minimum_variance", PortfolioMethod::MinimumVariance),
        ("mean_variance", PortfolioMethod::MeanVariance { risk_aversion: p["risk_aversion"] }),
        ("maximum_sharpe", PortfolioMethod::MaximumSharpe { risk_free_rate: p["risk_free_rate"] }),
        ("target_return", PortfolioMethod::TargetReturn { target: p["target_return"] }),
        ("risk_budget", PortfolioMethod::RiskBudget { budget: vec![1.0; n] }),
    ];
    if c.bounded() {
        v.push(("maximum_return", PortfolioMethod::MaximumReturn));
        v.push(("minimum_ES", PortfolioMethod::MinimumExpectedShortfall { confidence_level: p["confidence_level"] }));
        v.push(("target_risk", PortfolioMethod::TargetVolatility { target: p["target_volatility"] }));
    }
    v
}

#[test]
fn optimal_weights_match_the_references() {
    let mut checked = 0;
    for (id, c) in cases() {
        let (values, assets) = c.matrix();
        let returns = ReturnMatrix { values: &values, assets };
        let constraints = c.constraints();
        for (name, method) in methods(&c) {
            let p = compute_optimal_portfolio(&returns, &constraints, &method).unwrap_or_else(|e| panic!("{id} {name}: {e}"));
            assert!((p.weights.iter().sum::<f64>() - 1.0).abs() < 1e-12, "{id} {name}: sum");
            for i in 0..assets {
                assert!(p.weights[i] >= constraints.lower[i] - 1e-12 && p.weights[i] <= constraints.upper[i] + 1e-12, "{id} {name}: bounds");
            }
            let reference = c.vector("weight", name).unwrap();
            if name == "minimum_ES" {
                // the linear program's optimum is unique in value, not always in weights
                let value = compute_historical_es(&returns, &p.weights, c.parameter["confidence_level"]);
                let diff = (value - c.parameter["minimum_es_value"]).abs();
                assert!(diff < 1e-12, "{id} {name}: ES {value} vs {}", c.parameter["minimum_es_value"]);
            } else {
                let diff = max_diff(&p.weights, &reference);
                assert!(diff < 1e-10, "{id} {name}: weights differ by {diff:e}\n{:?}\n{reference:?}", p.weights);
            }
            assert!(p.stationarity_residual < 1e-12, "{id} {name}: residual {:e}", p.stationarity_residual);
            checked += 1;
        }
    }
    assert_eq!(checked, 8 * 3 + 5 * 3);
}

#[test]
fn risk_measures_and_contributions_match_the_closed_forms() {
    let rel = |a: f64, b: f64| (a - b).abs() / b.abs().max(1.0);
    for (id, c) in cases() {
        let (values, assets) = c.matrix();
        let returns = ReturnMatrix { values: &values, assets };
        for (name, _) in methods(&c) {
            let w = c.vector("weight", name).unwrap();
            let r = compute_portfolio_risk(&returns, &w, c.parameter["confidence_level"]).unwrap();
            let stats = [
                ("expected_return", r.expected_return),
                ("volatility", r.volatility),
                ("normal_var", r.normal_var),
                ("normal_es", r.normal_es),
                ("historical_var", r.historical_var),
                ("historical_es", r.historical_es),
                ("tail_mean_es", r.tail_mean_es),
            ];
            for (stat, value) in stats {
                // At the minimum-ES weights many losses tie at the VaR by construction, so the mean of the
                // losses at or above the VaR (a discontinuous count) is not compared there.
                if name == "minimum_ES" && stat == "tail_mean_es" {
                    continue;
                }
                let reference = c.scalars[&("risk".to_string(), format!("{name}:{stat}"))];
                assert!(rel(value, reference) < 1e-13, "{id} {name} {stat}: {value} vs {reference}");
            }
            let contributions = [
                ("volatility", &r.volatility_contribution),
                ("normal_var", &r.normal_var_contribution),
                ("normal_es", &r.normal_es_contribution),
                ("historical_es", &r.historical_es_contribution),
            ];
            for (measure, parts) in contributions {
                let reference = c.vector("contribution", &format!("{name}:{measure}")).unwrap();
                assert!(max_diff(parts, &reference) < 1e-14, "{id} {name} {measure}: {parts:?} vs {reference:?}");
            }
        }
    }
}

#[test]
fn the_frontier_matches_the_target_return_programs() {
    let mut points = 0;
    for (id, c) in cases() {
        let (values, assets) = c.matrix();
        let returns = ReturnMatrix { values: &values, assets };
        let frontier: Vec<(f64, Vec<f64>)> = c
            .vectors
            .iter()
            .filter(|((kind, _), _)| kind == "frontier")
            .map(|((_, j), m)| (j.parse().unwrap(), m.values().copied().collect()))
            .collect();
        let targets: Vec<f64> = frontier.iter().map(|(t, _)| *t).collect();
        let solved = compute_efficient_frontier(&returns, &c.constraints(), &targets).unwrap();
        for ((t, w), p) in frontier.iter().zip(&solved) {
            assert!(max_diff(&p.weights, w) < 1e-10, "{id} frontier at {t}");
            points += 1;
        }
    }
    assert_eq!(points, 30 * 3);
}

fn gq(c: &Case, method: &str, stat: &str) -> f64 {
    c.scalars[&("gq_value".to_string(), format!("{method}:{stat}"))]
}

#[test]
fn gaussquant_exact_methods_agree_and_the_others_are_improved() {
    let all = cases();
    let c = &all["lf_i8"];
    let (values, assets) = c.matrix();
    let returns = ReturnMatrix { values: &values, assets };
    let constraints = c.constraints();
    let solve = |m: PortfolioMethod| -> OptimalPortfolio { compute_optimal_portfolio(&returns, &constraints, &m).unwrap() };
    let p = &c.parameter;

    // quadprog and glpk in GaussQuant: the same weights (GaussQuant's risk_aversion 5 is gamma 10)
    for (name, method) in [
        ("minimum_variance", PortfolioMethod::MinimumVariance),
        ("mean_variance", PortfolioMethod::MeanVariance { risk_aversion: p["risk_aversion"] }),
        ("maximum_return", PortfolioMethod::MaximumReturn),
        ("target_return", PortfolioMethod::TargetReturn { target: p["target_return"] }),
    ] {
        let diff = max_diff(&solve(method).weights, &c.vector("gq_weight", name).unwrap());
        assert!(diff < 1e-10, "GaussQuant {name}: {diff:e}");
    }
    // Nelder-Mead in GaussQuant: at least as good on GaussQuant's own objective
    let sharpe = solve(PortfolioMethod::MaximumSharpe { risk_free_rate: 0.0 }).objective;
    assert!(sharpe >= gq(c, "maximum_sharpe", "sharpe_ratio"), "{sharpe}");
    let alpha = p["confidence_level"];
    let es = solve(PortfolioMethod::MinimumExpectedShortfall { confidence_level: alpha }).objective;
    let es_gq = compute_historical_es(&returns, &c.vector("gq_weight", "minimum_ES").unwrap(), alpha);
    assert!(es <= es_gq, "{es} {es_gq}");
    let budget = solve(PortfolioMethod::RiskBudget { budget: vec![1.0; assets] });
    assert!(budget.objective < 1e-24, "{}", budget.objective);
    let risk = solve(PortfolioMethod::TargetVolatility { target: p["target_volatility"] });
    assert!(risk.volatility <= p["target_volatility"] * (1.0 + 1e-12));
    assert!(risk.expected_return >= gq(c, "target_risk", "expected_return") || gq(c, "target_risk", "volatility") > p["target_volatility"]);
    eprintln!(
        "lf_i8: Sharpe {sharpe} (GaussQuant {}), RU ES {es} (GaussQuant's weights {es_gq}), target-risk return {} (GaussQuant {})",
        gq(c, "maximum_sharpe", "sharpe_ratio"),
        risk.expected_return,
        gq(c, "target_risk", "expected_return")
    );
}

#[test]
fn rejects_inputs_it_cannot_solve() {
    let r = [0.01, 0.02, -0.01, 0.0, 0.02, 0.01];
    let returns = ReturnMatrix { values: &r, assets: 2 };
    let tight = PortfolioConstraints { weight_sum: 1.0, lower: vec![0.0, 0.0], upper: vec![0.4, 0.4] };
    assert!(compute_optimal_portfolio(&returns, &tight, &PortfolioMethod::MinimumVariance).is_err());
    let ok = PortfolioConstraints { weight_sum: 1.0, lower: vec![0.0, 0.0], upper: vec![1.0, 1.0] };
    assert!(compute_optimal_portfolio(&returns, &ok, &PortfolioMethod::TargetReturn { target: 1.0 }).is_err());
    assert!(compute_optimal_portfolio(&returns, &ok, &PortfolioMethod::MinimumExpectedShortfall { confidence_level: 1.0 }).is_err());
    let nan = [0.01, f64::NAN, -0.01, 0.0];
    assert!(compute_optimal_portfolio(&ReturnMatrix { values: &nan, assets: 2 }, &ok, &PortfolioMethod::MinimumVariance).is_err());
}
