//! Builds TONAR curves from a CSV for `tools/verify/verify_ois.R`, until the R entry point of P4.
//!
//! Usage: `verify_ois <input.csv> <output.csv>`. The first 9 input columns are those of
//! `tests/golden/ois_tonar_cases.csv`; then interpolation (LogLinear or MonotonicLogCubic) and
//! the probe date serials separated by spaces. Each output row is case, interpolation, kind,
//! key (a date serial, or the instrument position for implied_rate and pillar_date), value;
//! a value is a number or ERROR. A curve that cannot be built gives the single kind `build`.

#[path = "../tests/common/mod.rs"]
mod common;

use common::{date, day_count, tonar_curve_spec};
use riemann::interest_rate::{Compounding, InterestRate};
use riemann::ois::{build_ois_curve, OisCurve, OisInterpolation};
use riemann::Error;

fn cell(value: Result<f64, Error>) -> String {
    value.map_or_else(|_| "ERROR".to_string(), |v| v.to_string())
}

fn rate_cell(value: Result<InterestRate, Error>) -> String {
    cell(value.map(|r| r.rate))
}

fn value_rows(r: &[String]) -> Vec<(String, i64, String)> {
    let interpolation = match r[9].as_str() {
        "LogLinear" => OisInterpolation::LogLinear,
        "MonotonicLogCubic" => OisInterpolation::MonotonicLogCubic,
        other => panic!("unknown interpolation {other}"),
    };
    let OisCurve {
        curve,
        quote_repricing,
    } = match build_ois_curve(&tonar_curve_spec(r, interpolation)) {
        Ok(built) => built,
        Err(_) => return vec![("build".to_string(), 0, "ERROR".to_string())],
    };
    let curve = curve.with_extrapolation(r[4] == "1");
    let dc = day_count(&r[3]);
    let mut rows = Vec::new();
    for (position, q) in quote_repricing.iter().enumerate() {
        let implied = q
            .implied_rate
            .map_or("ERROR".to_string(), |v| v.to_string());
        rows.push(("implied_rate".to_string(), position as i64, implied));
        rows.push((
            "pillar_date".to_string(),
            position as i64,
            q.pillar_date.serial().to_string(),
        ));
    }
    let (node_dates, node_values) = curve.discount_nodes().expect("a discount curve");
    for (d, v) in node_dates.iter().zip(node_values) {
        rows.push((
            "node_discount".to_string(),
            i64::from(d.serial()),
            v.to_string(),
        ));
    }
    for serial in r[10].split(' ') {
        let d = date(serial);
        let next = date(&(serial.parse::<i64>().unwrap() + 1).to_string());
        let key: i64 = serial.parse().unwrap();
        let continuous = Compounding::Continuous;
        rows.push(("discount".to_string(), key, cell(curve.discount(d))));
        rows.push((
            "zero_continuous".to_string(),
            key,
            rate_cell(curve.zero_rate(d, &dc, continuous, 1)),
        ));
        rows.push((
            "forward_1d_simple".to_string(),
            key,
            rate_cell(curve.forward_rate(d, next, &dc, Compounding::Simple, 1)),
        ));
        rows.push((
            "forward_instantaneous".to_string(),
            key,
            rate_cell(curve.forward_rate(d, d, &dc, continuous, 1)),
        ));
    }
    rows
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert_eq!(args.len(), 3, "usage: verify_ois <input.csv> <output.csv>");
    let input = std::fs::read_to_string(&args[1]).expect("cannot read the input file");
    let mut lines = vec!["case,interpolation,kind,key,value".to_string()];
    for line in input.lines().skip(1) {
        let r: Vec<String> = line.split(',').map(|s| s.trim().to_string()).collect();
        for (kind, key, value) in value_rows(&r) {
            lines.push(format!("{},{},{kind},{key},{value}", r[0], r[9]));
        }
    }
    std::fs::write(&args[2], lines.join("\n") + "\n").expect("cannot write the output file");
}
