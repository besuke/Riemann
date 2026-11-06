//! The forward market model of term rates, lognormal (Black) or normal (Bachelier), with joint paths
//! under the spot measure, the Rebonato approximation of swaption volatilities, and the calibration
//! of an exponential correlation to swaptions (LagrangeFinance II-7.4 and II-7.6, extended).
//!
//! The tenor structure is `T_0 < T_1 < ... < T_n` (years from today); the forward `L_i` covers
//! `[T_i, T_{i+1}]` with accrual `tau_i = T_{i+1} - T_i`, fixes at `T_i` and pays at `T_{i+1}`. Each forward
//! has a constant volatility `sigma_i`, and the Brownian motions are correlated by `rho`. Under the
//! spot measure, whose numeraire rolls the money from one tenor date to the next,
//!
//! `dL_k / L_k = sigma_k sum_{j = eta(t)}^{k} tau_j rho_kj sigma_j L_j / (1 + tau_j L_j) dt + sigma_k dW_k`,
//!
//! where `eta(t)` is the first forward not yet fixed (Glasserman, *Monte Carlo Methods in Financial
//! Engineering*, 3.7). The paths take log-Euler steps with the drift at the start of each step; a few
//! steps per period make the discretization error small. The numeraire is `B(T_0) = 1 / P(0, T_0)` and
//! `B(T_{k+1}) = B(T_k) (1 + tau_k L_k(T_k))`.
//!
//! Under the terminal measure ([`Measure::Terminal`]) the numeraire is the bond to `T_n` and the drift is
//! `-sigma_k sum_{j = k + 1}^{n - 1} tau_j rho_kj sigma_j L_j / (1 + tau_j L_j)`; the valuations are the same
//! functions, as each divides a payoff by the numeraire on the path.
//!
//! The rates are generic forward-looking term rates. This is a model of term forwards, not an IBOR
//! index: no fixings, no index curve (see `docs/quantlib_guide_inventory.md` on IBOR).
//!
//! With fewer factors, the reduced correlation follows QuantLib v1.42
//! `ql/math/matrixutilities/pseudosqrt.cpp` (`rankReducedSqrt` and `normalizePseudoRoot`; Copyright (C)
//! 2003, 2004, 2007 Ferdinando Ametrano; (C) 2006 Yiping Chen; (C) 2007 Neil Firth); modified BSD license.
//! See THIRD_PARTY_NOTICES. The eigen decomposition is this crate's own Jacobi method.

// `!(x > 0.0)` is intended: it also rejects NaN
#![allow(clippy::neg_cmp_op_on_partial_ord)]
// the forwards, accruals, volatilities and correlations share one index
#![allow(clippy::needless_range_loop)]

use crate::lmm::{check_correlation, check_correlation_entries};
use crate::math::linear::compute_symmetric_eigen;
use crate::math::random::generate_standard_normal;
use crate::options::{compute_forward_value, ForwardInputs, ForwardModel, OptionType};
use crate::Error;

fn invalid<T>(reason: String) -> Result<T, Error> {
    Err(Error::InvalidInput { reason })
}

/// The model: tenor dates, forwards today, volatilities and correlation.
#[derive(Debug, Clone, PartialEq)]
pub struct LmmSpec {
    /// `T_0, ..., T_n` in years from today, increasing, `T_0 >= 0`.
    pub times: Vec<f64>,
    /// `L_0(0), ..., L_{n-1}(0)`.
    pub forwards: Vec<f64>,
    /// Constant lognormal volatilities `sigma_0, ..., sigma_{n-1}`.
    pub volatilities: Vec<f64>,
    /// `n` by `n`, by rows.
    pub correlation: Vec<f64>,
    /// `P(0, T_0)` when `T_0 >= 0`. When the first period started before today (`T_0 < 0`, backward-looking
    /// rates only), the compounding factor `A(0)` of its fixings so far: then `P(0, T_k) = A(0) / prod_{j<k}
    /// (1 + tau_j R_j(0))` and the numeraire is `B(T_1) = (1 + tau_0 R_0(T_1)) / A(0)`, as with `P(0, T_0)`;
    /// `forwards[0]` is the forward of the whole period, from
    /// [`crate::fmm::compute_backward_forward_rate`].
    pub discount_start: f64,
    /// False for forward-looking term rates, fixed at the start of their period. True for
    /// backward-looking compounded rates (the generalized FMM of Lyashenko and Mercurio): rate `i`
    /// keeps moving through its period, with its volatility falling linearly to 0 at `T_{i+1}`, and
    /// is known at the end.
    pub backward_looking: bool,
    /// `Black`: lognormal rates, `dR / R = ... + sigma dW`. `Bachelier`: normal rates, `dR = ... + sigma dW`,
    /// with `sigma` in rate units; the rates may then turn negative.
    pub model: ForwardModel,
    /// `None` for the constant `volatilities`. Otherwise `n` rows of `n + 1` values, by rows: column `k` is
    /// the volatility on `(T_{k-1}, T_k]`, column 0 the one up to `T_0` (and before, for the periods under
    /// way). A rate uses its row while it moves; `volatilities` is then unused by the paths.
    pub volatility_grid: Option<Vec<f64>>,
    /// `None`: as many factors as forwards, the paths correlating `n` normals per step by the Cholesky
    /// factor of `correlation`. `Some(m)`: `m` factors. `correlation` must then have rank at most `m`
    /// (make it with [`create_reduced_correlation`]); the paths draw `m` normals per step and correlate
    /// them by its pseudo-root from the `m` largest eigenvalues. The drift, the variances and Rebonato's
    /// approximation use `correlation` itself, so the model stays one.
    pub factors: Option<usize>,
    /// The pricing measure of the paths: `Spot` (the numeraire rolls from one tenor date to the next) or
    /// `Terminal` (the bond to `T_n`). The values are the same up to the simulation and discretization
    /// errors; only the drift and the numeraire on the paths change.
    pub measure: Measure,
    /// The time steps: `Euler` (the drift at the start of each step) or `PredictorCorrector`.
    pub discretization: Discretization,
}

/// How a step treats the drift, which depends on the rates. The shocks are the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Discretization {
    /// The drift at the start of the step (log-Euler for lognormal rates, Euler for normal ones).
    Euler,
    /// A step with the drift at the start predicts the rates at the end; the step is then taken again
    /// with the mean of the drifts at the start and at the predicted rates (Hunter, Jaeckel and Joshi).
    PredictorCorrector,
}

/// The measure under which the paths move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Measure {
    /// The spot measure: numeraire `B(T_{k+1}) = B(T_k) (1 + tau_k L_k(T_k))`, drift
    /// `sigma_k g_k sum_{j = eta}^{k} tau_j rho_kj sigma_j g_j L_j / (1 + tau_j L_j)`.
    Spot,
    /// The terminal measure: numeraire `P(t, T_n) / P(0, T_n)`, drift
    /// `-sigma_k g_k sum_{j = k + 1}^{n - 1} tau_j rho_kj sigma_j g_j L_j / (1 + tau_j L_j)`
    /// (without the `L_j` in the numerator for normal rates).
    Terminal,
}

impl LmmSpec {
    fn count(&self) -> usize {
        self.forwards.len()
    }

    fn accrual(&self, i: usize) -> f64 {
        self.times[i + 1] - self.times[i]
    }

    /// The rows that correlate the normals of a step: the Cholesky factor's rows up to the diagonal, or
    /// the `m` columns of the reduced pseudo-root.
    fn check(&self) -> Result<Vec<Vec<f64>>, Error> {
        let n = self.count();
        if n == 0 || self.times.len() != n + 1 || self.volatilities.len() != n {
            return invalid(format!("give n + 1 times and n forwards and volatilities (n = {n})"));
        }
        if !self.times[0].is_finite() || self.times.windows(2).any(|w| !(w[1] > w[0])) {
            return invalid("the times must increase".into());
        }
        // periods that started before today: only backward-looking rates. Those that ended on or before
        // today are complete, and the next one is under way
        if self.times[0] < 0.0 {
            if !self.backward_looking {
                return invalid("only backward-looking rates can have periods that started before today".into());
            }
            if !(self.times[self.count()] > 0.0) {
                return invalid("the last period must end after today".into());
            }
        }
        if (self.model == ForwardModel::Black && self.forwards.iter().any(|f| !(*f > 0.0)))
            || self.forwards.iter().any(|f| !f.is_finite())
            || self.volatilities.iter().any(|v| !(*v >= 0.0) || !v.is_finite()) {
            return invalid("the forwards must be positive and the volatilities finite and non-negative".into());
        }
        if let Some(grid) = &self.volatility_grid {
            if grid.len() != self.count() * (self.count() + 1) || grid.iter().any(|v| !(*v >= 0.0) || !v.is_finite()) {
                return invalid(format!("the volatility grid must have {} finite non-negative values", self.count() * (self.count() + 1)));
            }
        }
        if !(self.discount_start > 0.0) {
            return invalid("the discount factor to the first time must be positive".into());
        }
        match self.factors {
            None => check_correlation(&self.correlation, n).map_err(|_| Error::InvalidInput {
                reason: "the correlation is not positive definite: for one of reduced rank, give the number of factors".into(),
            }),
            Some(m) => {
                if m == 0 || m > n {
                    return invalid(format!("the number of factors must be from 1 to {n}, not {m}"));
                }
                let (values, vectors) = compute_symmetric_eigen(&check_correlation_entries(&self.correlation, n)?)?;
                let tolerance = 1e-10 * n as f64;
                if values[n - 1] < -tolerance || (m < n && values[m] > tolerance) {
                    return invalid(format!(
                        "the correlation must be positive semi-definite with rank at most {m}: reduce it with create_reduced_correlation"
                    ));
                }
                Ok(pseudo_root(&values, &vectors, m))
            }
        }
    }

    /// The volatility of rate `i` at time `u`: its constant volatility, or the grid's column of the
    /// tenor interval `(T_{k-1}, T_k]` that holds `u`.
    pub fn volatility_at(&self, i: usize, u: f64) -> f64 {
        match &self.volatility_grid {
            None => self.volatilities[i],
            Some(grid) => {
                let k = self.times.iter().filter(|&&t| t < u).count();
                grid[i * (self.count() + 1) + k]
            }
        }
    }

    /// The first period that has not ended by today: 0 unless periods of backward-looking rates ended
    /// on or before today. Those are complete: their rates are fixed and paid.
    pub fn first_open_period(&self) -> usize {
        (0..self.count()).find(|&i| self.times[i + 1] > 0.0).unwrap_or(self.count())
    }

    /// `P(0, T_k)` from the forwards, for `T_k` not before the first open period's start (`NaN` before).
    /// `discount_start` is `P(0, T_c)` for that period `c`, or its realized compounding when it is under way.
    pub fn discount(&self, k: usize) -> f64 {
        let c = self.first_open_period();
        if k < c {
            return f64::NAN;
        }
        (c..k).fold(self.discount_start, |p, i| p / (1.0 + self.accrual(i) * self.forwards[i]))
    }
}

/// An exponential correlation `rho_ij = exp(-beta |T_i - T_j|)` on the forwards' fixing times.
pub fn create_exponential_correlation(times: &[f64], beta: f64) -> Result<Vec<f64>, Error> {
    if !(beta >= 0.0) || !beta.is_finite() {
        return invalid(format!("beta must be finite and non-negative, not {beta}"));
    }
    Ok(times.iter().flat_map(|&a| times.iter().map(move |&b| (-beta * (a - b).abs()).exp())).collect())
}

/// The pseudo-root from the `m` largest eigenvalues, `B[i][k] = v_k[i] sqrt(lambda_k)`, with each row
/// rescaled to unit length so that `B B^T` has ones on its diagonal (QuantLib's `rankReducedSqrt` with
/// all components retained up to the rank, and its `normalizePseudoRoot`).
fn pseudo_root(values: &[f64], vectors: &[Vec<f64>], m: usize) -> Vec<Vec<f64>> {
    vectors
        .iter()
        .map(|v| {
            let row: Vec<f64> = (0..m).map(|k| v[k] * values[k].max(0.0).sqrt()).collect();
            let norm = row.iter().map(|x| x * x).sum::<f64>().sqrt();
            if norm > 0.0 { row.iter().map(|x| x / norm).collect() } else { row }
        })
        .collect()
}

