//! The FX smile against the QuantLib R package (tools/golden/p10_fx_smile.R): BlackDeltaCalculator's
//! strikes, deltas and ATM strikes, and BlackVolatilitySurfaceDelta's Black volatilities for each delta
//! and ATM type. The switch to long-term conventions is checked by joining two of QuantLib's surfaces.
//! All inputs are synthetic.

mod common;

use common::golden_rows;
use riemann::curves::YieldCurve;
use riemann::fx_smile::{
    FxAtmType, FxDeltaCalculator, FxDeltaType, FxSmileConventions, FxSmileSurface,
    SmileInterpolation,
};
use riemann::interest_rate::{Compounding, InterestRate};
use riemann::options::OptionType;
use riemann::time::{Actual365Kind, DayCount};
use riemann::Date;
use std::collections::BTreeMap;

fn delta_type(s: &str) -> FxDeltaType {
    match s {
        "Spot" => FxDeltaType::Spot,
        "Fwd" => FxDeltaType::Forward,
        "PaSpot" => FxDeltaType::PremiumAdjustedSpot,
        "PaFwd" => FxDeltaType::PremiumAdjustedForward,
        other => panic!("{other}"),
    }
}

fn atm_type(s: &str) -> FxAtmType {
    match s {
        "AtmSpot" => FxAtmType::Spot,
        "AtmFwd" => FxAtmType::Forward,
        "AtmDeltaNeutral" => FxAtmType::DeltaNeutral,
        "AtmPutCall50" => FxAtmType::PutCall50,
        other => panic!("{other}"),
    }
}

#[test]
fn delta_calculator_matches_quantlib() {
    let rows = golden_rows("fx_delta.csv");
    assert_eq!(rows.len(), 540);
    let mut worst: BTreeMap<String, f64> = BTreeMap::new();
    for r in &rows {
        let n = |i: usize| r[i].parse::<f64>().unwrap();
        let calculator = FxDeltaCalculator {
            option: if r[0] == "Call" {
                OptionType::Call
            } else {
                OptionType::Put
            },
            delta_type: delta_type(&r[1]),
            spot: n(2),
            domestic_discount: n(3),
            foreign_discount: n(4),
            std_dev: n(5),
        };
        let (kind, input, expected) = (r[6].as_str(), n(7), n(8));
        let got = match kind {
            "strike_from_delta" => calculator.strike_from_delta(input).unwrap(),
            "delta_from_strike" => calculator.delta_from_strike(input).unwrap(),
            atm => calculator
                .atm_strike(atm_type(atm.trim_start_matches("atm_")))
                .unwrap(),
        };
        let error = (got - expected).abs() / expected.abs().max(1.0);
        let key = format!(
            "{kind} {}",
            if calculator.delta_type == FxDeltaType::PremiumAdjustedSpot
                || calculator.delta_type == FxDeltaType::PremiumAdjustedForward
            {
                "premium-adjusted"
            } else {
                "plain"
            }
        );
        let w = worst.entry(key).or_insert(0.0);
        *w = w.max(error);
        // Brent's method stops within 1e-10 of the root in both
        let tolerance = if kind == "strike_from_delta" && r[1].starts_with("Pa") {
            1e-12
        } else {
            1e-14
        };
        assert!(error < tolerance, "{r:?}: {got} against {expected}");
    }
    println!("{worst:#?}");
}

fn flat(rate: f64) -> YieldCurve {
    let dc = DayCount::Actual365Fixed(Actual365Kind::Standard);
    YieldCurve::flat(
        Date::from_ymd(2026, 11, 2).unwrap(),
        InterestRate::new(rate, dc, Compounding::Continuous, 0).unwrap(),
    )
}

struct Case {
    settings: BTreeMap<String, String>,
    market: BTreeMap<String, f64>,
    expiries: Vec<f64>,
    vols: Vec<Vec<f64>>,
    expected: Vec<(f64, f64, f64)>,
}

fn cases() -> BTreeMap<String, Case> {
    let mut out: BTreeMap<String, Case> = BTreeMap::new();
    for r in golden_rows("fx_smile.csv") {
        let c = out.entry(r[0].clone()).or_insert_with(|| Case {
            settings: BTreeMap::new(),
            market: BTreeMap::new(),
            expiries: Vec::new(),
            vols: Vec::new(),
            expected: Vec::new(),
        });
        let n = |i: usize| r[i].parse::<f64>().unwrap();
        match r[1].as_str() {
            "setting" => {
                c.settings.insert(r[2].clone(), r[3].clone());
            }
            "market" => {
                c.market.insert(r[2].clone(), n(4));
            }
            "expiry" => c.expiries.push(n(4)),
            "vol" => {
                let i = n(2) as usize;
                if c.vols.len() <= i {
                    c.vols.push(Vec::new());
                }
                c.vols[i].push(n(4));
            }
            "black_vol" => c.expected.push((n(2), n(3), n(4))),
            // read by prdc_notes_on_quantlib_surfaces
            "prdc_rate" | "prdc_npv" => {}
            other => panic!("{other}"),
        }
    }
    out
}

