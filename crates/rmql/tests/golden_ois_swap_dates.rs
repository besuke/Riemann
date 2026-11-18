//! The dates of an OIS swap made from a tenor, against QuantLib's `MakeOIS(swapTenor, index)` with
//! settlement days (`tools/golden/p11_ois_swap_dates.R`): 4 indices, 6 spans of evaluation dates (one is the 2026-09-10 of Carmine Studio) that
//! cover weekends, holidays and month ends, 13 tenors, settlement days 0 to 3. The effective and the
//! maturity date (the termination date adjusted modified following) must match
//! exactly. Where QuantLib cannot build the swap (a schedule with a single date), the swap built from
//! our dates must fail as well.

mod common;

use common::{date, golden_rows};
use rmql::names;
use rmql::ois::compute_ois_swap_dates;
use rmql::ois_swap::{create_ois_swap, OisSwapSpec, SwapSide};
use rmql::time::{Period, TimeUnit};

fn unit(name: &str) -> TimeUnit {
    match name {
        "Days" => TimeUnit::Days,
        "Weeks" => TimeUnit::Weeks,
        "Months" => TimeUnit::Months,
        "Years" => TimeUnit::Years,
        other => panic!("unknown unit {other}"),
    }
}

#[test]
fn dates_from_a_tenor_against_quantlib() {
    let rows = golden_rows("ois_swap_dates.csv");
    assert_eq!(rows.len(), 13_520);
    let (mut matched, mut failed) = (0, 0);
    for r in &rows {
        let index = names::parse_overnight_index(&r[0]).unwrap();
        let tenor = Period::new(r[2].parse().unwrap(), unit(&r[3]));
        let (dates, _) = compute_ois_swap_dates(&index, date(&r[1]), tenor, r[4].parse().unwrap()).unwrap();
        let (effective, termination) = (dates.effective_date, dates.termination_date);
        if r[5] == "ok" {
            assert_eq!(effective, date(&r[6]), "effective date in {r:?}");
            assert_eq!(dates.maturity_date, date(&r[7]), "maturity date in {r:?}");
            matched += 1;
        } else {
            let built = create_ois_swap(OisSwapSpec {
                side: SwapSide::Payer,
                nominal: 1.0,
                effective_date: effective,
                termination_date: termination,
                index,
                fixed_rate: 0.01,
                fixed_day_count: None,
                spread: 0.0,
                fixed_frequency: 1,
                overnight_frequency: 1,
                payment_lag: 0,
            });
            assert!(built.is_err(), "QuantLib fails and we do not in {r:?}");
            failed += 1;
        }
    }
    assert_eq!((matched, failed), (13_514, 6));
}