/// The `n` by `m` pseudo-root of a correlation from its `m` largest eigenvalues, by rows, as QuantLib's
/// `rankReducedSqrt(matrix, m, 1.0, SalvagingAlgorithm::None)`: a negative eigenvalue below `-1e-16` is
/// an error.
pub fn compute_reduced_pseudo_root(correlation: &[f64], n: usize, factors: usize) -> Result<Vec<f64>, Error> {
    let rows = check_correlation_entries(correlation, n)?;
    if factors == 0 || factors > n {
        return invalid(format!("the number of factors must be from 1 to {n}, not {factors}"));
    }
    let (values, vectors) = compute_symmetric_eigen(&rows)?;
    if values[n - 1] < -1e-16 {
        return invalid(format!("the correlation has a negative eigenvalue ({:e})", values[n - 1]));
    }
    let root = pseudo_root(&values, &vectors, factors);
    if root.iter().any(|row| row.iter().all(|x| *x == 0.0)) {
        return invalid("a forward has no weight on the retained factors".into());
    }
    Ok(root.concat())
}

/// The correlation of `m` factors, `B B^T` for the pseudo-root `B` of [`compute_reduced_pseudo_root`]:
/// symmetric, positive semi-definite, of rank `m`, with ones on its diagonal. With `m = n` it is the
/// correlation itself (up to rounding).
pub fn create_reduced_correlation(correlation: &[f64], n: usize, factors: usize) -> Result<Vec<f64>, Error> {
    let root = compute_reduced_pseudo_root(correlation, n, factors)?;
    let mut out = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..n {
            out[i * n + j] = if i == j {
                1.0
            } else {
                (0..factors).map(|k| root[i * factors + k] * root[j * factors + k]).sum::<f64>().clamp(-1.0, 1.0)
            };
        }
    }
    // exactly symmetric
    for i in 0..n {
        for j in 0..i {
            out[i * n + j] = out[j * n + i];
        }
    }
    Ok(out)
}

/// Joint paths: `state[p][k][i]` is `L_i(T_k)` on path `p` for `i >= k`, and for a backward-looking rate
/// also `i = k - 1`, its value at the end of its period (NaN for the others),
/// and `numeraire[p][k]` is the numeraire at `T_k`, normalized to 1 today: `B(T_k)` under the spot measure,
/// `P(T_k, T_n) / P(0, T_n)` under the terminal one. A payoff `X` at `T_k` is worth `E[X / numeraire[k]]`.
#[derive(Debug, Clone, PartialEq)]
pub struct LmmPaths {
    pub state: Vec<Vec<Vec<f64>>>,
    pub numeraire: Vec<Vec<f64>>,
    /// The time of `observed`, when the paths were asked for one.
    pub observation: Option<f64>,
    /// `observed[p][i]` is rate `i` on path `p` at the observation time (fixed rates at their fixing).
    pub observed: Vec<Vec<f64>>,
}

/// Simulates `paths` paths with `steps_per_period` log-Euler steps in each period (and from 0 to
/// `T_0`). The normal variate of path `p`, step `s` and forward `i` is the library's counter-based
/// draw for `(seed, p, s n + i)`, correlated by the Cholesky factor of `rho`.
pub fn generate_lmm_paths(spec: &LmmSpec, paths: usize, steps_per_period: usize, seed: u64) -> Result<LmmPaths, Error> {
    generate_lmm_paths_observed(spec, paths, steps_per_period, seed, None)
}

/// The paths with the rates also kept at `observation` (years, after 0 and not after the last time),
/// for a swaption exercised between tenor dates. The period that holds the observation is simulated in
/// two parts of `steps_per_period` steps each, so the draws differ from paths without an observation.
pub fn generate_lmm_paths_observed(
    spec: &LmmSpec,
    paths: usize,
    steps_per_period: usize,
    seed: u64,
    observation: Option<f64>,
) -> Result<LmmPaths, Error> {
    let root = spec.check()?;
    let width = spec.factors.unwrap_or(spec.count());
    if let Some(t) = observation {
        if !(t > 0.0 && t <= spec.times[spec.count()]) {
            return invalid(format!("the observation time must be after 0 and not after the last time, not {t}"));
        }
    }
    if paths == 0 || steps_per_period == 0 {
        return invalid("give at least one path and one step per period".into());
    }
    let n = spec.count();
    if paths.saturating_mul(n).saturating_mul(n + 1) > 200_000_000 {
        return invalid(format!("{paths} paths of {n} forwards are too many to keep"));
    }
    let mut state = Vec::with_capacity(paths);
    let mut numeraire = Vec::with_capacity(paths);
    let mut observed = Vec::with_capacity(if observation.is_some() { paths } else { 0 });
    for p in 0..paths {
        let mut seen: Option<Vec<f64>> = None;
        let mut l = spec.forwards.clone();
        let mut path_state = vec![vec![f64::NAN; n]; n + 1];
        let mut path_numeraire = vec![f64::NAN; n + 1];
        let open = spec.first_open_period();
        path_numeraire[open] = 1.0 / spec.discount_start;
        let mut step_count: u64 = 0;
        // `accruing` is the backward-looking rate whose period is [from, to], if any
        let mut evolve = |l: &mut Vec<f64>, from: f64, to: f64, alive: usize, accruing: Option<usize>| {
            let dt = (to - from) / steps_per_period as f64;
            for s in 0..steps_per_period {
                let z: Vec<f64> = (0..width).map(|i| generate_standard_normal(seed, p as u64, step_count * n as u64 + i as u64)).collect();
                step_count += 1;
                let w: Vec<f64> = root.iter().map(|row| row.iter().zip(&z).map(|(a, b)| a * b).sum()).collect();
                // the decay of the accruing rate: g at the step's middle for the drift, and the mean
                // of g^2 over the step for the variance (exact for the linear g)
                let mut g = vec![1.0; n];
                let mut g_squared = vec![1.0; n];
                if let Some(i) = accruing {
                    // g(u) = (T_{i+1} - u) / tau_i, from the actual time: below 1 for a period started before today
                    let end = spec.times[i + 1];
                    let tau = spec.accrual(i);
                    let (u0, u1) = (from + s as f64 * dt, from + (s + 1) as f64 * dt);
                    let (g0, g1) = ((end - u0) / tau, (end - u1) / tau);
                    g[i] = (g0 + g1) / 2.0;
                    g_squared[i] = (g0 * g0 + g0 * g1 + g1 * g1) / 3.0;
                }
                let snapshot = l.clone();
                // the volatilities hold over the step: it lies within one tenor interval
                let middle = from + (s as f64 + 0.5) * dt;
                let sigma: Vec<f64> = (0..n).map(|i| spec.volatility_at(i, middle)).collect();
                let mut drift = drift_with(spec, alive, &snapshot, &g, &sigma);
                let advance = |l: &mut Vec<f64>, drift: &[f64]| {
                    for k in alive..n {
                        let variance = sigma[k] * sigma[k] * g_squared[k];
                        l[k] = match spec.model {
                        ForwardModel::Black => snapshot[k] * ((drift[k] - 0.5 * variance) * dt + (variance * dt).sqrt() * w[k]).exp(),
                        ForwardModel::Bachelier => snapshot[k] + drift[k] * dt + (variance * dt).sqrt() * w[k],
                        };
                    }
                };
                advance(l, &drift);
                if spec.discretization == Discretization::PredictorCorrector {
                    // the drift again at the predicted rates, and the step with the mean of both drifts
                    // and the same shocks (QuantLib's LogNormalFwdRatePc and NormalFwdRatePc)
                    let predicted = drift_with(spec, alive, l, &g, &sigma);
                    for k in alive..n {
                        drift[k] = 0.5 * (drift[k] + predicted[k]);
                    }
                    advance(l, &drift);
                }
            }
        };
        let mut segment = |l: &mut Vec<f64>, from: f64, to: f64, alive: usize, accruing: Option<usize>, seen: &mut Option<Vec<f64>>| {
            match observation {
                Some(t) if seen.is_none() && from < t && t <= to => {
                    evolve(l, from, t, alive, accruing);
                    *seen = Some(l.clone());
                    if t < to {
                        evolve(l, t, to, alive, accruing);
                    }
                }
                _ => evolve(l, from, to, alive, accruing),
            }
        };
        if spec.times[0] > 0.0 {
            segment(&mut l, 0.0, spec.times[0], 0, None, &mut seen);
        }
        // complete periods keep their fixings; the state at their end dates holds them
        for (k, row) in path_state.iter_mut().enumerate().take(open + 1) {
            row.clone_from(&l);
            if k > 0 {
                row[..k.min(n)].fill(f64::NAN);
                row[k - 1] = l[k - 1];
            }
        }
        for k in open..n {
            if spec.backward_looking {
                // rate k accrues over [T_k, T_{k+1}] and is known at its end; a period started before
                // today moves only from today, its fixings so far being in `discount_start`
                segment(&mut l, spec.times[k].max(0.0), spec.times[k + 1], k, Some(k), &mut seen);
                path_numeraire[k + 1] = path_numeraire[k] * (1.0 + spec.accrual(k) * l[k]);
                path_state[k + 1][k..n].copy_from_slice(&l[k..n]);
            } else {
                path_numeraire[k + 1] = path_numeraire[k] * (1.0 + spec.accrual(k) * l[k]);
                if k + 1 < n {
                    segment(&mut l, spec.times[k], spec.times[k + 1], k + 1, None, &mut seen);
                    path_state[k + 1][(k + 1)..n].copy_from_slice(&l[(k + 1)..n]);
                }
            }
        }
        if spec.measure == Measure::Terminal {
            // N(T_k) = P(T_k, T_n) / P(0, T_n) from the rates alive at T_k; a period under way keeps its
            // bookkeeping value 1 / A(0), as under the spot measure
            let last = spec.discount(n);
            for k in open..=n {
                if k == open && spec.times[k] < 0.0 {
                    continue;
                }
                let bond = (k..n).fold(1.0, |p, j| p / (1.0 + spec.accrual(j) * path_state[k][j]));
                path_numeraire[k] = bond / last;
            }
        }
        state.push(path_state);
        numeraire.push(path_numeraire);
        if observation.is_some() {
            // an observation after the last rate's start of a forward-looking model sees the rates as fixed
            observed.push(seen.unwrap_or(l));
        }
    }
    Ok(LmmPaths { state, numeraire, observation, observed })
}

/// A Monte Carlo value with its standard error.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LmmEstimate {
    pub value: f64,
    pub standard_error: f64,
}

fn estimate(values: &[f64]) -> LmmEstimate {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / (n - 1.0).max(1.0);
    LmmEstimate { value: mean, standard_error: (variance / n).sqrt() }
}

/// The swap rate from `T_a` to `T_b` and its annuity, at `T_a` on a path or today (`k` the index of
/// the time, `forwards` the forwards then, from index `a`).
/// The drift of `ln L_k` under the spot measure without the `-sigma_k^2 / 2` term, per year, for the
/// forwards from `alive` (the first not yet fixed) on: `sigma_k sum_{j = alive}^{k} tau_j rho_kj sigma_j L_j
/// / (1 + tau_j L_j)`. Entries before `alive` are 0 (QuantLib `LMMDriftCalculator::computePlain` with the
/// numeraire at `alive`).
fn drift_of(spec: &LmmSpec, alive: usize, forwards: &[f64]) -> Vec<f64> {
    drift_scaled(spec, alive, forwards, &vec![1.0; spec.count()])
}

/// The drift with each volatility scaled by `g` (the FMM's decay of an accruing rate).
fn drift_scaled(spec: &LmmSpec, alive: usize, forwards: &[f64], g: &[f64]) -> Vec<f64> {
    drift_with(spec, alive, forwards, g, &spec.volatilities)
}

