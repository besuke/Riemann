//! Compares fixed-rate bond cash flows and accrued interest with golden values from the
//! QuantLib R binding (`tools/golden/p2_fixed_rate_bonds.R`). Dates must match exactly;
//! amounts exactly or within a relative 1e-15 (specification 7.3), with inexact matches counted.

use riemann::cashflows::CashFlow;
use riemann::instruments::{FixedRateBond, FixedRateBondSpec};
use riemann::time::{
    ActualActualKind, Actual365Kind, BusinessDayConvention, Calendar, DateGenerationRule, DayCount,
    HolidayCalendar, Period, ScheduleSpec, Thirty360Kind, TimeUnit, UsMarket,
};
use riemann::Date;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn golden_rows(file: &str) -> Vec<Vec<String>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden").join(file);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}; run tools/golden/p2_fixed_rate_bonds.R", path.display()));
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').map(|s| s.trim().to_string()).collect())
        .collect()
}

fn date(s: &str) -> Date {
    Date::from_serial(s.parse().unwrap()).unwrap()
}

fn optional_date(s: &str) -> Option<Date> {
    if s == "0" {
        None
    } else {
        Some(date(s))
    }
}

fn convention(name: &str) -> BusinessDayConvention {
    use BusinessDayConvention::*;
    match name {
        "Following" => Following,
        "ModifiedFollowing" => ModifiedFollowing,
        "Preceding" => Preceding,
        "Unadjusted" => Unadjusted,
        other => panic!("unknown convention {other}"),
    }
}

fn spec(r: &[String]) -> FixedRateBondSpec {
    let calendar = HolidayCalendar::from(match r[5].as_str() {
        "japan" => Calendar::Japan,
        "target" => Calendar::Target,
        "us_sofr" => Calendar::UnitedStates(UsMarket::Sofr),
        "null_calendar" => Calendar::NullCalendar,
        other => panic!("unknown calendar {other}"),
    });
    let unit = match r[4].as_str() {
        "Months" => TimeUnit::Months,
        "Years" => TimeUnit::Years,
        other => panic!("unknown unit {other}"),
    };
    let day_count = match r[13].as_str() {
        "actual_365_fixed" => DayCount::Actual365Fixed(Actual365Kind::Standard),
        "actual_360" => DayCount::Actual360 { include_last_day: false },
        "thirty_360_bond_basis" => DayCount::Thirty360(Thirty360Kind::BondBasis),
        "act_act_isma" => DayCount::ActualActual(ActualActualKind::Isma),
        "act_act_isda" => DayCount::ActualActual(ActualActualKind::Isda),
        other => panic!("unknown day counter {other}"),
    };
    FixedRateBondSpec {
        settlement_days: r[16].parse().unwrap(),
        face_amount: r[17].parse().unwrap(),
        schedule: ScheduleSpec {
            effective_date: date(&r[1]),
            termination_date: date(&r[2]),
            tenor: Period::new(r[3].parse().unwrap(), unit),
            calendar,
            convention: convention(&r[6]),
            termination_convention: convention(&r[7]),
            rule: if r[8] == "Backward" { DateGenerationRule::Backward } else { DateGenerationRule::Forward },
            end_of_month: r[9] == "1",
            first_date: optional_date(&r[10]),
            next_to_last_date: optional_date(&r[11]),
        },
        coupon_rate: r[12].parse().unwrap(),
        day_count,
        payment_convention: convention(&r[14]),
        redemption: r[15].parse().unwrap(),
        issue_date: optional_date(&r[18]),
        payment_calendar: None,
    }
}

#[derive(Default)]
struct Stats {
    exact: usize,
    inexact: usize,
    max_rel: f64,
}

impl Stats {
    /// Compares a golden number with a Riemann value; true if within a relative 1e-15.
    fn close(&mut self, golden: &str, value: f64) -> bool {
        let q: f64 = golden.parse().unwrap();
        if q == value {
            self.exact += 1;
            return true;
        }
        let rel = (q - value).abs() / q.abs().max(1e-300);
        self.inexact += 1;
        self.max_rel = self.max_rel.max(rel);
        rel <= 1e-15
    }
}