fn conventions(c: &Case) -> FxSmileConventions {
    let dt = delta_type(&c.settings["delta_type"]);
    FxSmileConventions {
        delta_type: dt,
        atm_type: atm_type(&c.settings["atm_type"]),
        atm_delta_type: dt,
    }
}

fn surface<'c>(c: &Case, d: &'c YieldCurve, f: &'c YieldCurve) -> FxSmileSurface<'c> {
    FxSmileSurface {
        spot: c.market["spot"],
        domestic: d,
        foreign: f,
        expiries: c.expiries.clone(),
        put_deltas: vec![-0.1, -0.25],
        call_deltas: vec![0.25, 0.1],
        has_atm: c.settings["has_atm"] == "TRUE",
        volatilities: c.vols.clone(),
        conventions: conventions(c),
        long_term: None,
        interpolation: SmileInterpolation::Linear,
        flat_strike_extrapolation: false,
    }
}

#[test]
fn delta_surface_matches_quantlib() {
    let cases = cases();
    assert_eq!(cases.len(), 7);
    let (d, f) = (flat(0.012), flat(0.035));
    let mut count = 0;
    for (name, c) in &cases {
        assert_eq!(
            (c.market["domestic_rate"], c.market["foreign_rate"]),
            (0.012, 0.035)
        );
        let s = surface(c, &d, &f);
        let mut worst = 0.0f64;
        for &(t, k, expected) in &c.expected {
            let got = s.volatility(t, k).unwrap();
            worst = worst.max((got - expected).abs());
            assert!(
                (got - expected).abs() < 1e-12,
                "{name} t {t} k {k}: {got} against {expected}"
            );
            count += 1;
        }
        println!("{name}: worst {worst:e}");
    }
    assert_eq!(count, 7 * 12 * 14);
}

#[test]
fn prdc_notes_on_quantlib_surfaces() {
    let cases = cases();
    let (d, f) = (flat(0.012), flat(0.035));
    for (name, c) in &cases {
        let rows: Vec<Vec<String>> = golden_rows("fx_smile.csv")
            .into_iter()
            .filter(|r| &r[0] == name && r[1].starts_with("prdc"))
            .collect();
        let rates: Vec<(f64, f64, f64)> = rows
            .iter()
            .filter(|r| r[1] == "prdc_rate")
            .map(|r| {
                (
                    r[2].parse().unwrap(),
                    r[3].parse().unwrap(),
                    r[4].parse().unwrap(),
                )
            })
            .collect();
        let npv: f64 = rows.iter().find(|r| r[1] == "prdc_npv").unwrap()[4]
            .parse()
            .unwrap();
        let terms = riemann::prdc::PrdcTerms {
            notional: 1.0,
            redemption: 1.0,
            coupon_times: rates.iter().map(|r| r.0).collect(),
            accrual_fractions: rates.iter().map(|r| r.1).collect(),
            fixed_rates: vec![None; rates.len()],
            fx_base: 150.0,
            leverage: 0.8,
            subtraction: 0.78,
            floor: 0.0,
            cap: 0.08,
        };
        let s = riemann::prdc::compute_prdc_smile_value(&terms, &surface(c, &d, &f)).unwrap();
        for (coupon, r) in s.coupons.iter().zip(&rates) {
            assert!(
                (coupon.rate - r.2).abs() < 1e-15,
                "{name} {}: {} against {}",
                r.0,
                coupon.rate,
                r.2
            );
        }
        assert!(
            (s.npv - npv).abs() < 1e-14,
            "{name}: {} against {npv}",
            s.npv
        );
    }
}

#[test]
fn the_switch_joins_two_quantlib_surfaces() {
    let cases = cases();
    let (short, long) = (&cases["spot_dn"], &cases["pafwd_dn"]);
    let (d, f) = (flat(0.012), flat(0.035));
    // the 1-year switch of QuantLib's convention: Actual/365 from 2026-11-02 to 2027-11-02
    let switch = 1.0;
    let joined = FxSmileSurface {
        long_term: Some((switch, conventions(long))),
        ..surface(short, &d, &f)
    };
    for (a, b) in short.expected.iter().zip(&long.expected) {
        assert_eq!((a.0, a.1), (b.0, b.1));
        let expected = if a.0 < switch { a.2 } else { b.2 };
        let got = joined.volatility(a.0, a.1).unwrap();
        assert!(
            (got - expected).abs() < 1e-12,
            "t {} k {}: {got} against {expected}",
            a.0,
            a.1
        );
    }
}