/// The drift with given volatilities.
fn drift_with(spec: &LmmSpec, alive: usize, forwards: &[f64], g: &[f64], sigma: &[f64]) -> Vec<f64> {
    let n = spec.count();
    let mut drift = vec![0.0; n];
    for k in alive..n {
        let mut sum = 0.0;
        let (others, sign) = match spec.measure {
            Measure::Spot => (alive..k + 1, 1.0),
            Measure::Terminal => (k + 1..n, -1.0),
        };
        for j in others {
            let tau = spec.accrual(j);
            let level = if spec.model == ForwardModel::Black { forwards[j] } else { 1.0 };
            sum += tau * spec.correlation[k * n + j] * sigma[j] * g[j] * level / (1.0 + tau * forwards[j]);
        }
        drift[k] = sign * sigma[k] * g[k] * sum;
    }
    drift
}

/// The drift for the forwards `forwards` under the model's measure (see [`Measure`]).
pub fn compute_lmm_drift(spec: &LmmSpec, alive: usize, forwards: &[f64]) -> Result<Vec<f64>, Error> {
    spec.check()?;
    if forwards.len() != spec.count() || alive >= spec.count() {
        return invalid(format!("give {} forwards and a first alive index below it", spec.count()));
    }
    Ok(drift_of(spec, alive, forwards))
}

/// The swap rate from `T_a` to `T_b` and its annuity per unit discount at `T_a`, for given forwards.
pub fn compute_lmm_swap_rate(spec: &LmmSpec, a: usize, b: usize, forwards: &[f64]) -> Result<(f64, f64), Error> {
    spec.check()?;
    if !(a < b && b <= spec.count()) || forwards.len() != spec.count() {
        return invalid(format!("the swap must run from T_a to T_b with a < b <= {}", spec.count()));
    }
    Ok(swap_rate_and_annuity(spec, a, b, forwards))
}

fn swap_rate_and_annuity(spec: &LmmSpec, a: usize, b: usize, forwards: &[f64]) -> (f64, f64) {
    let (mut discount, mut annuity) = (1.0, 0.0);
    for i in a..b {
        discount /= 1.0 + spec.accrual(i) * forwards[i];
        annuity += spec.accrual(i) * discount;
    }
    ((1.0 - discount) / annuity, annuity)
}

/// A payer swaption exercised at `T_a` into the swap paying `strike` and receiving the forwards from
/// `T_a` to `T_b`, per unit notional, on the paths.
pub fn compute_lmm_swaption_value(spec: &LmmSpec, paths: &LmmPaths, a: usize, b: usize, strike: f64) -> Result<LmmEstimate, Error> {
    spec.check()?;
    let n = spec.count();
    if !(a < b && b <= n) {
        return invalid(format!("the swap must run from T_a to T_b with a < b <= {n}"));
    }
    if spec.times[a] < 0.0 {
        return invalid(format!("the exercise T_a = {} is before today; give an exercise time with compute_lmm_swaption_value_at", spec.times[a]));
    }
    let values: Vec<f64> = paths
        .state
        .iter()
        .zip(&paths.numeraire)
        .map(|(state, numeraire)| {
            let (rate, annuity) = swap_rate_and_annuity(spec, a, b, &state[a]);
            annuity * (rate - strike).max(0.0) / numeraire[a]
        })
        .collect();
    Ok(estimate(&values))
}

/// A payer swaption exercised at the paths' observation time `t_e` (between 0 and `T_{a+1}`) into the
/// swap from `T_a` to `T_b`. At `t_e` the swap's payoff `A (S - K)^+` is known in units of
/// `P(t_e, T_m)`, where `T_m` is the first tenor date at or after `t_e`: the discount ratios
/// `P(t_e, T_{j+1}) / P(t_e, T_m)` are products of the rates at `t_e`. The value is then
/// `E[Y / N(T_m)]`, since `P(t_e, T_m) / N(t_e) = E_{t_e}[1 / N(T_m)]` for the model's numeraire `N`. For a backward-looking first
/// rate exercised during its period, its value at `t_e` is its forward, the fixings so far and the
/// expected rest.
pub fn compute_lmm_swaption_value_at(spec: &LmmSpec, paths: &LmmPaths, a: usize, b: usize, strike: f64) -> Result<LmmEstimate, Error> {
    spec.check()?;
    let n = spec.count();
    let Some(expiry) = paths.observation else {
        return invalid("the paths have no observation time; use generate_lmm_paths_observed".into());
    };
    if !(a < b && b <= n) {
        return invalid(format!("the swap must run from T_a to T_b with a < b <= {n}"));
    }
    if a < spec.first_open_period() {
        return invalid(format!("the swap starts in a period that ended before today ({a})"));
    }
    if expiry > spec.times[a + 1] {
        return invalid(format!("the exercise {expiry} is after the end of the swap's first period {}", spec.times[a + 1]));
    }
    let m = spec.times.iter().position(|&t| t >= expiry).expect("the expiry is not after the last time");
    let values: Vec<f64> = paths
        .observed
        .iter()
        .zip(&paths.numeraire)
        .map(|(rates, numeraire)| {
            // P(t_e, T_{j+1}) / P(t_e, T_m) for the swap's payments
            let (mut annuity, mut floating) = (0.0, 0.0);
            for j in a..b {
                let ratio: f64 = (m..=j).map(|l| 1.0 / (1.0 + spec.accrual(l) * rates[l])).product();
                annuity += spec.accrual(j) * ratio;
                floating += spec.accrual(j) * rates[j] * ratio;
            }
            (floating - strike * annuity).max(0.0) / numeraire[m]
        })
        .collect();
    Ok(estimate(&values))
}

/// A caplet on `L_i` paid at `T_{i+1}`, per unit notional, on the paths.
pub fn compute_lmm_caplet_value(spec: &LmmSpec, paths: &LmmPaths, i: usize, strike: f64) -> Result<LmmEstimate, Error> {
    spec.check()?;
    if i >= spec.count() {
        return invalid(format!("no forward {i}"));
    }
    if i < spec.first_open_period() {
        return invalid(format!("the period of forward {i} ended before today: its caplet is fixed and paid"));
    }
    let tau = spec.accrual(i);
    let values: Vec<f64> = paths
        .state
        .iter()
        .zip(&paths.numeraire)
        .map(|(state, numeraire)| {
            // a forward-looking rate is known at T_i, a backward-looking one at T_{i+1}
            let fixing = if spec.backward_looking { state[i + 1][i] } else { state[i][i] };
            tau * (fixing - strike).max(0.0) / numeraire[i + 1]
        })
        .collect();
    Ok(estimate(&values))
}

/// Rebonato's approximation of the volatility of the swaption from `T_a` to `T_b`, with the weights
/// frozen today: lognormal, `v^2 S^2 = sum_ij w_i w_j L_i L_j rho_ij sigma_i sigma_j`, for Black; normal,
/// `v^2 = sum_ij w_i w_j rho_ij sigma_i sigma_j`, for Bachelier. For backward-looking rates (FMM) the formula is the same:
/// the rates of a swap starting at `T_a` start accruing at or after `T_a`, so their volatility does not
/// decay before the exercise.
pub fn compute_rebonato_swaption_volatility(spec: &LmmSpec, a: usize, b: usize) -> Result<f64, Error> {
    spec.check()?;
    compute_rebonato_swaption_volatility_at(spec, a, b, spec.times.get(a).copied().unwrap_or(f64::NAN))
}

/// The volatility of the rate `i` relative to its level at time `u`: 1 until the rate starts (or
/// fixes, for a forward-looking rate), then, for a backward-looking rate, falling linearly to 0 at the
/// end of its period.
fn decay(spec: &LmmSpec, i: usize, u: f64) -> f64 {
    let (start, end) = (spec.times[i], spec.times[i + 1]);
    if u <= start {
        1.0
    } else if spec.backward_looking && u < end {
        (end - u) / (end - start)
    } else {
        0.0
    }
}

/// `int_0^t g_i(u) g_j(u) du / t`: Simpson's rule between the breakpoints, exact for these pieces.
fn decay_overlap(spec: &LmmSpec, i: usize, j: usize, t: f64) -> f64 {
    if t <= spec.times[i].min(spec.times[j]) {
        return 1.0;
    }
    let mut points = vec![0.0, t];
    points.extend([spec.times[i], spec.times[i + 1], spec.times[j], spec.times[j + 1]].iter().filter(|&&x| x > 0.0 && x < t));
    points.sort_by(f64::total_cmp);
    let product = |u: f64| decay(spec, i, u) * decay(spec, j, u);
    let mut integral = 0.0;
    for w in points.windows(2) {
        let (x0, x1) = (w[0], w[1]);
        // the decay is 1 or 0 at a breakpoint from the left: evaluate just inside the piece
        let inside = |x: f64| product(x.clamp(x0 + 1e-15 * (x1 - x0), x1 - 1e-15 * (x1 - x0)));
        integral += (x1 - x0) / 6.0 * (inside(x0) + 4.0 * product((x0 + x1) / 2.0) + inside(x1));
    }
    integral / t
}

/// `int_0^t sigma_i(u) sigma_j(u) g_i(u) g_j(u) du / t`. With constant volatilities it is
/// `sigma_i sigma_j` times [`decay_overlap`]; with a grid, the tenor dates join the breakpoints, so the
/// pieces stay quadratic and Simpson's rule exact.
fn covariance_overlap(spec: &LmmSpec, i: usize, j: usize, t: f64) -> f64 {
    if spec.volatility_grid.is_none() {
        return spec.volatilities[i] * spec.volatilities[j] * decay_overlap(spec, i, j, t);
    }
    let mut points = vec![0.0, t];
    points.extend(spec.times.iter().filter(|&&x| x > 0.0 && x < t));
    points.sort_by(f64::total_cmp);
    points.dedup();
    let mut integral = 0.0;
    for w in points.windows(2) {
        let (x0, x1) = (w[0], w[1]);
        let middle = (x0 + x1) / 2.0;
        let sigma = spec.volatility_at(i, middle) * spec.volatility_at(j, middle);
        let product = |u: f64| decay(spec, i, u) * decay(spec, j, u);
        let inside = |x: f64| product(x.clamp(x0 + 1e-15 * (x1 - x0), x1 - 1e-15 * (x1 - x0)));
        integral += sigma * (x1 - x0) / 6.0 * (inside(x0) + 4.0 * product(middle) + inside(x1));
    }
    integral / t
}

/// The variance of rate `i` from today to `t`, `int_0^t sigma_i^2 g_i^2 du` (the total, not per year).
pub fn compute_lmm_rate_variance(spec: &LmmSpec, i: usize, t: f64) -> Result<f64, Error> {
    spec.check()?;
    if i >= spec.count() || !(t > 0.0) {
        return invalid(format!("give a rate below {} and a time after 0", spec.count()));
    }
    Ok(covariance_overlap(spec, i, i, t) * t)
}

/// The caplet on rate `i` by its formula (Black or Bachelier) with the rate's total variance to its
/// fixing (`T_i`, or `T_{i+1}` for a backward-looking rate): the reference value of
/// [`compute_lmm_caplet_value`], for constant and time-dependent volatilities.
pub fn compute_lmm_caplet_formula(spec: &LmmSpec, i: usize, strike: f64) -> Result<f64, Error> {
    spec.check()?;
    if i >= spec.count() {
        return invalid(format!("no forward {i}"));
    }
    if i < spec.first_open_period() {
        return invalid(format!("the period of forward {i} ended before today: its caplet is fixed and paid"));
    }
    let fixing = if spec.backward_looking { spec.times[i + 1] } else { spec.times[i] };
    let variance = if fixing > 0.0 { compute_lmm_rate_variance(spec, i, fixing)? } else { 0.0 };
    crate::lmm::compute_caplet_value(
        spec.model,
        &crate::lmm::CapletTerms {
            forward: spec.forwards[i],
            strike,
            volatility: variance.sqrt(),
            expiry: 1.0,
            accrual: spec.accrual(i),
            discount_factor: spec.discount(i + 1),
            notional: 1.0,
        },
    )
}

