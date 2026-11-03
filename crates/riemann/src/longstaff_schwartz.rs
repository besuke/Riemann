//! Black-Scholes paths and the Longstaff-Schwartz method for options with early exercise.
//!
//! The rolling back follows QuantLib v1.42 `ql/methods/montecarlo/longstaffschwartzpathpricer.hpp`
//! (Copyright (C) 2006 Klaus Spanderen); modified BSD license. See THIRD_PARTY_NOTICES. The choices that
//! QuantLib and LagrangeFinance make differently are settings here: see `docs/spec_longstaff_schwartz.md`.

// `!(x > 0.0)` is intended: it also rejects NaN
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::math::linear::solve_least_squares;
use crate::math::normal::compute_inverse_normal_cdf;
use crate::math::random::generate_standard_normal;
use crate::math::sobol::{BrownianBridge, SobolSequence};
use crate::options::{OptionInputs, OptionType};
use crate::Error;

fn invalid<T>(reason: String) -> Result<T, Error> {
    Err(Error::InvalidInput { reason })
}

/// Where the normal variates of the paths come from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathSequence {
    /// A Sobol sequence, one dimension per step, without scrambling. With the bridge, the first
    /// dimensions set the largest scales of the path; without it, dimension `i` drives step `i`.
    Sobol { brownian_bridge: bool },
    /// The random numbers of [`generate_standard_normal`], addressed by path and step.
    Pseudo { seed: u64 },
}

/// The prices of `count` paths at the times `0, dt, ..., steps * dt`, by rows. Each path has
/// `steps + 1` values. `first` is the index of the first path in the sequence.
fn generate_price_matrix(
    inputs: &OptionInputs<f64>,
    steps: usize,
    sequence: PathSequence,
    first: usize,
    count: usize,
) -> Result<Vec<f64>, Error> {
    let dt = inputs.time / steps as f64;
    let drift = (inputs.rate - inputs.dividend_yield - 0.5 * inputs.volatility * inputs.volatility) * dt;
    let diffusion = inputs.volatility * dt.sqrt();
    let mut out = vec![0.0; count * (steps + 1)];
    let mut write = |path: usize, z: &[f64]| {
        let row = &mut out[path * (steps + 1)..(path + 1) * (steps + 1)];
        row[0] = inputs.spot;
        for i in 0..steps {
            row[i + 1] = row[i] * (drift + diffusion * z[i]).exp();
        }
    };
    match sequence {
        PathSequence::Sobol { brownian_bridge } => {
            if (first + count) as u64 > SobolSequence::MAX_POINTS {
                return invalid(format!("a Sobol sequence gives at most {} points", SobolSequence::MAX_POINTS));
            }
            let mut sobol = SobolSequence::new(steps)?;
            for _ in 0..first {
                sobol.next_integers();
            }
            let times: Vec<f64> = (1..=steps).map(|i| i as f64 * dt).collect();
            let bridge = brownian_bridge.then(|| BrownianBridge::new(&times));
            let (mut u, mut z, mut increments) = (vec![0.0; steps], vec![0.0; steps], vec![0.0; steps]);
            for path in 0..count {
                sobol.next_point(&mut u);
                for (zi, ui) in z.iter_mut().zip(&u) {
                    *zi = compute_inverse_normal_cdf(*ui);
                }
                match &bridge {
                    Some(b) => {
                        b.transform(&z, &mut increments);
                        write(path, &increments);
                    }
                    None => write(path, &z),
                }
            }
        }
        PathSequence::Pseudo { seed } => {
            let mut z = vec![0.0; steps];
            for path in 0..count {
                for (i, zi) in z.iter_mut().enumerate() {
                    *zi = generate_standard_normal(seed, (first + path) as u64, i as u64);
                }
                write(path, &z);
            }
        }
    }
    Ok(out)
}

