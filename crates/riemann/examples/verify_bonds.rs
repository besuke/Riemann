//! Values bonds from a CSV for `tools/verify/verify_bonds.R`, until the R entry point of P4.
//!
//! Usage: `verify_bonds <input.csv> <output.csv>`. The first 20 input columns are those of
//! `tests/golden/bond_cases.csv`; then evaluation_date, rate, yield_day_counter, compounding,
//! frequency, price, price_type. Each output value is a number or ERROR.

#[path = "../tests/common/mod.rs"]
mod common;

use common::{bond_spec, date, day_count};
use riemann::cashflow_analysis::{DurationType, YieldConvention, YieldSolver};
use riemann::instruments::{FixedRateBond, PriceType};
use riemann::interest_rate::Compounding;
use riemann::Error;

fn cell(value: Result<f64, Error>) -> String {
    value.map_or_else(|_| "ERROR".to_string(), |v| v.to_string())
}

fn value_row(r: &[String]) -> Result<Vec<String>, Error> {
    let bond = FixedRateBond::new(bond_spec(r))?;
    let settlement = bond.settlement_date(date(&r[20]))?;
    let compounding = match r[23].as_str() {
        "Simple" => Compounding::Simple,
        "Compounded" => Compounding::Compounded,
        "Continuous" => Compounding::Continuous,
        other => panic!("unknown compounding {other}"),
    };
    let convention = YieldConvention {
        day_count: day_count(&r[22]),
        compounding,
        frequency: r[24].parse().expect("frequency"),
    };
    let y = convention.rate(r[21].parse().expect("rate"))?;
    let price_type = if r[26] == "Clean" {
        PriceType::Clean
    } else {
        PriceType::Dirty
    };
    let price = r[25].parse().expect("price");
    Ok(vec![
        settlement.serial().to_string(),
        cell(bond.clean_price(&y, settlement)),
        cell(bond.dirty_price(&y, settlement)),
        cell(bond.accrued_amount(settlement)),
        cell(bond.duration(&y, DurationType::Modified, settlement)),
        cell(bond.convexity(&y, settlement)),
        cell(bond.basis_point_value(&y, settlement)),
        cell(bond.yield_from_price(
            price,
            price_type,
            &convention,
            settlement,
            YieldSolver::default(),
        )),
    ])
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert_eq!(
        args.len(),
        3,
        "usage: verify_bonds <input.csv> <output.csv>"
    );
    let input = std::fs::read_to_string(&args[1]).expect("cannot read the input file");
    let mut lines = vec![
        "row,settlement_date,clean_price,dirty_price,accrued,duration_modified,convexity,bpv,yield"
            .to_string(),
    ];
    for line in input.lines().skip(1) {
        let r: Vec<String> = line.split(',').map(|s| s.trim().to_string()).collect();
        let values = value_row(&r).unwrap_or_else(|_| vec!["ERROR".to_string(); 8]);
        lines.push(format!("{},{}", r[27], values.join(",")));
    }
    std::fs::write(&args[2], lines.join("\n") + "\n").expect("cannot write the output file");
}
