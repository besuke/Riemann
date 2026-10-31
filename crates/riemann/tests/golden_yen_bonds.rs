//! Compares the Japanese bond conventions with golden values from GaussQuant's
//! `bond_*_YEN_GQL` (`tools/golden/p2_yen_bonds.R`). Remaining years must match exactly;
//! simple-yield values exactly or within a relative 1e-15; compound-yield values, which use
//! pow, within 1e-12. Errors must match. Inexact matches are counted per value and reported.

mod common;

use common::{date, golden_rows, Stats};
use riemann::yen_bond::{self, CompoundFrequencies, YenBondRisk};
use riemann::Error;
use std::collections::BTreeMap;

/// Per-value statistics: exact and inexact matches, with the tolerance of each value.
#[derive(Default)]
struct Checks {
    stats: BTreeMap<&'static str, Stats>,
    bad: Vec<String>,
}

impl Checks {
    fn check(
        &mut self,
        case: &str,
        name: &'static str,
        golden: &str,
        got: Result<f64, Error>,
        rel: f64,
    ) {
        let stats = self.stats.entry(name).or_default();
        let ok = match (golden, &got) {
            ("ERROR", Err(_)) => true,
            ("ERROR", Ok(_)) | (_, Err(_)) => false,
            (g, Ok(v)) => stats.close(g, *v, rel),
        };
        if !ok {
            self.bad.push(format!(
                "case {case} {name}: GaussQuant {golden}, Riemann {got:?}"
            ));
        }
    }
}

fn risk_fields(r: &YenBondRisk) -> [f64; 6] {
    [
        r.price,
        r.dp_dy_pct,
        r.d2p_dy2_pct,
        r.bpv_yen,
        r.modified_duration,
        r.convexity,
    ]
}

const SIMPLE_RISK: [&str; 6] = [
    "risk_s_price",
    "risk_s_d1",
    "risk_s_d2",
    "risk_s_bpv_yen",
    "risk_s_mod_dur",
    "risk_s_convexity",
];
const COMPOUND_RISK: [&str; 6] = [
    "risk_c_price",
    "risk_c_d1",
    "risk_c_d2",
    "risk_c_bpv_yen",
    "risk_c_mod_dur",
    "risk_c_convexity",
];

#[test]
fn yen_bonds_match_gaussquant() {
    let rows = golden_rows("yen_bonds.csv");
    let mut c = Checks::default();
    let mut rule_differs = 0;
    let num = |s: &str| s.parse::<f64>().unwrap();
    for r in &rows {
        let case = r[0].as_str();
        // remaining years: the rule of docs/spec_bonds.md 5.1, as in GaussQuant
        c.check(
            case,
            "remaining_years",
            &r[11],
            yen_bond::remaining_years(date(&r[1]), date(&r[2])),
            0.0,
        );
        rule_differs += usize::from(r[13] == "1");
        // the formulas, with GaussQuant's remaining years
        let t = num(&r[11]);
        let (coupon, redemption, y_s, y_c, accrued, target) = (
            num(&r[3]),
            num(&r[4]),
            num(&r[5]),
            num(&r[6]),
            num(&r[9]),
            num(&r[10]),
        );
        let freq = CompoundFrequencies {
            coupon: r[7].parse().unwrap(),
            compound: r[8].parse().unwrap(),
        };
        c.check(
            case,
            "price_simple",
            &r[14],
            yen_bond::price_from_simple_rate(t, coupon, y_s, redemption, None),
            1e-15,
        );
        c.check(
            case,
            "price_simple_3",
            &r[15],
            yen_bond::price_from_simple_rate(t, coupon, y_s, redemption, Some(3)),
            1e-15,
        );
        c.check(
            case,
            "rate_simple",
            &r[16],
            yen_bond::simple_rate_from_price(t, coupon, target, redemption, 7, 100),
            1e-15,
        );
        c.check(
            case,
            "price_compound",
            &r[17],
            yen_bond::price_from_compound_yield(t, coupon, y_c, redemption, freq, None),
            1e-12,
        );
        c.check(
            case,
            "price_compound_3",
            &r[18],
            yen_bond::price_from_compound_yield(t, coupon, y_c, redemption, freq, Some(3)),
            1e-12,
        );
        c.check(
            case,
            "rate_compound",
            &r[19],
            yen_bond::compound_yield_from_price(t, coupon, target, redemption, freq, 7, 100),
            1e-12,
        );
        let risk_s = yen_bond::risk_from_simple_rate(t, coupon, y_s, redemption, accrued);
        let risk_c = yen_bond::risk_from_compound_yield(t, coupon, y_c, redemption, freq, accrued);
        for i in 0..6 {
            let s = risk_s
                .as_ref()
                .map(|v| risk_fields(v)[i])
                .map_err(Clone::clone);
            let k = risk_c
                .as_ref()
                .map(|v| risk_fields(v)[i])
                .map_err(Clone::clone);
            c.check(case, SIMPLE_RISK[i], &r[20 + i], s, 1e-15);
            c.check(case, COMPOUND_RISK[i], &r[26 + i], k, 1e-12);
        }
    }
    eprintln!(
        "rows {} (rule differs from the market-practice description in {rule_differs})",
        rows.len()
    );
    for (name, s) in &c.stats {
        eprintln!(
            "  {name:18} {:5} exact {:4} inexact (max rel {:e})",
            s.exact, s.inexact, s.max_rel
        );
    }
    assert!(
        c.bad.is_empty(),
        "{} mismatches; first:\n{}",
        c.bad.len(),
        c.bad[..c.bad.len().min(8)].join("\n")
    );
}