#[test]
fn fixed_rate_bonds_match_quantlib() {
    let cases = golden_rows("bond_cases.csv");
    let mut flows: BTreeMap<usize, Vec<Vec<String>>> = BTreeMap::new();
    for r in golden_rows("bond_cashflows.csv") {
        flows.entry(r[0].parse().unwrap()).or_default().push(r);
    }
    let mut accrued: BTreeMap<usize, Vec<Vec<String>>> = BTreeMap::new();
    for r in golden_rows("bond_accrued.csv") {
        accrued.entry(r[0].parse().unwrap()).or_default().push(r);
    }

    let mut stats = Stats::default();
    let (mut n_flows, mut n_accrued, mut n_built) = (0, 0, 0);
    let mut bad = Vec::new();
    for c in &cases {
        let k: usize = c[0].parse().unwrap();
        let built = FixedRateBond::new(spec(c));
        let bond = match (c[19].as_str(), built) {
            ("0", Err(_)) => continue,
            ("0", Ok(_)) => {
                bad.push(format!("case {k}: QuantLib failed to build, Riemann built"));
                continue;
            }
            (_, Err(e)) => {
                bad.push(format!("case {k}: QuantLib built, Riemann error {e}"));
                continue;
            }
            (_, Ok(b)) => b,
        };
        n_built += 1;

        let golden_flows = &flows[&k];
        if golden_flows.len() != bond.cashflows.len() {
            bad.push(format!("case {k}: {} cash flows in QuantLib, {} in Riemann", golden_flows.len(), bond.cashflows.len()));
            continue;
        }
        for (g, cf) in golden_flows.iter().zip(&bond.cashflows) {
            n_flows += 1;
            let ok = match cf {
                CashFlow::FixedRateCoupon(cp) => {
                    g[1] == "coupon"
                        && date(&g[2]) == cp.payment_date
                        && stats.close(&g[3], cp.amount().unwrap())
                        && stats.close(&g[4], cp.nominal)
                        && stats.close(&g[5], cp.rate)
                        && date(&g[6]) == cp.accrual_start
                        && date(&g[7]) == cp.accrual_end
                        && date(&g[8]) == cp.ref_start
                        && date(&g[9]) == cp.ref_end
                }
                CashFlow::Redemption { date: d, amount } => {
                    g[1] == "redemption" && date(&g[2]) == *d && stats.close(&g[3], *amount)
                }
            };
            if !ok {
                bad.push(format!("case {k}: QuantLib {g:?}, Riemann {cf:?}"));
            }
        }

        for a in accrued.get(&k).map(|v| v.as_slice()).unwrap_or(&[]) {
            n_accrued += 1;
            let evaluation = date(&a[1]);
            let settlement = bond.settlement_date(evaluation);
            let ok = match (a[2].as_str(), &settlement) {
                ("ERROR", Err(_)) => true,
                (_, Ok(s)) if a[2] != "ERROR" => {
                    date(&a[2]) == *s
                        && stats.close(&a[3], bond.notional(*s))
                        && stats.close(&a[4], bond.accrued_amount(*s).unwrap())
                }
                _ => false,
            };
            if !ok {
                bad.push(format!(
                    "case {k} evaluation {evaluation}: QuantLib {:?}, Riemann settlement {settlement:?} notional {:?} accrued {:?}",
                    &a[2..],
                    settlement.as_ref().map(|s| bond.notional(*s)),
                    settlement.as_ref().map(|s| bond.accrued_amount(*s))
                ));
            }
        }
    }
    eprintln!(
        "bonds built: {n_built}, cash flows: {n_flows}, accrued checks: {n_accrued}; numbers: {} exact, {} inexact (max rel {:e})",
        stats.exact, stats.inexact, stats.max_rel
    );
    assert!(bad.is_empty(), "{} mismatches; first:\n{}", bad.len(), bad[..bad.len().min(8)].join("\n"));
}