/// Rebonato's approximation at an exercise time `expiry`, which may lie between tenor dates: the time
/// integral of each covariance is `int_0^expiry sigma_i sigma_j g_i g_j du`, with the decay `g` of
/// backward-looking rates inside their period (Lyashenko and Mercurio). At `expiry = T_a` it is
/// [`compute_rebonato_swaption_volatility`]. The volatility is per year of `expiry`.
pub fn compute_rebonato_swaption_volatility_at(spec: &LmmSpec, a: usize, b: usize, expiry: f64) -> Result<f64, Error> {
    spec.check()?;
    if !(expiry > 0.0) || (a < spec.count() && expiry > spec.times[a + 1]) {
        return invalid(format!("the exercise time must be after 0 and not after the end of the swap's first period, not {expiry}"));
    }
    let n = spec.count();
    if !(a < b && b <= n) {
        return invalid(format!("the swap must run from T_a to T_b with a < b <= {n}"));
    }
    let (rate, annuity) = swap_rate_and_annuity(spec, a, b, &spec.forwards);
    let mut discount = 1.0;
    let weights: Vec<f64> = (a..b)
        .map(|i| {
            discount /= 1.0 + spec.accrual(i) * spec.forwards[i];
            spec.accrual(i) * discount / annuity
        })
        .collect();
    let mut sum = 0.0;
    for i in a..b {
        for j in a..b {
            let levels = if spec.model == ForwardModel::Black { spec.forwards[i] * spec.forwards[j] } else { 1.0 };
            sum += weights[i - a] * weights[j - a] * levels * spec.correlation[i * n + j] * covariance_overlap(spec, i, j, expiry);
        }
    }
    // a lognormal volatility for Black, a normal one for Bachelier
    Ok(if spec.model == ForwardModel::Black { sum.sqrt() / rate } else { sum.sqrt() })
}

/// The value of the swaption at a volatility by the model's formula (Black or Bachelier), per unit
/// notional, today.
pub fn compute_formula_swaption_value(spec: &LmmSpec, a: usize, b: usize, strike: f64, volatility: f64) -> Result<f64, Error> {
    spec.check()?;
    compute_formula_swaption_value_at(spec, a, b, strike, volatility, spec.times[a])
}

/// The formula value of the swaption exercised at `expiry`, with a volatility per year of `expiry`.
pub fn compute_formula_swaption_value_at(spec: &LmmSpec, a: usize, b: usize, strike: f64, volatility: f64, expiry: f64) -> Result<f64, Error> {
    spec.check()?;
    if a < spec.first_open_period() {
        return invalid(format!("the swap starts in a period that ended before today ({a})"));
    }
    let (rate, annuity) = swap_rate_and_annuity(spec, a, b, &spec.forwards);
    let value = compute_forward_value(
        spec.model,
        OptionType::Call,
        &ForwardInputs { forward: rate, strike, volatility, time: expiry, discount: 1.0 },
    )?;
    Ok(spec.discount(a) * annuity * value)
}

/// A swaption volatility to fit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwaptionQuote {
    pub start: usize,
    pub end: usize,
    pub volatility: f64,
}

/// The fit of the correlation.
#[derive(Debug, Clone, PartialEq)]
pub struct CorrelationFit {
    pub beta: f64,
    /// Rebonato volatilities at the fitted `beta`, in the order of the quotes.
    pub fitted: Vec<f64>,
    pub root_mean_square_error: f64,
}

/// Fits `beta` of the exponential correlation to swaption volatilities by least squares on the
/// Rebonato volatilities, with the forwards' volatilities fixed (from the caplets: with a constant
/// volatility per forward, `sigma_i` is the Black volatility of the caplet on `L_i`). The search is a
/// golden section on `[0, beta_max]`.
pub fn calibrate_lmm_correlation(spec: &LmmSpec, quotes: &[SwaptionQuote], beta_max: f64) -> Result<CorrelationFit, Error> {
    if quotes.is_empty() {
        return invalid("no swaption volatilities given".into());
    }
    if !(beta_max > 0.0) || !beta_max.is_finite() {
        return invalid("beta_max must be positive and finite".into());
    }
    let fixing_times = &spec.times[..spec.count()];
    let fitted_at = |beta: f64| -> Result<Vec<f64>, Error> {
        let full = create_exponential_correlation(fixing_times, beta)?;
        // with fewer factors, the reduced correlation of the exponential one
        let correlation = match spec.factors {
            None => full,
            Some(m) => create_reduced_correlation(&full, spec.count(), m)?,
        };
        let trial = LmmSpec { correlation, ..spec.clone() };
        quotes.iter().map(|q| compute_rebonato_swaption_volatility(&trial, q.start, q.end)).collect()
    };
    let error_at = |beta: f64| -> Result<f64, Error> {
        Ok(fitted_at(beta)?.iter().zip(quotes).map(|(f, q)| (f - q.volatility) * (f - q.volatility)).sum())
    };
    let ratio = (5f64.sqrt() - 1.0) / 2.0;
    let (mut lo, mut hi) = (0.0, beta_max);
    let mut x1 = hi - ratio * (hi - lo);
    let mut x2 = lo + ratio * (hi - lo);
    let (mut f1, mut f2) = (error_at(x1)?, error_at(x2)?);
    for _ in 0..200 {
        if hi - lo < 1e-12 * beta_max.max(1.0) {
            break;
        }
        if f1 <= f2 {
            hi = x2;
            x2 = x1;
            f2 = f1;
            x1 = hi - ratio * (hi - lo);
            f1 = error_at(x1)?;
        } else {
            lo = x1;
            x1 = x2;
            f1 = f2;
            x2 = lo + ratio * (hi - lo);
            f2 = error_at(x2)?;
        }
    }
    let beta = (lo + hi) / 2.0;
    let fitted = fitted_at(beta)?;
    let root_mean_square_error = (error_at(beta)? / quotes.len() as f64).sqrt();
    Ok(CorrelationFit { beta, fitted, root_mean_square_error })
}

/// The fit of the volatilities to a swaption matrix.
#[derive(Debug, Clone, PartialEq)]
pub struct VolatilityFit {
    /// The model with the fitted `volatility_grid` (and, in `volatilities`, each rate's volatility on
    /// the interval that ends at its fixing).
    pub spec: LmmSpec,
    /// Rebonato volatilities of the fitted model, in the order of the quotes.
    pub fitted: Vec<f64>,
    /// The largest `|fitted - quoted|`.
    pub max_error: f64,
}

