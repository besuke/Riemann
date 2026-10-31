//! Sequential and parallel evaluation agree exactly (specification, section 6.3): for any number
//! of threads, when repeated, and for any order of the units. Results are compared through their
//! debug text, which writes every floating-point number exactly and includes errors.

mod common;

use common::{bond_spec, date, day_count, golden_rows, ois_curve_spec};
use riemann::batch::{
    bond_values_batch, curve_values_batch, map_parallel, BondEvaluation, CurveEvaluation,
};
use riemann::cashflow_analysis::YieldConvention;
use riemann::interest_rate::Compounding;
use riemann::ois::{estr, sofr, sonia, tonar, OisInterpolation, OvernightIndex};

const THREADS: [usize; 4] = [2, 4, 8, 0];

fn bond_units() -> Vec<BondEvaluation> {
    let conventions = [
        (Compounding::Compounded, 2),
        (Compounding::Continuous, 1),
        (Compounding::Simple, 1),
    ];
    golden_rows("bond_cases.csv")
        .iter()
        .filter(|r| r[19] == "1")
        .take(300)
        .enumerate()
        .map(|(i, r)| {
            let spec = bond_spec(r);
            let (compounding, frequency) = conventions[i % conventions.len()];
            BondEvaluation {
                evaluation_date: spec.schedule.effective_date,
                spec,
                rate: 0.002 + 0.0001 * i as f64,
                convention: YieldConvention {
                    day_count: day_count("actual_365_fixed"),
                    compounding,
                    frequency,
                },
            }
        })
        .collect()
}

fn curve_units(prefix: &str, index: OvernightIndex) -> Vec<CurveEvaluation> {
    let discounts = golden_rows(&format!("{prefix}_discounts.csv"));
    golden_rows(&format!("{prefix}_cases.csv"))
        .iter()
        .take(120)
        .map(|r| CurveEvaluation {
            spec: ois_curve_spec(r, index.clone(), OisInterpolation::MonotonicLogCubic),
            dates: discounts
                .iter()
                .filter(|d| d[0] == r[0])
                .map(|d| date(&d[1]))
                .collect(),
            day_count: day_count(&r[3]),
        })
        .collect()
}

fn all_curve_units() -> Vec<CurveEvaluation> {
    let mut units = curve_units("ois_tonar_log_cubic", tonar());
    units.extend(curve_units("ois_estr_log_cubic", estr()));
    units.extend(curve_units("ois_sofr_log_cubic", sofr()));
    units.extend(curve_units("ois_sonia_log_cubic", sonia()));
    units
}

fn exact_text<T: std::fmt::Debug>(items: &[T]) -> Vec<String> {
    items.iter().map(|x| format!("{x:?}")).collect()
}

#[test]
fn bonds_agree_for_any_number_of_threads() {
    let units = bond_units();
    assert!(units.len() > 100);
    let sequential = exact_text(&bond_values_batch(&units, 1).unwrap());
    for threads in THREADS {
        let parallel = exact_text(&bond_values_batch(&units, threads).unwrap());
        assert_eq!(sequential, parallel, "{threads} threads");
    }
}

#[test]
fn curves_agree_for_any_number_of_threads() {
    let units = all_curve_units();
    let sequential = exact_text(&curve_values_batch(&units, 1).unwrap());
    // both curves that build and curves that fail are covered
    assert!(sequential.iter().any(|r| r.starts_with("Ok")));
    assert!(sequential.iter().any(|r| r.starts_with("Err")));
    for threads in THREADS {
        let parallel = exact_text(&curve_values_batch(&units, threads).unwrap());
        assert_eq!(sequential, parallel, "{threads} threads");
    }
}

#[test]
fn repeated_runs_agree() {
    let units = all_curve_units();
    let first = exact_text(&curve_values_batch(&units, 8).unwrap());
    for _ in 0..3 {
        assert_eq!(first, exact_text(&curve_values_batch(&units, 8).unwrap()));
    }
}

#[test]
fn the_order_of_the_units_does_not_change_their_results() {
    let units = all_curve_units();
    let forward = exact_text(&curve_values_batch(&units, 4).unwrap());
    let reversed_units: Vec<_> = units.iter().rev().cloned().collect();
    let mut reversed = exact_text(&curve_values_batch(&reversed_units, 4).unwrap());
    reversed.reverse();
    assert_eq!(forward, reversed);

    // splitting the units in two gives the same results
    let middle = units.len() / 2;
    let mut split = exact_text(&curve_values_batch(&units[..middle], 2).unwrap());
    split.extend(exact_text(
        &curve_values_batch(&units[middle..], 3).unwrap(),
    ));
    assert_eq!(forward, split);
}

#[test]
fn a_failing_unit_does_not_affect_the_others() {
    let mut units = bond_units();
    let good = exact_text(&bond_values_batch(&units, 4).unwrap());
    units[3].convention.frequency = 0;
    units[3].convention.compounding = Compounding::Compounded;
    let results = bond_values_batch(&units, 4).unwrap();
    assert!(results[3].is_err());
    let text = exact_text(&results);
    assert_eq!(good[..3], text[..3]);
    assert_eq!(good[4..], text[4..]);
}

#[test]
fn the_output_keeps_the_input_order_when_later_items_finish_first() {
    let items: Vec<u64> = (0..64).collect();
    for threads in [1, 2, 4, 8] {
        let got = map_parallel(&items, threads, |&i| {
            // early items take longer, so they finish after the later ones
            std::thread::sleep(std::time::Duration::from_millis(64 - i));
            i * 10
        })
        .unwrap();
        let expected: Vec<u64> = items.iter().map(|i| i * 10).collect();
        assert_eq!(got, expected, "{threads} threads");
    }
}

#[test]
fn the_pool_really_has_the_requested_number_of_threads() {
    let items = [0];
    let caller = std::thread::current().id();
    for threads in [1, 2, 4, 8] {
        let seen = map_parallel(&items, threads, |_| {
            (rayon::current_num_threads(), std::thread::current().id())
        })
        .unwrap();
        assert_eq!(seen[0].0, threads);
        // never on the calling thread, whose floating-point state can differ (see `map_parallel`)
        assert_ne!(seen[0].1, caller, "{threads} threads");
    }
}
