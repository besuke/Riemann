//! Compares day count conventions with golden values from the QuantLib R binding
//! (`tools/golden/p1_day_counts.R`). Day counts and errors must match exactly; year
//! fractions must match exactly or, failing that, within a relative 1e-15
//! (specification 5.1, arithmetic-only calculations). The number of inexact matches is reported.

use riemann::time::{
    Actual365Kind, ActualActualKind, Calendar, DayCount, HolidayCalendar, Thirty360Kind,
};
use riemann::Date;
use std::path::PathBuf;

fn golden_rows(file: &str) -> (String, Vec<Vec<String>>) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden").join(file);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}; run tools/golden/p1_day_counts.R", path.display()));
    let header = text.lines().next().unwrap_or_default().to_string();
    let rows = text
        .lines()
        .filter(|l| !l.starts_with('#'))
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').map(|s| s.trim().to_string()).collect())
        .collect();
    (header, rows)
}

fn day_counter(name: &str, termination: Date) -> DayCount {
    use DayCount::*;
    match name {
        "actual_360" => Actual360 { include_last_day: false },
        "actual_360_inc" => Actual360 { include_last_day: true },
        "actual_364" => Actual364,
        "actual_36525" => Actual36525 { include_last_day: false },
        "actual_36525_inc" => Actual36525 { include_last_day: true },
        "actual_366" => Actual366 { include_last_day: false },
        "actual_366_inc" => Actual366 { include_last_day: true },
        "actual_365_fixed" => Actual365Fixed(Actual365Kind::Standard),
        "actual_365_canadian" => Actual365Fixed(Actual365Kind::Canadian),
        "actual_365_no_leap" => Actual365Fixed(Actual365Kind::NoLeap),
        "act_act_isda" | "act_act_historical" | "act_act_actual365" => ActualActual(ActualActualKind::Isda),
        "act_act_isma" | "act_act_bond" => ActualActual(ActualActualKind::Isma),
        "act_act_afb" | "act_act_euro" => ActualActual(ActualActualKind::Afb),
        "thirty_360_usa" => Thirty360(Thirty360Kind::Usa),
        "thirty_360_bond_basis" | "thirty_360_isma" => Thirty360(Thirty360Kind::BondBasis),
        "thirty_360_european" | "thirty_360_eurobond_basis" => Thirty360(Thirty360Kind::European),
        "thirty_360_italian" => Thirty360(Thirty360Kind::Italian),
        "thirty_360_german" | "thirty_360_isda" => Thirty360(Thirty360Kind::Isda { termination: Some(termination) }),
        "thirty_360_nasd" => Thirty360(Thirty360Kind::Nasd),
        "thirty_365" => Thirty365,
        "one" => One,
        "simple" => Simple,
        "business_252_japan" => Business252(HolidayCalendar::from(Calendar::Japan)),
        other => panic!("unknown day counter {other}"),
    }
}

fn date(s: &str) -> Option<Date> {
    match s {
        "0" => None,
        _ => Some(Date::from_serial(s.parse().unwrap()).unwrap()),
    }
}

#[test]
fn day_counts_match_quantlib() {
    let (header, rows) = golden_rows("day_counts.csv");
    let termination: i64 = header.rsplit(' ').next().unwrap().parse().expect("termination serial in header");
    let termination = Date::from_serial(termination).unwrap();

    let mut bad = Vec::new();
    let (mut exact, mut inexact, mut max_rel) = (0usize, 0usize, 0.0_f64);
    for r in &rows {
        let dc = day_counter(&r[0], termination);
        let (d1, d2) = (date(&r[1]).unwrap(), date(&r[2]).unwrap());
        let (rs, re) = (date(&r[3]), date(&r[4]));
        let count = dc.day_count(d1, d2).unwrap().to_string();
        if r[5] != count {
            bad.push(format!("{} {d1}->{d2}: dayCount QuantLib {} Riemann {count}", r[0], r[5]));
        }
        match (r[6].as_str(), dc.year_fraction(d1, d2, rs, re)) {
            ("ERROR", Err(_)) => {}
            ("ERROR", Ok(v)) => bad.push(format!("{} {d1}->{d2} ref {rs:?}-{re:?}: QuantLib ERROR, Riemann {v}", r[0])),
            (g, Err(e)) => bad.push(format!("{} {d1}->{d2} ref {rs:?}-{re:?}: QuantLib {g}, Riemann error {e}", r[0])),
            (g, Ok(v)) => {
                let q: f64 = g.parse().unwrap();
                if q == v {
                    exact += 1;
                } else {
                    let rel = (q - v).abs() / q.abs().max(1e-300);
                    inexact += 1;
                    max_rel = max_rel.max(rel);
                    if rel > 1e-15 {
                        bad.push(format!("{} {d1}->{d2} ref {rs:?}-{re:?}: QuantLib {g}, Riemann {v:.17e} (rel {rel:e})", r[0]));
                    }
                }
            }
        }
    }
    eprintln!("year fractions: {exact} exact, {inexact} inexact (max relative difference {max_rel:e})");
    assert!(
        bad.is_empty(),
        "{} of {} rows differ; first:\n{}",
        bad.len(),
        rows.len(),
        bad[..bad.len().min(10)].join("\n")
    );
}