/// Fits the volatility grid to swaption volatilities one by one (the cascade calibration of Brigo and
/// Mercurio, *Interest Rate Models*), with the correlation fixed. The quotes must hold every
/// swaption `a x b` with `a < b <= n` whose exercise `T_a` is after today, once each (the caplets are the
/// swaptions `a x (a + 1)`). Rebonato's volatility of `a x b` depends on rate `i` in `a..b` over the
/// intervals `0..=a`; in the order `a = 0, 1, ...` and `b = a + 1, ...`, only rate `b - 1` on interval
/// `a` is new, and the squared volatility is a quadratic in it, solved for its larger root. A quote
/// below what the earlier ones imply has no non-negative root and is an error. The cells that no
/// swaption reaches keep the nearest fitted cell of their row: the intervals after a rate's fixing (used
/// by a backward-looking rate in its own period) and, when `T_0 = 0`, interval 0.
pub fn calibrate_lmm_volatilities(spec: &LmmSpec, quotes: &[SwaptionQuote]) -> Result<VolatilityFit, Error> {
    spec.check()?;
    let n = spec.count();
    if spec.times[0] < 0.0 {
        return invalid("the cascade needs the first period to start today or later".into());
    }
    // the first exercise after today
    let first = if spec.times[0] > 0.0 { 0 } else { 1 };
    let mut quoted = vec![vec![f64::NAN; n + 1]; n + 1];
    for q in quotes {
        if !(q.start < q.end && q.end <= n && q.start >= first) {
            return invalid(format!("a swaption runs from T_a to T_b with {first} <= a < b <= {n}, not {} x {}", q.start, q.end));
        }
        if !(q.volatility > 0.0) || !q.volatility.is_finite() {
            return invalid(format!("the volatility of {} x {} must be positive and finite", q.start, q.end));
        }
        if !quoted[q.start][q.end].is_nan() {
            return invalid(format!("the swaption {} x {} is given twice", q.start, q.end));
        }
        quoted[q.start][q.end] = q.volatility;
    }
    for a in first..n {
        for b in a + 1..=n {
            if quoted[a][b].is_nan() {
                return invalid(format!("the cascade needs every swaption a x b with {first} <= a < b <= {n}: {a} x {b} is missing"));
            }
        }
    }
    let width = n + 1;
    let mut grid: Vec<f64> = match &spec.volatility_grid {
        Some(g) => g.clone(),
        None => (0..n).flat_map(|i| vec![spec.volatilities[i]; width]).collect(),
    };
    let mut trial = spec.clone();
    for a in first..n {
        for b in a + 1..=n {
            let target = quoted[a][b];
            let cell = (b - 1) * width + a;
            // the squared volatility at x = 0, s, 2s: exactly a quadratic in the cell
            let s = target;
            let mut squared = [0.0; 3];
            for (m, value) in squared.iter_mut().enumerate() {
                grid[cell] = m as f64 * s;
                trial.volatility_grid = Some(grid.clone());
                let v = compute_rebonato_swaption_volatility(&trial, a, b)?;
                *value = v * v;
            }
            let quadratic = (squared[2] - 2.0 * squared[1] + squared[0]) / 2.0;
            let linear = squared[1] - squared[0] - quadratic;
            let constant = squared[0] - target * target;
            let discriminant = linear * linear - 4.0 * quadratic * constant;
            let root = (-linear + discriminant.max(0.0).sqrt()) / (2.0 * quadratic);
            if !(quadratic > 0.0) || discriminant < 0.0 || !(root >= 0.0) || !root.is_finite() {
                return invalid(format!(
                    "the swaption {a} x {b} cannot be fitted: its volatility {target} is below what the earlier swaptions give its rates"
                ));
            }
            grid[cell] = root * s;
        }
    }
    // the cells no swaption reaches: the nearest fitted one in the row
    for i in first..n {
        for k in 0..width {
            if k < first {
                grid[i * width + k] = grid[i * width + first];
            } else if k > i {
                grid[i * width + k] = grid[i * width + i];
            }
        }
    }
    let mut volatilities = spec.volatilities.clone();
    for i in first..n {
        volatilities[i] = grid[i * width + i];
    }
    let fitted_spec = LmmSpec { volatilities, volatility_grid: Some(grid), ..spec.clone() };
    let fitted: Vec<f64> = quotes.iter().map(|q| compute_rebonato_swaption_volatility(&fitted_spec, q.start, q.end)).collect::<Result<_, _>>()?;
    let max_error = fitted.iter().zip(quotes).map(|(f, q)| (f - q.volatility).abs()).fold(0.0, f64::max);
    Ok(VolatilityFit { spec: fitted_spec, fitted, max_error })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lmm::{compute_caplet_value, CapletTerms};

    fn spec(beta: f64) -> LmmSpec {
        let times: Vec<f64> = (0..=10).map(|k| 1.0 + 0.5 * k as f64).collect();
        let forwards: Vec<f64> = (0..10).map(|k| 0.02 + 0.001 * k as f64).collect();
        let volatilities: Vec<f64> = (0..10).map(|k| 0.25 - 0.005 * k as f64).collect();
        let correlation = create_exponential_correlation(&times[..10], beta).unwrap();
        LmmSpec { times, forwards, volatilities, correlation, discount_start: (-0.02f64).exp(), backward_looking: false, model: ForwardModel::Black, volatility_grid: None, factors: None, measure: Measure::Spot, discretization: Discretization::Euler }
    }

    const PATHS: usize = 2000;

    #[test]
    fn discounted_zero_bonds_are_martingales_and_caplets_follow_black() {
        let s = spec(0.2);
        let paths = generate_lmm_paths(&s, PATHS, 4, 11).unwrap();
        // E[1 / B(T_k)] = P(0, T_k): no arbitrage under the spot measure
        for k in [1usize, 5, 10] {
            let values: Vec<f64> = paths.numeraire.iter().map(|b| 1.0 / b[k]).collect();
            let e = estimate(&values);
            assert!((e.value - s.discount(k)).abs() < 4.0 * e.standard_error + 1e-12, "P(0,T_{k}): {} against {} ± {}", s.discount(k), e.value, e.standard_error);
        }
        // each caplet is Black's formula under its own forward measure
        for i in [0usize, 4, 9] {
            let strike = s.forwards[i];
            let mc = compute_lmm_caplet_value(&s, &paths, i, strike).unwrap();
            let black = compute_caplet_value(
                ForwardModel::Black,
                &CapletTerms {
                    forward: s.forwards[i],
                    strike,
                    volatility: s.volatilities[i],
                    expiry: s.times[i],
                    accrual: s.accrual(i),
                    discount_factor: s.discount(i + 1),
                    notional: 1.0,
                },
            )
            .unwrap();
            assert!((mc.value - black).abs() < 4.0 * mc.standard_error, "caplet {i}: {black} against {} ± {}", mc.value, mc.standard_error);
        }
    }

    #[test]
    fn rebonato_is_close_to_the_simulated_swaption_and_the_calibration_recovers_beta() {
        let s = spec(0.3);
        let paths = generate_lmm_paths(&s, PATHS, 4, 13).unwrap();
        for (a, b) in [(2usize, 6usize), (4, 10)] {
            let (rate, _) = swap_rate_and_annuity(&s, a, b, &s.forwards);
            let mc = compute_lmm_swaption_value(&s, &paths, a, b, rate).unwrap();
            let approx = compute_formula_swaption_value(&s, a, b, rate, compute_rebonato_swaption_volatility(&s, a, b).unwrap()).unwrap();
            // Rebonato is an approximation: allow the simulation error and 2% of the value
            assert!((mc.value - approx).abs() < 4.0 * mc.standard_error + 0.02 * approx, "{a}x{b}: {approx} against {} ± {}", mc.value, mc.standard_error);
        }
        // quotes made at beta = 0.3 give back 0.3
        let quotes: Vec<SwaptionQuote> = [(2usize, 6usize), (4, 10), (1, 8), (6, 10)]
            .iter()
            .map(|&(a, b)| SwaptionQuote { start: a, end: b, volatility: compute_rebonato_swaption_volatility(&s, a, b).unwrap() })
            .collect();
        let fit = calibrate_lmm_correlation(&spec(0.9), &quotes, 5.0).unwrap();
        assert!((fit.beta - 0.3).abs() < 1e-6, "{}", fit.beta);
        assert!(fit.root_mean_square_error < 1e-9);
    }

    #[test]
    fn backward_looking_paths_keep_bonds_martingales_and_reproduce_the_fmm_caplet() {
        use crate::fmm::{compute_backward_caplet_value, BackwardCapletTerms};
        let s = LmmSpec { backward_looking: true, ..spec(0.2) };
        let paths = generate_lmm_paths(&s, PATHS, 8, 17).unwrap();
        // the accruing rate is fixed at the end of its period
        assert!(paths.state.iter().all(|st| st[1][0].is_finite() && st[0][0].is_finite()));
        for k in [1usize, 5, 10] {
            let values: Vec<f64> = paths.numeraire.iter().map(|b| 1.0 / b[k]).collect();
            let e = estimate(&values);
            assert!((e.value - s.discount(k)).abs() < 4.0 * e.standard_error + 1e-12, "P(0,T_{k}): {} against {} ± {}", s.discount(k), e.value, e.standard_error);
        }
        for i in [0usize, 4, 9] {
            let strike = s.forwards[i];
            let mc = compute_lmm_caplet_value(&s, &paths, i, strike).unwrap();
            let fmm = compute_backward_caplet_value(
                ForwardModel::Black,
                &BackwardCapletTerms {
                    forward: s.forwards[i],
                    strike,
                    volatility: s.volatilities[i],
                    time_to_start: s.times[i],
                    time_to_end: s.times[i + 1],
                    accrual: s.accrual(i),
                    discount_factor: s.discount(i + 1),
                    notional: 1.0,
                },
            )
            .unwrap();
            assert!((mc.value - fmm).abs() < 4.0 * mc.standard_error, "caplet {i}: {fmm} against {} ± {}", mc.value, mc.standard_error);
        }
        // the backward-looking caplet is worth more than the forward-looking one on the same rate
        let forward_looking = generate_lmm_paths(&spec(0.2), PATHS, 8, 17).unwrap();
        let (b, f) = (compute_lmm_caplet_value(&s, &paths, 4, s.forwards[4]).unwrap(), compute_lmm_caplet_value(&spec(0.2), &forward_looking, 4, s.forwards[4]).unwrap());
        assert!(b.value > f.value);
    }

    #[test]
    fn a_period_under_way_moves_only_its_unfixed_part() {
        use crate::fmm::{compute_backward_caplet_value, compute_backward_forward_rate, BackwardCapletTerms};
        // the first quarter started a month ago; its fixings so far compound to 1.0017, and the curve
        // discounts its end (two months away) at 0.9965
        let times: Vec<f64> = (0..=8).map(|k| -1.0 / 12.0 + 0.25 * k as f64).collect();
        let realized = 1.0017;
        let first = compute_backward_forward_rate(realized, 1.0, 0.9965, 0.25).unwrap();
        let mut forwards: Vec<f64> = (0..8).map(|k| 0.02 + 0.001 * k as f64).collect();
        forwards[0] = first;
        let s = LmmSpec {
            correlation: create_exponential_correlation(&times[..8], 0.3).unwrap(),
            times,
            forwards,
            volatilities: vec![0.3; 8],
            discount_start: realized,
            backward_looking: true,
            model: ForwardModel::Black,
            volatility_grid: None,
            factors: None,
            measure: Measure::Spot,
            discretization: Discretization::Euler,
        };
        assert!((s.discount(1) - 0.9965).abs() < 1e-15);
        let paths = generate_lmm_paths(&s, PATHS, 8, 23).unwrap();
        for k in [1usize, 4, 8] {
            let values: Vec<f64> = paths.numeraire.iter().map(|b| 1.0 / b[k]).collect();
            let e = estimate(&values);
            assert!((e.value - s.discount(k)).abs() < 4.0 * e.standard_error + 1e-12, "P(0,T_{k}): {} against {} ± {}", s.discount(k), e.value, e.standard_error);
        }
        for i in [0usize, 3] {
            let strike = s.forwards[i] + 0.0005;
            let mc = compute_lmm_caplet_value(&s, &paths, i, strike).unwrap();
            let fmm = compute_backward_caplet_value(
                ForwardModel::Black,
                &BackwardCapletTerms {
                    forward: s.forwards[i],
                    strike,
                    volatility: s.volatilities[i],
                    time_to_start: s.times[i],
                    time_to_end: s.times[i + 1],
                    accrual: s.accrual(i),
                    discount_factor: s.discount(i + 1),
                    notional: 1.0,
                },
            )
            .unwrap();
            assert!((mc.value - fmm).abs() < 4.0 * mc.standard_error, "caplet {i}: {fmm} against {} ± {}", mc.value, mc.standard_error);
        }
        // only the remaining two months are random: the spread of the first rate's fixing is that of
        // the in-period variance time (T_1)^3 / (3 tau^2), far below a whole period's
        let fixings: Vec<f64> = paths.state.iter().map(|st| st[1][0].ln()).collect();
        let mean = fixings.iter().sum::<f64>() / fixings.len() as f64;
        let variance = fixings.iter().map(|f| (f - mean) * (f - mean)).sum::<f64>() / (fixings.len() - 1) as f64;
        let expected = 0.09 * (2.0f64 / 12.0).powi(3) / (3.0 * 0.0625);
        assert!((variance / expected - 1.0).abs() < 0.1, "{variance} against {expected}");
        // a forward-looking rate cannot have started
        assert!(generate_lmm_paths(&LmmSpec { backward_looking: false, ..s }, 10, 4, 1).is_err());
    }

    #[test]
    fn normal_rates_keep_bonds_martingales_and_follow_bachelier() {
        use crate::fmm::{compute_backward_caplet_value, BackwardCapletTerms};
        for backward_looking in [false, true] {
            // normal volatilities of about 60bp a year; rates near 0 can turn negative
            let s = LmmSpec { volatilities: vec![0.006; 10], model: ForwardModel::Bachelier, backward_looking, ..spec(0.2) };
            let paths = generate_lmm_paths(&s, PATHS, 8, 29).unwrap();
            for k in [1usize, 5, 10] {
                let values: Vec<f64> = paths.numeraire.iter().map(|b| 1.0 / b[k]).collect();
                let e = estimate(&values);
                assert!((e.value - s.discount(k)).abs() < 4.0 * e.standard_error + 1e-12, "{backward_looking} P(0,T_{k}): {} against {} ± {}", s.discount(k), e.value, e.standard_error);
            }
            for i in [0usize, 4, 9] {
                let strike = s.forwards[i];
                let mc = compute_lmm_caplet_value(&s, &paths, i, strike).unwrap();
                let terms = BackwardCapletTerms {
                    forward: s.forwards[i],
                    strike,
                    volatility: s.volatilities[i],
                    // a forward-looking rate is a backward-looking one whose period has length 0
                    time_to_start: s.times[i],
                    time_to_end: if backward_looking { s.times[i + 1] } else { s.times[i] + 1e-12 },
                    accrual: s.accrual(i),
                    discount_factor: s.discount(i + 1),
                    notional: 1.0,
                };
                let bachelier = compute_backward_caplet_value(ForwardModel::Bachelier, &terms).unwrap();
                assert!((mc.value - bachelier).abs() < 4.0 * mc.standard_error, "{backward_looking} caplet {i}: {bachelier} against {} ± {}", mc.value, mc.standard_error);
            }
            for (a, b) in [(2usize, 6usize), (4, 10)] {
                let (rate, _) = swap_rate_and_annuity(&s, a, b, &s.forwards);
                let mc = compute_lmm_swaption_value(&s, &paths, a, b, rate).unwrap();
                let approx = compute_formula_swaption_value(&s, a, b, rate, compute_rebonato_swaption_volatility(&s, a, b).unwrap()).unwrap();
                assert!((mc.value - approx).abs() < 4.0 * mc.standard_error + 0.02 * approx, "{backward_looking} {a}x{b}: {approx} against {} ± {}", mc.value, mc.standard_error);
            }
        }
        // the normal model accepts a negative forward
        let mut s = LmmSpec { volatilities: vec![0.006; 10], model: ForwardModel::Bachelier, ..spec(0.2) };
        s.forwards[0] = -0.002;
        assert!(generate_lmm_paths(&s, 10, 4, 1).is_ok());
    }

    #[test]
    fn backward_looking_swaptions_follow_the_forward_looking_approximation() {
        // the rates of a swap starting at T_a have not started accruing at T_a, so their decay is 1 up
        // to the exercise and Rebonato's formula is the forward-looking one
        let forward_looking = spec(0.3);
        let backward_looking = LmmSpec { backward_looking: true, ..spec(0.3) };
        let fl_paths = generate_lmm_paths(&forward_looking, PATHS, 8, 31).unwrap();
        let bl_paths = generate_lmm_paths(&backward_looking, PATHS, 8, 31).unwrap();
        for (a, b) in [(2usize, 6usize), (4, 10), (1, 9)] {
            let v_fl = compute_rebonato_swaption_volatility(&forward_looking, a, b).unwrap();
            let v_bl = compute_rebonato_swaption_volatility(&backward_looking, a, b).unwrap();
            assert_eq!(v_fl, v_bl);
            let (rate, _) = swap_rate_and_annuity(&backward_looking, a, b, &backward_looking.forwards);
            let approx = compute_formula_swaption_value(&backward_looking, a, b, rate, v_bl).unwrap();
            let bl = compute_lmm_swaption_value(&backward_looking, &bl_paths, a, b, rate).unwrap();
            let fl = compute_lmm_swaption_value(&forward_looking, &fl_paths, a, b, rate).unwrap();
            assert!((bl.value - approx).abs() < 4.0 * bl.standard_error + 0.02 * approx, "{a}x{b}: {approx} against {} ± {}", bl.value, bl.standard_error);
            // the two models price the swaption alike: their spot-measure drifts differ (an accruing
            // rate stays in the drift), but not the law of the swap rate at T_a
            let combined = (bl.standard_error.powi(2) + fl.standard_error.powi(2)).sqrt();
            assert!((bl.value - fl.value).abs() < 4.0 * combined, "{a}x{b}: {} against {} ± {combined}", bl.value, fl.value);
        }
    }

    #[test]
    fn swaptions_exercised_between_tenor_dates() {
        let fmm = LmmSpec { backward_looking: true, ..spec(0.3) };
        let (a, b) = (4usize, 9usize);
        // at T_a the observed paths and the formula are those of the usual swaption
        let at_start = generate_lmm_paths_observed(&fmm, PATHS, 8, 37, Some(fmm.times[a])).unwrap();
        let (rate, _) = swap_rate_and_annuity(&fmm, a, b, &fmm.forwards);
        let usual = compute_lmm_swaption_value(&fmm, &at_start, a, b, rate).unwrap();
        let observed = compute_lmm_swaption_value_at(&fmm, &at_start, a, b, rate).unwrap();
        assert!((usual.value - observed.value).abs() < 1e-14, "{} against {}", usual.value, observed.value);
        assert_eq!(compute_rebonato_swaption_volatility(&fmm, a, b).unwrap(), compute_rebonato_swaption_volatility_at(&fmm, a, b, fmm.times[a]).unwrap());
        // inside the first period of the swap, and inside the period before its start
        for expiry in [fmm.times[a] + 0.5 * fmm.accrual(a), fmm.times[a] + 0.9 * fmm.accrual(a), fmm.times[a] - 0.25 * fmm.accrual(a - 1)] {
            let paths = generate_lmm_paths_observed(&fmm, PATHS, 8, 41, Some(expiry)).unwrap();
            let mc = compute_lmm_swaption_value_at(&fmm, &paths, a, b, rate).unwrap();
            let v = compute_rebonato_swaption_volatility_at(&fmm, a, b, expiry).unwrap();
            let approx = compute_formula_swaption_value_at(&fmm, a, b, rate, v, expiry).unwrap();
            assert!((mc.value - approx).abs() < 4.0 * mc.standard_error + 0.02 * approx, "exercise {expiry}: {approx} against {} ± {}", mc.value, mc.standard_error);
            // no arbitrage: with a strike of 0 the swaption is the floating leg, P(0, T_a) - P(0, T_b)
            let floating = compute_lmm_swaption_value_at(&fmm, &paths, a, b, 0.0).unwrap();
            let leg = fmm.discount(a) - fmm.discount(b);
            assert!((floating.value - leg).abs() < 4.0 * floating.standard_error + 1e-12, "exercise {expiry}: {leg} against {} ± {}", floating.value, floating.standard_error);
        }
        // the decay of the first rate adds variance as the exercise moves into its period, less than
        // a whole period of full volatility would
        let at = |t: f64| compute_rebonato_swaption_volatility_at(&fmm, a, b, t).unwrap().powi(2) * t;
        let (t_a, t_mid) = (fmm.times[a], fmm.times[a] + 0.5 * fmm.accrual(a));
        assert!(at(t_mid) > at(t_a));
        let forward_looking = spec(0.3);
        let full = |t: f64| compute_rebonato_swaption_volatility_at(&forward_looking, a, b, t).unwrap().powi(2) * t;
        assert!(at(t_mid) - at(t_a) > 0.0 && full(t_a) == at(t_a));
        // the integral of g_a^2 and g_a over [0, t] in the first period, in closed form
        let (s, e, tau) = (fmm.times[a], fmm.times[a + 1], fmm.accrual(a));
        let squared = (s + (tau.powi(3) - (e - t_mid).powi(3)) / (3.0 * tau * tau)) / t_mid;
        let single = (s + (tau * tau - (e - t_mid).powi(2)) / (2.0 * tau)) / t_mid;
        assert!((decay_overlap(&fmm, a, a, t_mid) - squared).abs() < 1e-12);
        assert!((decay_overlap(&fmm, a, a + 2, t_mid) - single).abs() < 1e-12);
        assert!(compute_rebonato_swaption_volatility_at(&fmm, a, b, fmm.times[a + 1] + 0.01).is_err());
        assert!(compute_lmm_swaption_value_at(&fmm, &generate_lmm_paths(&fmm, 10, 4, 1).unwrap(), a, b, rate).is_err());
    }

    #[test]
    fn complete_periods_before_the_one_under_way_are_fixed_and_left_out() {
        use crate::fmm::{compute_backward_caplet_value, BackwardCapletTerms};
        // quarterly periods from 0.6 years ago: two are complete, the third is under way
        let times: Vec<f64> = (0..=10).map(|k| -0.6 + 0.25 * k as f64).collect();
        let realized = 1.0012;
        let mut forwards: Vec<f64> = (0..10).map(|k| 0.02 + 0.001 * k as f64).collect();
        forwards[0] = 0.0185;
        forwards[1] = 0.0190;
        let full = LmmSpec {
            correlation: create_exponential_correlation(&times[..10], 0.3).unwrap(),
            times: times.clone(),
            forwards: forwards.clone(),
            volatilities: vec![0.25; 10],
            discount_start: realized,
            backward_looking: true,
            model: ForwardModel::Black,
            volatility_grid: None,
            factors: None,
            measure: Measure::Spot,
            discretization: Discretization::Euler,
        };
        assert_eq!(full.first_open_period(), 2);
        // the same model without the complete periods
        let trimmed = LmmSpec {
            correlation: create_exponential_correlation(&times[2..10], 0.3).unwrap(),
            times: times[2..].to_vec(),
            forwards: forwards[2..].to_vec(),
            volatilities: vec![0.25; 8],
            ..full.clone()
        };
        assert_eq!(trimmed.first_open_period(), 0);
        for k in 2..=10 {
            assert_eq!(full.discount(k), trimmed.discount(k - 2));
        }
        assert!(full.discount(1).is_nan());
        assert_eq!(compute_rebonato_swaption_volatility(&full, 4, 9).unwrap(), compute_rebonato_swaption_volatility(&trimmed, 2, 7).unwrap());
        let paths = generate_lmm_paths(&full, PATHS, 8, 43).unwrap();
        // the complete periods stay at their fixings
        assert!(paths.state.iter().all(|st| st[1][0] == 0.0185 && st[2][1] == 0.0190));
        for k in [3usize, 6, 10] {
            let values: Vec<f64> = paths.numeraire.iter().map(|b| 1.0 / b[k]).collect();
            let e = estimate(&values);
            assert!((e.value - full.discount(k)).abs() < 4.0 * e.standard_error + 1e-12, "P(0,T_{k}): {} against {} ± {}", full.discount(k), e.value, e.standard_error);
        }
        for i in [2usize, 5] {
            let strike = full.forwards[i] + 0.0005;
            let mc = compute_lmm_caplet_value(&full, &paths, i, strike).unwrap();
            let formula = compute_backward_caplet_value(
                ForwardModel::Black,
                &BackwardCapletTerms {
                    forward: full.forwards[i],
                    strike,
                    volatility: 0.25,
                    time_to_start: full.times[i],
                    time_to_end: full.times[i + 1],
                    accrual: full.accrual(i),
                    discount_factor: full.discount(i + 1),
                    notional: 1.0,
                },
            )
            .unwrap();
            assert!((mc.value - formula).abs() < 4.0 * mc.standard_error, "caplet {i}: {formula} against {} ± {}", mc.value, mc.standard_error);
        }
        // the full and the trimmed models price a swaption alike
        let trimmed_paths = generate_lmm_paths(&trimmed, PATHS, 8, 47).unwrap();
        let rate = swap_rate_and_annuity(&full, 4, 9, &full.forwards).0;
        let (f, t) = (compute_lmm_swaption_value(&full, &paths, 4, 9, rate).unwrap(), compute_lmm_swaption_value(&trimmed, &trimmed_paths, 2, 7, rate).unwrap());
        assert!((f.value - t.value).abs() < 4.0 * (f.standard_error.powi(2) + t.standard_error.powi(2)).sqrt(), "{} against {}", f.value, t.value);
        // instruments of the complete periods are refused
        assert!(compute_lmm_caplet_value(&full, &paths, 1, 0.02).is_err());
        assert!(compute_lmm_swaption_value(&full, &paths, 1, 5, 0.02).is_err());
        assert!(compute_formula_swaption_value_at(&full, 1, 5, 0.02, 0.2, 0.1).is_err());
    }

    #[test]
    fn volatilities_that_change_over_time() {
        let n = 10;
        // a grid with the constant volatilities in every column gives the same paths and formulas
        let constant = spec(0.3);
        let flat_grid: Vec<f64> = (0..n).flat_map(|i| std::iter::repeat_n(constant.volatilities[i], n + 1)).collect();
        let gridded = LmmSpec { volatility_grid: Some(flat_grid), ..constant.clone() };
        let (p1, p2) = (generate_lmm_paths(&constant, 200, 4, 53).unwrap(), generate_lmm_paths(&gridded, 200, 4, 53).unwrap());
        // the same values, NaN in the same places (NaN != NaN, so compare by bits)
        let bits = |p: &LmmPaths| -> Vec<u64> { p.state.iter().flatten().flatten().chain(p.numeraire.iter().flatten()).map(|x| x.to_bits()).collect() };
        assert_eq!(bits(&p1), bits(&p2));
        assert!((compute_rebonato_swaption_volatility(&constant, 2, 7).unwrap() - compute_rebonato_swaption_volatility(&gridded, 2, 7).unwrap()).abs() < 1e-14);
        // time-homogeneous volatilities: high near the fixing, low far from it,
        // sigma_i(t) = 0.12 + 0.2 exp(-(T_i - t)), on each interval at its middle
        for backward_looking in [false, true] {
            let base = LmmSpec { backward_looking, ..spec(0.3) };
            let grid: Vec<f64> = (0..n)
                .flat_map(|i| {
                    let times = base.times.clone();
                    (0..=n).map(move |k| {
                        let (lo, hi) = (if k == 0 { 0.0 } else { times[k - 1] }, times[k]);
                        0.12 + 0.2 * (-(times[i] - (lo + hi) / 2.0).max(0.0)).exp()
                    })
                })
                .collect();
            let s = LmmSpec { volatility_grid: Some(grid), ..base };
            let paths = generate_lmm_paths(&s, PATHS, 8, 59).unwrap();
            for k in [1usize, 5, 10] {
                let values: Vec<f64> = paths.numeraire.iter().map(|b| 1.0 / b[k]).collect();
                let e = estimate(&values);
                assert!((e.value - s.discount(k)).abs() < 4.0 * e.standard_error + 1e-12, "{backward_looking} P(0,T_{k})");
            }
            for i in [0usize, 4, 9] {
                let strike = s.forwards[i];
                let mc = compute_lmm_caplet_value(&s, &paths, i, strike).unwrap();
                let formula = compute_lmm_caplet_formula(&s, i, strike).unwrap();
                assert!((mc.value - formula).abs() < 4.0 * mc.standard_error, "{backward_looking} caplet {i}: {formula} against {} ± {}", mc.value, mc.standard_error);
            }
            for (a, b) in [(2usize, 6usize), (4, 10)] {
                let (rate, _) = swap_rate_and_annuity(&s, a, b, &s.forwards);
                let mc = compute_lmm_swaption_value(&s, &paths, a, b, rate).unwrap();
                let approx = compute_formula_swaption_value(&s, a, b, rate, compute_rebonato_swaption_volatility(&s, a, b).unwrap()).unwrap();
                assert!((mc.value - approx).abs() < 4.0 * mc.standard_error + 0.02 * approx, "{backward_looking} {a}x{b}: {approx} against {} ± {}", mc.value, mc.standard_error);
            }
        }
        // the variance of a forward-looking rate is the sum over the intervals to its fixing
        let grid: Vec<f64> = (0..n).flat_map(|i| (0..=n).map(move |k| 0.1 + 0.01 * (i + k) as f64)).collect();
        let s = LmmSpec { volatility_grid: Some(grid), ..spec(0.3) };
        let i = 3;
        let mut expected = 0.0;
        for k in 0..=i {
            let (lo, hi) = (if k == 0 { 0.0 } else { s.times[k - 1] }, s.times[k]);
            let v = 0.1 + 0.01 * (i + k) as f64;
            expected += v * v * (hi - lo);
        }
        assert!((compute_lmm_rate_variance(&s, i, s.times[i]).unwrap() - expected).abs() < 1e-14);
        assert!(generate_lmm_paths(&LmmSpec { volatility_grid: Some(vec![0.2; 5]), ..spec(0.3) }, 10, 4, 1).is_err());
    }

    #[test]
    fn fewer_factors_keep_bonds_martingales_and_follow_rebonato_with_the_reduced_correlation() {
        let n = 10;
        let full = spec(0.3);
        // all factors give back the correlation; fewer give rank m with ones on the diagonal
        let same = create_reduced_correlation(&full.correlation, n, n).unwrap();
        assert!(same.iter().zip(&full.correlation).all(|(a, b)| (a - b).abs() < 1e-12));
        for m in [1usize, 2, 3, 5] {
            let reduced = create_reduced_correlation(&full.correlation, n, m).unwrap();
            let rows: Vec<Vec<f64>> = reduced.chunks(n).map(<[f64]>::to_vec).collect();
            let (values, _) = compute_symmetric_eigen(&rows).unwrap();
            assert!(values[m..].iter().all(|v| v.abs() < 1e-12), "{m}: {values:?}");
            assert!((0..n).all(|i| reduced[i * n + i] == 1.0 && (0..n).all(|j| reduced[i * n + j] == reduced[j * n + i])));
            let root = compute_reduced_pseudo_root(&full.correlation, n, m).unwrap();
            assert_eq!(root.len(), n * m);
        }
        // one factor: the rates move together
        assert!(create_reduced_correlation(&full.correlation, n, 1).unwrap().iter().all(|r| (r - 1.0).abs() < 1e-12));

        let s = LmmSpec { correlation: create_reduced_correlation(&full.correlation, n, 3).unwrap(), factors: Some(3), ..full.clone() };
        let paths = generate_lmm_paths(&s, PATHS, 4, 61).unwrap();
        for k in [1usize, 5, 10] {
            let values: Vec<f64> = paths.numeraire.iter().map(|b| 1.0 / b[k]).collect();
            let e = estimate(&values);
            assert!((e.value - s.discount(k)).abs() < 4.0 * e.standard_error + 1e-12, "P(0,T_{k})");
        }
        // the caplets do not depend on the correlation
        for i in [0usize, 4, 9] {
            let strike = s.forwards[i];
            let mc = compute_lmm_caplet_value(&s, &paths, i, strike).unwrap();
            let formula = compute_lmm_caplet_formula(&s, i, strike).unwrap();
            assert!((mc.value - formula).abs() < 4.0 * mc.standard_error, "caplet {i}: {formula} against {} ± {}", mc.value, mc.standard_error);
        }
        for (a, b) in [(2usize, 6usize), (4, 10), (1, 8)] {
            let (rate, _) = swap_rate_and_annuity(&s, a, b, &s.forwards);
            let mc = compute_lmm_swaption_value(&s, &paths, a, b, rate).unwrap();
            let approx = compute_formula_swaption_value(&s, a, b, rate, compute_rebonato_swaption_volatility(&s, a, b).unwrap()).unwrap();
            assert!((mc.value - approx).abs() < 4.0 * mc.standard_error + 0.02 * approx, "{a}x{b}: {approx} against {} ± {}", mc.value, mc.standard_error);
            // fewer factors correlate the rates more, and raise the swaption's volatility
            assert!(compute_rebonato_swaption_volatility(&s, a, b).unwrap() > compute_rebonato_swaption_volatility(&full, a, b).unwrap());
        }
        // the simulated correlation of the log changes to T_0 is the reduced one
        let changes: Vec<Vec<f64>> = paths.state.iter().map(|st| (0..n).map(|i| (st[0][i] / s.forwards[i]).ln()).collect()).collect();
        let column = |i: usize| -> Vec<f64> { changes.iter().map(|c| c[i]).collect() };
        for (i, j) in [(0usize, 5usize), (0, 9), (3, 4)] {
            let (x, y) = (column(i), column(j));
            let (mx, my) = (x.iter().sum::<f64>() / x.len() as f64, y.iter().sum::<f64>() / y.len() as f64);
            let cov: f64 = x.iter().zip(&y).map(|(a, b)| (a - mx) * (b - my)).sum();
            let vx: f64 = x.iter().map(|a| (a - mx) * (a - mx)).sum();
            let vy: f64 = y.iter().map(|b| (b - my) * (b - my)).sum();
            let rho = cov / (vx * vy).sqrt();
            assert!((rho - s.correlation[i * n + j]).abs() < 0.1, "{i} {j}: {rho} against {}", s.correlation[i * n + j]);
        }
        // the calibration fits beta with the reduced correlation
        let quotes: Vec<SwaptionQuote> = [(2usize, 6usize), (4, 10), (1, 8), (6, 10)]
            .iter()
            .map(|&(a, b)| SwaptionQuote { start: a, end: b, volatility: compute_rebonato_swaption_volatility(&s, a, b).unwrap() })
            .collect();
        let fit = calibrate_lmm_correlation(&LmmSpec { factors: Some(3), ..spec(0.9) }, &quotes, 5.0).unwrap();
        assert!((fit.beta - 0.3).abs() < 1e-6, "{}", fit.beta);
        // the rank must fit the factors, and a reduced correlation needs its factors
        assert!(generate_lmm_paths(&LmmSpec { factors: Some(3), ..full.clone() }, 10, 4, 1).is_err());
        assert!(generate_lmm_paths(&LmmSpec { factors: Some(0), ..s.clone() }, 10, 4, 1).is_err());
        assert!(generate_lmm_paths(&LmmSpec { factors: Some(11), ..s.clone() }, 10, 4, 1).is_err());
        assert!(generate_lmm_paths(&LmmSpec { factors: None, ..s.clone() }, 10, 4, 1).is_err());
        assert!(create_reduced_correlation(&full.correlation, n, 0).is_err());
    }

    /// Volatilities high near the fixing, low far from it, on each interval at its middle.
    fn humped_grid(s: &LmmSpec, scale: f64) -> Vec<f64> {
        let n = s.count();
        (0..n)
            .flat_map(|i| {
                let times = s.times.clone();
                (0..=n).map(move |k| {
                    let (lo, hi) = (if k == 0 { 0.0 } else { times[k - 1] }, times[k]);
                    scale * (0.12 + 0.2 * (-(times[i] - (lo + hi) / 2.0).max(0.0)).exp() + 0.003 * i as f64)
                })
            })
            .collect()
    }

    fn all_quotes(s: &LmmSpec, first: usize) -> Vec<SwaptionQuote> {
        let n = s.count();
        (first..n)
            .flat_map(|a| (a + 1..=n).map(move |b| (a, b)))
            .map(|(a, b)| SwaptionQuote { start: a, end: b, volatility: compute_rebonato_swaption_volatility(s, a, b).unwrap() })
            .collect()
    }

    #[test]
    fn the_cascade_fits_each_swaption_and_gives_back_the_grid() {
        let n = 10;
        let reduced = LmmSpec { correlation: create_reduced_correlation(&spec(0.3).correlation, n, 3).unwrap(), factors: Some(3), ..spec(0.3) };
        let normal = LmmSpec { model: ForwardModel::Bachelier, ..spec(0.3) };
        let cases = [(spec(0.3), 1.0), (LmmSpec { backward_looking: true, ..spec(0.3) }, 1.0), (normal, 0.02), (reduced, 1.0)];
        for (base, scale) in cases {
            let truth = LmmSpec { volatility_grid: Some(humped_grid(&base, scale)), ..base.clone() };
            let quotes = all_quotes(&truth, 0);
            assert_eq!(quotes.len(), 55);
            // from constant volatilities that are not the answer
            let fit = calibrate_lmm_volatilities(&base, &quotes).unwrap();
            assert!(fit.max_error < 1e-12 * scale, "{:e}", fit.max_error);
            let (got, want) = (fit.spec.volatility_grid.as_ref().unwrap(), truth.volatility_grid.as_ref().unwrap());
            for i in 0..n {
                for k in 0..=i {
                    let (g, w) = (got[i * (n + 1) + k], want[i * (n + 1) + k]);
                    assert!((g - w).abs() < 1e-9 * scale, "{:?} {i} {k}: {g} against {w}", base.model);
                }
            }
        }
    }

    #[test]
    fn the_cascaded_model_reprices_its_swaptions_and_caplets() {
        let base = spec(0.3);
        let truth = LmmSpec { volatility_grid: Some(humped_grid(&base, 1.0)), ..base.clone() };
        let quotes = all_quotes(&truth, 0);
        let fit = calibrate_lmm_volatilities(&base, &quotes).unwrap();
        let s = &fit.spec;
        // the caplets a x (a + 1) are exact: the formula at the quoted volatility
        for q in quotes.iter().filter(|q| q.end == q.start + 1) {
            let i = q.start;
            let black = compute_caplet_value(
                ForwardModel::Black,
                &CapletTerms { forward: s.forwards[i], strike: s.forwards[i], volatility: q.volatility, expiry: s.times[i], accrual: s.accrual(i), discount_factor: s.discount(i + 1), notional: 1.0 },
            )
            .unwrap();
            assert!((compute_lmm_caplet_formula(s, i, s.forwards[i]).unwrap() - black).abs() < 1e-12, "caplet {i}");
        }
        // the simulated swaptions are near the quotes' Black values
        let paths = generate_lmm_paths(s, PATHS, 4, 67).unwrap();
        for (a, b) in [(1usize, 4usize), (3, 10), (6, 8)] {
            let q = quotes.iter().find(|q| q.start == a && q.end == b).unwrap();
            let (rate, _) = swap_rate_and_annuity(s, a, b, &s.forwards);
            let mc = compute_lmm_swaption_value(s, &paths, a, b, rate).unwrap();
            let quoted = compute_formula_swaption_value(s, a, b, rate, q.volatility).unwrap();
            assert!((mc.value - quoted).abs() < 4.0 * mc.standard_error + 0.02 * quoted, "{a}x{b}: {quoted} against {} ± {}", mc.value, mc.standard_error);
        }
        for i in [0usize, 5, 9] {
            let mc = compute_lmm_caplet_value(s, &paths, i, s.forwards[i]).unwrap();
            let formula = compute_lmm_caplet_formula(s, i, s.forwards[i]).unwrap();
            assert!((mc.value - formula).abs() < 4.0 * mc.standard_error, "caplet {i}");
        }
    }

    #[test]
    fn the_cascade_from_today_and_its_errors() {
        // T_0 = 0: the swaptions exercised from T_1 on
        let times: Vec<f64> = (0..=8).map(|k| 0.5 * k as f64).collect();
        let base = LmmSpec {
            correlation: create_exponential_correlation(&times[..8], 0.2).unwrap(),
            forwards: vec![0.02; 8],
            volatilities: vec![0.2; 8],
            times,
            discount_start: 1.0,
            backward_looking: true,
            model: ForwardModel::Black,
            volatility_grid: None,
            factors: None,
            measure: Measure::Spot,
            discretization: Discretization::Euler,
        };
        let truth = LmmSpec { volatility_grid: Some(humped_grid(&base, 1.0)), ..base.clone() };
        let quotes = all_quotes(&truth, 1);
        assert_eq!(quotes.len(), 28);
        let fit = calibrate_lmm_volatilities(&base, &quotes).unwrap();
        assert!(fit.max_error < 1e-12);
        let s = spec(0.3);
        let mut quotes = all_quotes(&LmmSpec { volatility_grid: Some(humped_grid(&s, 1.0)), ..s.clone() }, 0);
        // missing, twice, before today, and too low for what the earlier swaptions give
        assert!(calibrate_lmm_volatilities(&s, &quotes[1..]).is_err());
        let twice = [quotes.clone(), vec![quotes[3]]].concat();
        assert!(calibrate_lmm_volatilities(&s, &twice).is_err());
        let today = [all_quotes(&truth, 1), vec![SwaptionQuote { start: 0, end: 2, volatility: 0.2 }]].concat();
        assert!(calibrate_lmm_volatilities(&base, &today).is_err());
        let low = quotes.iter().position(|q| q.start == 2 && q.end == 6).unwrap();
        quotes[low].volatility = 0.02;
        let e = calibrate_lmm_volatilities(&s, &quotes).unwrap_err();
        assert!(format!("{e:?}").contains("2 x 6"), "{e:?}");
    }

    #[test]
    fn the_terminal_measure_gives_the_spot_measure_values() {
        use crate::fmm::compute_backward_forward_rate;
        let n = 10;
        let backward = LmmSpec { backward_looking: true, ..spec(0.3) };
        let normal = LmmSpec { volatilities: vec![0.006; 10], model: ForwardModel::Bachelier, ..spec(0.2) };
        let normal_backward = LmmSpec { backward_looking: true, ..normal.clone() };
        let reduced = LmmSpec {
            correlation: create_reduced_correlation(&spec(0.3).correlation, n, 3).unwrap(),
            factors: Some(3),
            volatility_grid: Some(humped_grid(&spec(0.3), 1.0)),
            ..spec(0.3)
        };
        // a period under way, as in `a_period_under_way_moves_only_its_unfixed_part`
        let times: Vec<f64> = (0..=8).map(|k| -1.0 / 12.0 + 0.25 * k as f64).collect();
        let mut forwards: Vec<f64> = (0..8).map(|k| 0.02 + 0.001 * k as f64).collect();
        forwards[0] = compute_backward_forward_rate(1.0017, 1.0, 0.9965, 0.25).unwrap();
        let under_way = LmmSpec {
            correlation: create_exponential_correlation(&times[..8], 0.3).unwrap(),
            times,
            forwards,
            volatilities: vec![0.3; 8],
            discount_start: 1.0017,
            backward_looking: true,
            model: ForwardModel::Black,
            volatility_grid: None,
            factors: None,
            measure: Measure::Spot,
            discretization: Discretization::Euler,
        };
        for (case, spot) in [spec(0.3), backward, normal, normal_backward, reduced, under_way].into_iter().enumerate() {
            let terminal = LmmSpec { measure: Measure::Terminal, ..spot.clone() };
            let m = spot.count();
            let paths = generate_lmm_paths(&terminal, PATHS, 8, 71).unwrap();
            let spot_paths = generate_lmm_paths(&spot, PATHS, 8, 73).unwrap();
            // the numeraire is 1 / P(0, T_n) at the end, and 1 / N(T_k) prices the bonds
            assert!(paths.numeraire.iter().all(|b| (b[m] * spot.discount(m) - 1.0).abs() < 1e-14));
            for k in [1usize, m / 2, m - 1] {
                let values: Vec<f64> = paths.numeraire.iter().map(|b| 1.0 / b[k]).collect();
                let e = estimate(&values);
                assert!((e.value - spot.discount(k)).abs() < 4.0 * e.standard_error + 1e-12, "case {case} P(0,T_{k}): {} against {} ± {}", spot.discount(k), e.value, e.standard_error);
            }
            for i in [m / 2, m - 1] {
                let strike = spot.forwards[i];
                let mc = compute_lmm_caplet_value(&terminal, &paths, i, strike).unwrap();
                let formula = compute_lmm_caplet_formula(&spot, i, strike).unwrap();
                assert!((mc.value - formula).abs() < 4.0 * mc.standard_error, "case {case} caplet {i}: {formula} against {} ± {}", mc.value, mc.standard_error);
            }
            for (a, b) in [(2usize, 6usize), (3, m)] {
                let (rate, _) = swap_rate_and_annuity(&spot, a, b, &spot.forwards);
                let t = compute_lmm_swaption_value(&terminal, &paths, a, b, rate).unwrap();
                let s = compute_lmm_swaption_value(&spot, &spot_paths, a, b, rate).unwrap();
                let both = (t.standard_error.powi(2) + s.standard_error.powi(2)).sqrt();
                assert!((t.value - s.value).abs() < 4.0 * both, "case {case} {a}x{b}: terminal {} spot {} ± {both}", t.value, s.value);
                let approx = compute_formula_swaption_value(&spot, a, b, rate, compute_rebonato_swaption_volatility(&spot, a, b).unwrap()).unwrap();
                assert!((t.value - approx).abs() < 4.0 * t.standard_error + 0.02 * approx, "case {case} {a}x{b}: {approx} against {}", t.value);
            }
            // exercised between tenor dates: the payoff deferred to the next date, as under the spot measure
            let expiry = (spot.times[2] + spot.times[3]) / 2.0;
            let (rate, _) = swap_rate_and_annuity(&spot, 2, 6, &spot.forwards);
            let observed = generate_lmm_paths_observed(&terminal, PATHS, 8, 79, Some(expiry)).unwrap();
            let t = compute_lmm_swaption_value_at(&terminal, &observed, 2, 6, rate).unwrap();
            let spot_observed = generate_lmm_paths_observed(&spot, PATHS, 8, 83, Some(expiry)).unwrap();
            let s = compute_lmm_swaption_value_at(&spot, &spot_observed, 2, 6, rate).unwrap();
            let both = (t.standard_error.powi(2) + s.standard_error.powi(2)).sqrt();
            assert!((t.value - s.value).abs() < 4.0 * both, "case {case} at {expiry}: terminal {} spot {} ± {both}", t.value, s.value);
        }
    }

    #[test]
    fn a_predictor_corrector_step_follows_quantlib() {
        // three rates, one step to T_0: QuantLib's order (predict with D1, D2 at the prediction, add
        // (D2 - D1) / 2) by hand against the paths
        let times = vec![0.5, 1.0, 1.5, 2.0];
        for model in [ForwardModel::Black, ForwardModel::Bachelier] {
            for measure in [Measure::Spot, Measure::Terminal] {
                let s = LmmSpec {
                    correlation: create_exponential_correlation(&times[..3], 0.4).unwrap(),
                    times: times.clone(),
                    forwards: vec![0.03, 0.035, 0.04],
                    volatilities: if model == ForwardModel::Black { vec![0.4, 0.35, 0.3] } else { vec![0.01; 3] },
                    discount_start: 0.99,
                    backward_looking: false,
                    model,
                    volatility_grid: None,
                    factors: None,
                    measure,
                    discretization: Discretization::PredictorCorrector,
                };
                let paths = generate_lmm_paths(&s, 1, 1, 41).unwrap();
                let lower = s.check().unwrap();
                let z: Vec<f64> = (0..3).map(|i| generate_standard_normal(41, 0, i)).collect();
                let w: Vec<f64> = lower.iter().map(|row| row.iter().zip(&z).map(|(a, b)| a * b).sum()).collect();
                let dt: f64 = 0.5;
                let d1 = compute_lmm_drift(&s, 0, &s.forwards).unwrap();
                let shock = |k: usize| s.volatilities[k] * dt.sqrt() * w[k];
                let expected: Vec<f64> = match model {
                    ForwardModel::Black => {
                        let log0: Vec<f64> = s.forwards.iter().map(|f| f.ln()).collect();
                        let fixed = |k: usize| -0.5 * s.volatilities[k] * s.volatilities[k] * dt;
                        let predicted: Vec<f64> = (0..3).map(|k| (log0[k] + d1[k] * dt + fixed(k) + shock(k)).exp()).collect();
                        let d2 = compute_lmm_drift(&s, 0, &predicted).unwrap();
                        (0..3).map(|k| (predicted[k].ln() + (d2[k] - d1[k]) * dt / 2.0).exp()).collect()
                    }
                    ForwardModel::Bachelier => {
                        let predicted: Vec<f64> = (0..3).map(|k| s.forwards[k] + d1[k] * dt + shock(k)).collect();
                        let d2 = compute_lmm_drift(&s, 0, &predicted).unwrap();
                        (0..3).map(|k| predicted[k] + (d2[k] - d1[k]) * dt / 2.0).collect()
                    }
                };
                for k in 0..3 {
                    let got = paths.state[0][0][k];
                    assert!((got - expected[k]).abs() < 1e-15, "{model:?} {measure:?} rate {k}: {got} against {}", expected[k]);
                }
                // the corrector moved the rates away from the Euler step
                let euler = generate_lmm_paths(&LmmSpec { discretization: Discretization::Euler, ..s.clone() }, 1, 1, 41).unwrap();
                assert!((0..3).map(|k| (euler.state[0][0][k] - paths.state[0][0][k]).abs()).fold(0.0, f64::max) > 1e-8);
            }
        }
    }

    #[test]
    fn the_predictor_corrector_cuts_the_discretization_error_of_long_steps() {
        // annual periods, a volatility of 50% and one step a period: the Euler drift is far off
        let times: Vec<f64> = (0..=10).map(|k| 1.0 + k as f64).collect();
        let euler = LmmSpec {
            correlation: create_exponential_correlation(&times[..10], 0.1).unwrap(),
            times,
            forwards: vec![0.05; 10],
            volatilities: vec![0.5; 10],
            discount_start: 0.95,
            backward_looking: false,
            model: ForwardModel::Black,
            volatility_grid: None,
            factors: None,
            measure: Measure::Spot,
            discretization: Discretization::Euler,
        };
        let corrected = LmmSpec { discretization: Discretization::PredictorCorrector, ..euler.clone() };
        let many = 20_000;
        // the same shocks for both, so that the comparison sees the bias, not the noise
        let (pe, pc) = (generate_lmm_paths(&euler, many, 1, 5).unwrap(), generate_lmm_paths(&corrected, many, 1, 5).unwrap());
        let reference = generate_lmm_paths(&euler, many, 32, 99).unwrap();
        let bond = |p: &LmmPaths| estimate(&p.numeraire.iter().map(|b| 1.0 / b[10]).collect::<Vec<_>>()).value - euler.discount(10);
        let (rate, _) = swap_rate_and_annuity(&euler, 3, 10, &euler.forwards);
        let target = compute_lmm_swaption_value(&euler, &reference, 3, 10, rate).unwrap().value;
        let swaption = |s: &LmmSpec, p: &LmmPaths| compute_lmm_swaption_value(s, p, 3, 10, rate).unwrap().value - target;
        let formula = compute_lmm_caplet_formula(&euler, 9, 0.05).unwrap();
        let caplet = |s: &LmmSpec, p: &LmmPaths| compute_lmm_caplet_value(s, p, 9, 0.05).unwrap().value - formula;
        assert!(bond(&pc).abs() < 0.6 * bond(&pe).abs(), "bond: {} against {}", bond(&pc), bond(&pe));
        assert!(swaption(&corrected, &pc).abs() < 0.5 * swaption(&euler, &pe).abs(), "swaption: {} against {}", swaption(&corrected, &pc), swaption(&euler, &pe));
        assert!(caplet(&corrected, &pc).abs() < 0.5 * caplet(&euler, &pe).abs(), "caplet: {} against {}", caplet(&corrected, &pc), caplet(&euler, &pe));
        // with short steps both agree with the no-arbitrage values
        let fine = generate_lmm_paths(&corrected, PATHS, 8, 7).unwrap();
        for k in [1usize, 5, 10] {
            let e = estimate(&fine.numeraire.iter().map(|b| 1.0 / b[k]).collect::<Vec<_>>());
            assert!((e.value - euler.discount(k)).abs() < 4.0 * e.standard_error, "P(0,T_{k})");
        }
    }

    #[test]
    fn invalid_inputs_are_errors() {
        let mut s = spec(0.2);
        assert!(generate_lmm_paths(&s, 0, 4, 1).is_err());
        assert!(compute_rebonato_swaption_volatility(&s, 5, 5).is_err());
        assert!(create_exponential_correlation(&s.times, -1.0).is_err());
        s.forwards[3] = -0.01;
        assert!(generate_lmm_paths(&s, 10, 4, 1).is_err());
    }
}

