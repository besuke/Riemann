//! Writes every holiday of the calendars in a CSV for `tools/verify/verify_calendars.R`, until
//! the R entry point of P4.
//!
//! Usage: `verify_calendars <input.csv> <output.csv>`. The input has a header and one calendar
//! key per line, as in `tests/golden/calendar_markets.csv`. Each output row is key, serial, kind:
//! H = holiday on a non-weekend day, W = business day on a weekend day, E = an error.

#[path = "../tests/common/mod.rs"]
mod common;

use common::calendar_by_key;
use riemann::time::Date;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert_eq!(
        args.len(),
        3,
        "usage: verify_calendars <input.csv> <output.csv>"
    );
    let input = std::fs::read_to_string(&args[1]).expect("cannot read the input file");
    let mut lines = vec!["key,serial,kind".to_string()];
    for key in input.lines().skip(1).map(str::trim) {
        let calendar = calendar_by_key(key);
        for serial in Date::MIN_SERIAL..=Date::MAX_SERIAL {
            let date = Date::from_serial(serial as i64).expect("a valid serial");
            let on_weekend = calendar.is_weekend(date.weekday());
            match calendar.is_business_day(date) {
                Ok(false) if !on_weekend => lines.push(format!("{key},{serial},H")),
                Ok(true) if on_weekend => lines.push(format!("{key},{serial},W")),
                Ok(_) => {}
                Err(_) => lines.push(format!("{key},{serial},E")),
            }
        }
    }
    std::fs::write(&args[2], lines.join("\n") + "\n").expect("cannot write the output file");
}
