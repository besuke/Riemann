//! The Sobol sequence, the paths built from it and the Longstaff-Schwartz values, each against QuantLib
//! (tools/golden/p11_longstaff_schwartz.R), in that order, so that a difference can be placed. All
//! inputs are synthetic.

mod common;

use common::golden_rows;
use riemann::longstaff_schwartz::{
    compute_longstaff_schwartz, generate_black_scholes_paths, CalibrationPaths, FewInTheMoney, LongstaffSchwartzSettings, PathSequence,
};
use riemann::math::normal::compute_inverse_normal_cdf;
use riemann::math::sobol::SobolSequence;
use riemann::options::{OptionInputs, OptionType};

fn number(s: &str) -> f64 {
    s.parse().unwrap()
}

#[test]
fn sobol_points_are_quantlibs() {
    let rows = golden_rows("ls_sobol.csv");
    assert_eq!(rows.len(), 7 * 512);
    let mut checked = 0;
    for dimension in [1usize, 2, 3, 8, 9, 12, 32] {
        let mut sobol = SobolSequence::new(dimension).unwrap();
        let mut point = vec![0.0; dimension];
        for r in rows.iter().filter(|r| r[0] == dimension.to_string()) {
            sobol.next_point(&mut point);
            let expected: Vec<f64> = r[2].split(';').map(number).collect();
            assert_eq!(point, expected, "dimension {dimension}, point {}", r[1]);
            checked += 1;
        }
    }
    assert_eq!(checked, 7 * 512);
}

/// LagrangeFinance's generator: 12 dimensions, seed 1. Within the table the seed changes nothing, so the
/// sequence without a seed must give the same uniform points, and the same normal variates after the
/// inverse normal.
#[test]
fn uniform_points_and_normal_variates_are_quantlibs() {
    let rows = golden_rows("ls_normals.csv");
    assert_eq!(rows.len(), 64);
    let mut sobol = SobolSequence::new(12).unwrap();
    let mut u = vec![0.0; 12];
    let mut worst = 0.0f64;
    for r in &rows {
        sobol.next_point(&mut u);
        let expected_u: Vec<f64> = r[1].split(';').map(number).collect();
        assert_eq!(u, expected_u, "point {}", r[0]);
        for (ui, zi) in u.iter().zip(r[2].split(';').map(number)) {
            worst = worst.max((compute_inverse_normal_cdf(*ui) - zi).abs());
        }
    }
    println!("normal variates: worst absolute difference {worst:e}");
    assert_eq!(worst, 0.0);
}

#[test]
fn paths_are_quantlibs_with_and_without_the_bridge() {
    let rows = golden_rows("ls_paths.csv");
    // eight cases of 128 paths, then LagrangeFinance's 64 paths of II-4.6.1
    assert_eq!(rows.len(), 8 * 128 + 64);
    let mut worst = 0.0f64;
    for case in 1..=9 {
        let case_rows: Vec<_> = rows.iter().filter(|r| r[0] == case.to_string()).collect();
        let r0 = case_rows[0];
        let inputs = OptionInputs {
            spot: number(&r0[1]),
            strike: 100.0,
            rate: number(&r0[2]),
            dividend_yield: number(&r0[3]),
            volatility: number(&r0[4]),
            time: number(&r0[5]),
        };
        let steps: usize = r0[6].parse().unwrap();
        let sequence = PathSequence::Sobol { brownian_bridge: r0[7] == "1" };
        let paths = generate_black_scholes_paths(&inputs, steps, case_rows.len(), sequence).unwrap();
        for (p, r) in case_rows.iter().enumerate() {
            let expected: Vec<f64> = r[9].split(';').map(number).collect();
            for (got, want) in paths[p * (steps + 1)..(p + 1) * (steps + 1)].iter().zip(&expected) {
                worst = worst.max((got / want - 1.0).abs());
            }
        }
    }
    println!("paths: worst relative difference {worst:e}");
    assert!(worst < 1e-13, "{worst}");
}

#[test]
fn american_values_are_quantlibs_low_discrepancy_engine() {
    let rows = golden_rows("ls_american.csv");
    assert_eq!(rows.len(), 40);
    let (mut worst, mut few_dates_cases) = (0.0f64, 0);
    for r in &rows {
        let kind = if r[1] == "put" { OptionType::Put } else { OptionType::Call };
        let inputs = OptionInputs {
            spot: number(&r[2]),
            strike: number(&r[3]),
            rate: number(&r[4]),
            dividend_yield: number(&r[5]),
            volatility: number(&r[6]),
            time: number(&r[7]) / 365.0,
        };
        // QuantLib's engine: its own conditions, set here explicitly
        let settings = LongstaffSchwartzSettings {
            steps: r[8].parse().unwrap(),
            paths: r[11].parse().unwrap(),
            sequence: PathSequence::Sobol { brownian_bridge: false },
            basis_degree: r[9].parse().unwrap(),
            include_payoff: true,
            calibration: CalibrationPaths::Overlapping(r[10].parse().unwrap()),
            few_in_the_money: FewInTheMoney::Exercise,
            exercise_steps: None,
        };
        let result = compute_longstaff_schwartz(kind, &inputs, &settings).unwrap();
        let difference = (result.value - number(&r[12])).abs();
        worst = worst.max(difference);
        few_dates_cases += usize::from(result.dates_with_too_few_paths > 0);
        // Where it happened, LagrangeFinance's rule (hold) would give another value: a stated difference
        // of specification, recorded here and not hidden in the tolerance
        if result.dates_with_too_few_paths > 0 {
            let hold = LongstaffSchwartzSettings { few_in_the_money: FewInTheMoney::Hold, ..settings.clone() };
            let held = compute_longstaff_schwartz(kind, &inputs, &hold).unwrap();
            println!("   case {}: with the rule hold the value is {:.10}, {:+.3e} from QuantLib's rule", r[0], held.value, held.value - result.value);
        } else {
            let hold = LongstaffSchwartzSettings { few_in_the_money: FewInTheMoney::Hold, ..settings.clone() };
            assert_eq!(compute_longstaff_schwartz(kind, &inputs, &hold).unwrap().value, result.value, "case {}", r[0]);
        }
        println!(
            "{:>2} {} value {:.10} quantlib {:.10} difference {:.1e} dates with too few paths {}",
            r[0], r[1], result.value, number(&r[12]), difference, result.dates_with_too_few_paths
        );
    }
    println!("american: worst absolute difference {worst:e}; cases with dates of too few paths {few_dates_cases}");
    assert!(worst < 1e-10, "{worst}");
}