fn check_paths(inputs: &OptionInputs<f64>, steps: usize, paths: usize) -> Result<(), Error> {
    inputs.check()?;
    if !(inputs.time > 0.0) {
        return invalid(format!("paths need a positive time, not {}", inputs.time));
    }
    if steps == 0 || paths == 0 {
        return invalid("paths need at least 1 step and 1 path".into());
    }
    if paths.saturating_mul(steps + 1) > 2_000_000_000 {
        return invalid(format!("{paths} paths of {steps} steps are too many values to hold"));
    }
    Ok(())
}

/// Black-Scholes paths, by rows. The steps are exact for the constant coefficients.
pub fn generate_black_scholes_paths(
    inputs: &OptionInputs<f64>,
    steps: usize,
    paths: usize,
    sequence: PathSequence,
) -> Result<Vec<f64>, Error> {
    check_paths(inputs, steps, paths)?;
    generate_price_matrix(inputs, steps, sequence, 0, paths)
}

/// Which paths the regressions see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationPaths {
    /// The evaluation paths themselves (LagrangeFinance). The value is biased upward.
    InSample,
    /// The first paths of the sequence, with the evaluation starting from the same first path
    /// (QuantLib's low-discrepancy engine). They overlap the evaluation paths.
    Overlapping(usize),
    /// This many paths, with the evaluation continuing after them in the sequence, so that no path is
    /// used twice. For pseudo-random numbers the two sets are independent samples, and the value is then
    /// biased downward. For a Sobol sequence both halves come from one deterministic sequence: they share
    /// no path, but they are not independent samples, and no such statement about the bias follows.
    Separate(usize),
}

/// What to do at a date with fewer paths in the money than basis functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FewInTheMoney {
    /// Do not exercise (LagrangeFinance).
    Hold,
    /// Exercise every path in the money (QuantLib).
    Exercise,
}

/// The settings of [`compute_longstaff_schwartz`].
#[derive(Debug, Clone, PartialEq)]
pub struct LongstaffSchwartzSettings {
    pub steps: usize,
    pub paths: usize,
    pub sequence: PathSequence,
    /// The highest power of `S / K` in the regression. 0 is a constant.
    pub basis_degree: usize,
    /// Adds the payoff to the regression, as QuantLib always does.
    pub include_payoff: bool,
    pub calibration: CalibrationPaths,
    pub few_in_the_money: FewInTheMoney,
    /// The steps that allow exercise. `None` is all of them from 1. The last step is always allowed.
    pub exercise_steps: Option<Vec<usize>>,
}

/// One date of the regressions, rolling back on the calibration paths.
#[derive(Debug, Clone, PartialEq)]
pub struct RegressionDate {
    pub step: usize,
    pub time: f64,
    pub in_the_money_paths: usize,
    /// The paths that the regression tells to exercise at this date, whatever they did later.
    pub exercised_paths: usize,
    pub mean_immediate_value: f64,
    /// The mean over all paths of the cash flow already chosen, discounted to this date.
    pub mean_realized_continuation: f64,
    /// For a put the highest spot exercised, for a call the lowest. `None` with no exercise.
    pub exercise_boundary: Option<f64>,
    /// In the order 1, x, x^2, ..., and the payoff last if it is included. `None` with too few paths.
    pub coefficients: Option<Vec<f64>>,
    /// An exercise date with fewer paths in the money than basis functions: [`FewInTheMoney`] decided.
    pub too_few_paths: bool,
}

/// How many evaluation paths stop at a date.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExerciseCount {
    pub step: usize,
    pub time: f64,
    pub paths: usize,
}

/// The result of [`compute_longstaff_schwartz`].
#[derive(Debug, Clone, PartialEq)]
pub struct LongstaffSchwartzResult {
    pub value: f64,
    /// `sd / sqrt(paths)` over the evaluation paths. For a Sobol sequence the paths are not independent,
    /// so this is not an error estimate (QuantLib gives none there).
    pub standard_error: f64,
    /// The discounted payoff at maturity on the same paths, without early exercise.
    pub european_value: f64,
    /// The share of evaluation paths that ever pay.
    pub exercise_probability: f64,
    /// The evaluation paths that reach maturity without a payment.
    pub unexercised_paths: usize,
    pub exercise: Vec<ExerciseCount>,
    pub regressions: Vec<RegressionDate>,
    /// The exercise dates where [`FewInTheMoney`] decided, because a regression had too few paths.
    /// LagrangeFinance and QuantLib decide differently there, so a comparison records this count.
    pub dates_with_too_few_paths: usize,
}

/// An American or Bermudan option by the Longstaff-Schwartz method. The regression uses the paths in
/// the money. A path exercises when its immediate value exceeds the estimated continuation value.
pub fn compute_longstaff_schwartz(
    kind: OptionType,
    inputs: &OptionInputs<f64>,
    settings: &LongstaffSchwartzSettings,
) -> Result<LongstaffSchwartzResult, Error> {
    let LongstaffSchwartzSettings { steps, paths, sequence, basis_degree, include_payoff, calibration, few_in_the_money, .. } = *settings;
    check_paths(inputs, steps, paths)?;
    if basis_degree > 10 {
        return invalid(format!("the highest power of the basis is 10, not {basis_degree}"));
    }
    let mut allowed = vec![false; steps + 1];
    match &settings.exercise_steps {
        None => allowed[1..].fill(true),
        Some(list) => {
            if list.iter().any(|&s| s == 0 || s > steps) {
                return invalid(format!("exercise steps must be from 1 to {steps}"));
            }
            for &s in list {
                allowed[s] = true;
            }
            allowed[steps] = true;
        }
    }

    let (calibration_count, evaluation_first) = match calibration {
        CalibrationPaths::InSample => (paths, 0),
        CalibrationPaths::Overlapping(n) => (n, 0),
        CalibrationPaths::Separate(n) => (n, n),
    };
    if calibration_count == 0 {
        return invalid("the regressions need at least 1 path".into());
    }
    check_paths(inputs, steps, calibration_count)?;
    let width = steps + 1;
    let strike = inputs.strike;
    let sign = kind.sign();
    let payoff = |s: f64| (sign * (s - strike)).max(0.0);
    let columns = basis_degree + 1 + usize::from(include_payoff);
    let basis = |s: f64, row: &mut [f64]| {
        let x = s / strike;
        let mut power = 1.0;
        for value in row.iter_mut().take(basis_degree + 1) {
            *value = power;
            power *= x;
        }
        if include_payoff {
            row[basis_degree + 1] = payoff(s) / strike;
        }
    };
    let dt = inputs.time / steps as f64;
    let df = (-inputs.rate * dt).exp();

    // ---- rolling back on the calibration paths ----
    let train = generate_price_matrix(inputs, steps, sequence, 0, calibration_count)?;
    let n = calibration_count;
    let mut cash: Vec<f64> = (0..n).map(|j| payoff(train[j * width + steps])).collect();
    let mut regressions = Vec::with_capacity(steps.saturating_sub(1));
    let mut row = vec![0.0; columns];
    for step in (1..steps).rev() {
        let immediate: Vec<f64> = (0..n).map(|j| payoff(train[j * width + step])).collect();
        let realized: Vec<f64> = cash.iter().map(|c| c * df).collect();
        let mean_immediate_value = immediate.iter().sum::<f64>() / n as f64;
        let mean_realized_continuation = realized.iter().sum::<f64>() / n as f64;
        let in_the_money: Vec<usize> = (0..n).filter(|&j| immediate[j] > 0.0).collect();
        for c in &mut cash {
            *c *= df;
        }
        let time = step as f64 * dt;
        if !allowed[step] {
            regressions.push(RegressionDate {
                step,
                time,
                in_the_money_paths: in_the_money.len(),
                exercised_paths: 0,
                mean_immediate_value,
                mean_realized_continuation,
                exercise_boundary: None,
                coefficients: None,
                too_few_paths: false,
            });
            continue;
        }
        let coefficients = if in_the_money.len() >= columns {
            let mut design = vec![0.0; in_the_money.len() * columns];
            let mut target = Vec::with_capacity(in_the_money.len());
            for (i, &j) in in_the_money.iter().enumerate() {
                basis(train[j * width + step], &mut design[i * columns..(i + 1) * columns]);
                target.push(realized[j]);
            }
            Some(solve_least_squares(&design, columns, &target)?)
        } else {
            None
        };
        let mut exercised_paths = 0;
        let mut exercise_boundary: Option<f64> = None;
        for &j in &in_the_money {
            let spot = train[j * width + step];
            let stop = match &coefficients {
                Some(beta) => {
                    basis(spot, &mut row);
                    let continuation: f64 = beta.iter().zip(&row).map(|(b, v)| b * v).sum();
                    continuation < immediate[j]
                }
                None => few_in_the_money == FewInTheMoney::Exercise,
            };
            if stop {
                cash[j] = immediate[j];
                exercised_paths += 1;
                let better = match (exercise_boundary, kind) {
                    (None, _) => true,
                    (Some(b), OptionType::Put) => spot > b,
                    (Some(b), OptionType::Call) => spot < b,
                };
                if better {
                    exercise_boundary = Some(spot);
                }
            }
        }
        regressions.push(RegressionDate {
            step,
            time,
            in_the_money_paths: in_the_money.len(),
            exercised_paths,
            mean_immediate_value,
            mean_realized_continuation,
            exercise_boundary,
            too_few_paths: coefficients.is_none(),
            coefficients,
        });
    }
    let dates_with_too_few_paths = regressions.iter().filter(|r| r.too_few_paths).count();

    // ---- the value on the evaluation paths, with the decisions fixed ----
    let reused;
    let evaluation: &[f64] = if calibration == CalibrationPaths::InSample {
        &train
    } else {
        reused = generate_price_matrix(inputs, steps, sequence, evaluation_first, paths)?;
        &reused
    };
    let decisions: Vec<&RegressionDate> = {
        let mut by_step = regressions.iter().collect::<Vec<_>>();
        by_step.sort_by_key(|r| r.step);
        by_step
    };
    let mut counts = vec![0usize; steps + 1];
    let mut unexercised_paths = 0;
    let (mut sum, mut sum_squares, mut european, mut paying) = (0.0, 0.0, 0.0, 0usize);
    let final_discount = (-inputs.rate * inputs.time).exp();
    for j in 0..paths {
        let path = &evaluation[j * width..(j + 1) * width];
        let mut price = payoff(path[steps]);
        european += price * final_discount;
        let mut stop_step = if price > 0.0 { Some(steps) } else { None };
        for decision in decisions.iter().rev() {
            let step = decision.step;
            price *= df;
            if !allowed[step] {
                continue;
            }
            let immediate = payoff(path[step]);
            if immediate > 0.0 {
                let stop = match &decision.coefficients {
                    Some(beta) => {
                        basis(path[step], &mut row);
                        let continuation: f64 = beta.iter().zip(&row).map(|(b, v)| b * v).sum();
                        continuation < immediate
                    }
                    None => few_in_the_money == FewInTheMoney::Exercise,
                };
                if stop {
                    price = immediate;
                    stop_step = Some(step);
                }
            }
        }
        let value = price * df;
        sum += value;
        sum_squares += value * value;
        match stop_step {
            Some(step) => {
                counts[step] += 1;
                paying += 1;
            }
            None => unexercised_paths += 1,
        }
    }
    let count = paths as f64;
    let value = sum / count;
    let variance = if paths > 1 { ((sum_squares - sum * sum / count) / (count - 1.0)).max(0.0) } else { 0.0 };
    let exercise = (1..=steps)
        .filter(|&s| allowed[s])
        .map(|step| ExerciseCount { step, time: step as f64 * dt, paths: counts[step] })
        .collect();
    Ok(LongstaffSchwartzResult {
        value,
        standard_error: (variance / count).sqrt(),
        european_value: european / count,
        exercise_probability: paying as f64 / count,
        unexercised_paths,
        exercise,
        regressions,
        dates_with_too_few_paths,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::compute_european_value;

    fn inputs() -> OptionInputs<f64> {
        OptionInputs { spot: 100.0, strike: 100.0, rate: 0.05, dividend_yield: 0.0, volatility: 0.2, time: 1.0 }
    }

    fn settings(paths: usize, degree: usize) -> LongstaffSchwartzSettings {
        LongstaffSchwartzSettings {
            steps: 12,
            paths,
            sequence: PathSequence::Sobol { brownian_bridge: false },
            basis_degree: degree,
            include_payoff: false,
            calibration: CalibrationPaths::InSample,
            few_in_the_money: FewInTheMoney::Hold,
            exercise_steps: None,
        }
    }

    #[test]
    fn exercise_only_at_maturity_is_the_european_option() {
        let mut s = settings(1 << 14, 2);
        s.exercise_steps = Some(vec![12]);
        let r = compute_longstaff_schwartz(OptionType::Put, &inputs(), &s).unwrap();
        // the same payoffs, discounted by 12 steps or at once: equal up to rounding
        assert!((r.value - r.european_value).abs() < 1e-12, "{} {}", r.value, r.european_value);
        assert_eq!(r.exercise.len(), 1);
        // The points are deterministic and not scrambled, so the naive standard error is no confidence
        // interval: the test is the error against the closed form, as the number of paths grows. Without
        // the bridge the terminal value draws on 12 dimensions equally; with it, on the first alone.
        let exact = compute_european_value(OptionType::Put, &inputs()).unwrap();
        let error = |paths: usize, bridge: bool| {
            let s = LongstaffSchwartzSettings { paths, sequence: PathSequence::Sobol { brownian_bridge: bridge }, ..s.clone() };
            (compute_longstaff_schwartz(OptionType::Put, &inputs(), &s).unwrap().value - exact).abs()
        };
        // The error need not fall at every doubling. It is recorded at 2^14 to 2^18 paths and judged by
        // the accuracy at 2^18 and by the overall trend: the least-squares slope of log(error) on log(paths).
        for bridge in [false, true] {
            let points: Vec<(f64, f64)> = (14..=18).map(|k| ((k as f64) * 2f64.ln(), error(1 << k, bridge).max(1e-16).ln())).collect();
            println!("bridge {bridge}: errors {:?}", points.iter().map(|p| p.1.exp()).collect::<Vec<_>>());
            let mean_x = points.iter().map(|p| p.0).sum::<f64>() / 5.0;
            let mean_y = points.iter().map(|p| p.1).sum::<f64>() / 5.0;
            let slope = points.iter().map(|p| (p.0 - mean_x) * (p.1 - mean_y)).sum::<f64>()
                / points.iter().map(|p| (p.0 - mean_x).powi(2)).sum::<f64>();
            assert!(slope < -0.3, "bridge {bridge}: slope {slope}");
            let last = points[4].1.exp();
            assert!(last < if bridge { 1e-3 } else { 5e-3 }, "bridge {bridge}: {last}");
        }
    }

    #[test]
    fn a_call_without_dividends_is_not_exercised_early() {
        // its continuation value exceeds the immediate value on all but a few paths
        let r = compute_longstaff_schwartz(OptionType::Call, &inputs(), &settings(1 << 14, 3)).unwrap();
        let exact = compute_european_value(OptionType::Call, &inputs()).unwrap();
        assert!((r.value - exact).abs() < 0.15, "{} {exact}", r.value);
    }

    #[test]
    fn the_american_put_is_above_the_european_and_in_sample_above_separate() {
        let s = settings(1 << 14, 2);
        let in_sample = compute_longstaff_schwartz(OptionType::Put, &inputs(), &s).unwrap();
        assert!(in_sample.value > in_sample.european_value);
        let separate = LongstaffSchwartzSettings {
            sequence: PathSequence::Pseudo { seed: 7 },
            calibration: CalibrationPaths::Separate(1 << 14),
            ..s
        };
        let out = compute_longstaff_schwartz(OptionType::Put, &inputs(), &separate).unwrap();
        assert!(out.value > out.european_value - 3.0 * out.standard_error);
    }

    #[test]
    fn the_counts_add_up_and_the_result_repeats() {
        let s = settings(2048, 2);
        let r = compute_longstaff_schwartz(OptionType::Put, &inputs(), &s).unwrap();
        let total: usize = r.exercise.iter().map(|e| e.paths).sum();
        assert_eq!(total + r.unexercised_paths, 2048);
        assert_eq!(r, compute_longstaff_schwartz(OptionType::Put, &inputs(), &s).unwrap());
    }

    #[test]
    fn collinear_payoff_changes_nothing_when_the_basis_spans_it() {
        // The regression uses only the paths in the money, where the payoff of a put is K - S: a
        // combination of 1 and x = S / K. The extra column is dependent and gets a zero coefficient, so
        // the fitted continuation values, the decisions and the value do not change.
        let plain = compute_longstaff_schwartz(OptionType::Put, &inputs(), &settings(2048, 2)).unwrap();
        let mut s = settings(2048, 2);
        s.include_payoff = true;
        let with_payoff = compute_longstaff_schwartz(OptionType::Put, &inputs(), &s).unwrap();
        assert!((plain.value - with_payoff.value).abs() < 1e-10, "{} {}", plain.value, with_payoff.value);
    }

    #[test]
    fn the_constant_basis_with_the_payoff_is_the_linear_basis() {
        // For the same reason, {1, payoff} spans the same functions as {1, x} on the paths in the money.
        // This holds for a vanilla payoff, which is linear where it is positive (K - S for a put, S - K for
        // a call); it does not hold for a regression on all paths, where the payoff has a kink.
        let linear = compute_longstaff_schwartz(OptionType::Put, &inputs(), &settings(2048, 1)).unwrap();
        let mut s = settings(2048, 0);
        s.include_payoff = true;
        let with_payoff = compute_longstaff_schwartz(OptionType::Put, &inputs(), &s).unwrap();
        assert!((linear.value - with_payoff.value).abs() < 1e-10, "{} {}", linear.value, with_payoff.value);
    }

    #[test]
    fn sobol_paths_of_the_bridge_keep_the_first_path_and_the_distribution() {
        let i = inputs();
        let plain = generate_black_scholes_paths(&i, 8, 1024, PathSequence::Sobol { brownian_bridge: false }).unwrap();
        let bridged = generate_black_scholes_paths(&i, 8, 1024, PathSequence::Sobol { brownian_bridge: true }).unwrap();
        // The first Sobol point is 0.5 in every dimension (not 0). Its normal variates are all 0, so with
        // or without the bridge the increments are 0 and the first path is the drift alone.
        for (a, b) in plain[..9].iter().zip(&bridged[..9]) {
            assert!((a - b).abs() < 1e-11, "{a} {b}");
        }
        // the mean of the terminal price is the forward, to the accuracy of the points
        let mean = |m: &[f64]| m.chunks(9).map(|p| p[8]).sum::<f64>() / 1024.0;
        assert!((mean(&bridged) - 100.0 * 0.05f64.exp()).abs() < 0.1, "{}", mean(&bridged));
    }

    #[test]
    fn invalid_settings_are_errors() {
        let run = |s: &LongstaffSchwartzSettings| compute_longstaff_schwartz(OptionType::Put, &inputs(), s);
        let base = settings(64, 2);
        assert!(run(&LongstaffSchwartzSettings { steps: 0, ..base.clone() }).is_err());
        assert!(run(&LongstaffSchwartzSettings { paths: 0, ..base.clone() }).is_err());
        assert!(run(&LongstaffSchwartzSettings { steps: 33, ..base.clone() }).is_err());
        assert!(run(&LongstaffSchwartzSettings { basis_degree: 11, ..base.clone() }).is_err());
        assert!(run(&LongstaffSchwartzSettings { exercise_steps: Some(vec![13]), ..base.clone() }).is_err());
        assert!(run(&LongstaffSchwartzSettings { calibration: CalibrationPaths::Separate(0), ..base }).is_err());
    }
}
