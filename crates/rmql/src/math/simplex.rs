//! Linear programs in standard form, `min c'x` subject to `A x = b` and `x >= 0`, by the two-phase
//! simplex method on a dense tableau with Bland's rule (the lowest index enters and leaves), which
//! cannot cycle. For the small programs of this crate (a few hundred rows and columns); the same input
//! always gives the same result.

use crate::math::linear::solve;
use crate::Error;

/// The solution of a linear program.
#[derive(Debug, Clone, PartialEq)]
pub struct LinearSolution {
    pub x: Vec<f64>,
    pub objective: f64,
    pub pivots: usize,
}

fn invalid<T>(reason: &str) -> Result<T, Error> {
    Err(Error::InvalidInput { reason: reason.into() })
}

/// Pivot elements and ties of the ratio test.
const EPS: f64 = 1e-11;
/// A reduced cost counts as negative below `-COST_EPS` times the scale of the cost row; a column whose
/// reduced cost is negative only by rounding (below `NOISE` times the scale) and has no pivot element is
/// passed over rather than taken as a direction of unboundedness.
const COST_EPS: f64 = 1e-12;
const NOISE: f64 = 1e-8;

/// Pivots the tableau on (row, col).
fn pivot(t: &mut [Vec<f64>], row: usize, col: usize) {
    let p = t[row][col];
    for v in t[row].iter_mut() {
        *v /= p;
    }
    let pivot_row = t[row].clone();
    for (i, r) in t.iter_mut().enumerate() {
        if i == row {
            continue;
        }
        let f = r[col];
        if f != 0.0 {
            for (v, pv) in r.iter_mut().zip(&pivot_row) {
                *v -= f * pv;
            }
        }
    }
}

/// Runs the simplex method on the tableau whose last row is the reduced cost and whose last column is
/// the right-hand side, over the columns `0..columns`. Returns the number of pivots.
fn run(t: &mut [Vec<f64>], basis: &mut [usize], columns: usize, pivots: &mut usize, limit: usize) -> Result<(), Error> {
    let rows = basis.len();
    let rhs = t[0].len() - 1;
    let scale = 1.0 + t[rows][..columns].iter().fold(0.0_f64, |s, v| s.max(v.abs()));
    loop {
        let mut step = None;
        for col in (0..columns).filter(|&c| t[rows][c] < -COST_EPS * scale) {
            let mut best: Option<(usize, f64)> = None;
            for (i, row) in t.iter().take(rows).enumerate() {
                if row[col] > EPS {
                    let ratio = row[rhs] / row[col];
                    match best {
                        None => best = Some((i, ratio)),
                        Some((bi, br)) => {
                            if ratio < br - EPS || (ratio <= br + EPS && basis[i] < basis[bi]) {
                                best = Some((i, ratio));
                            }
                        }
                    }
                }
            }
            match best {
                Some((row, _)) => {
                    step = Some((row, col));
                    break;
                }
                None if t[rows][col] > -NOISE * scale => continue,
                None => return invalid("the linear program is unbounded"),
            }
        }
        let Some((row, col)) = step else { return Ok(()) };
        pivot(t, row, col);
        basis[row] = col;
        *pivots += 1;
        if *pivots > limit {
            return Err(Error::NoConvergence { reason: "the linear program did not finish within its pivot limit".into() });
        }
    }
}

/// Solves `min c'x`, `A x = b`, `x >= 0`. `a` has one row per constraint.
pub fn solve_linear_program(c: &[f64], a: &[Vec<f64>], b: &[f64]) -> Result<LinearSolution, Error> {
    let n = c.len();
    let m = a.len();
    if n == 0 || b.len() != m || a.iter().any(|row| row.len() != n) {
        return invalid("a linear program needs one row of the size of the cost per bound");
    }
    if c.iter().chain(b).chain(a.iter().flatten()).any(|v| !v.is_finite()) {
        return invalid("the linear program must be finite");
    }
    // Tableau: n columns, m artificial columns, right-hand side; rows with b < 0 are negated.
    let width = n + m + 1;
    let mut t: Vec<Vec<f64>> = (0..m)
        .map(|i| {
            let s = if b[i] < 0.0 { -1.0 } else { 1.0 };
            let mut row = vec![0.0; width];
            for k in 0..n {
                row[k] = s * a[i][k];
            }
            row[n + i] = 1.0;
            row[width - 1] = s * b[i];
            row
        })
        .collect();
    // phase 1: minimize the sum of the artificials
    let mut cost = vec![0.0; width];
    for row in &t {
        for k in 0..n {
            cost[k] -= row[k];
        }
        cost[width - 1] -= row[width - 1];
    }
    t.push(cost);
    let mut basis: Vec<usize> = (n..n + m).collect();
    let mut pivots = 0usize;
    let limit = 200 * (n + m) + 1000;
    run(&mut t, &mut basis, n, &mut pivots, limit)?;
    if -t[m][width - 1] > 1e-9 * (1.0 + b.iter().fold(0.0_f64, |s, v| s.max(v.abs()))) {
        return invalid("the constraints of the linear program cannot all be met (no feasible point)");
    }
    // drive artificials out of the basis where possible (redundant rows keep theirs at zero)
    for i in 0..m {
        if basis[i] >= n {
            if let Some(col) = (0..n).find(|&k| t[i][k].abs() > EPS) {
                pivot(&mut t, i, col);
                basis[i] = col;
            }
        }
    }
    // phase 2: the real cost, reduced on the basis; artificial columns may no longer enter
    let mut cost = vec![0.0; width];
    cost[..n].copy_from_slice(c);
    for i in 0..m {
        let bc = if basis[i] < n { c[basis[i]] } else { 0.0 };
        if bc != 0.0 {
            for k in 0..width {
                cost[k] -= bc * t[i][k];
            }
        }
    }
    t[m] = cost;
    run(&mut t, &mut basis, n, &mut pivots, limit)?;
    // The tableau gathers rounding over many pivots: solve B x_B = b on the original rows for the final
    // basis (an artificial column is the unit vector of its row, with the row's sign).
    let columns: Vec<Vec<f64>> = (0..m)
        .map(|i| {
            (0..m)
                .map(|basic| match basis[basic] {
                    k if k < n => a[i][k],
                    k if k - n == i => if b[i] < 0.0 { -1.0 } else { 1.0 },
                    _ => 0.0,
                })
                .collect()
        })
        .collect();
    let refined = solve(&columns, &b.iter().map(|v| vec![*v]).collect::<Vec<_>>());
    let mut x = vec![0.0; n];
    for i in 0..m {
        if basis[i] < n {
            x[basis[i]] = match &refined {
                Ok(sol) => sol[i][0].max(0.0),
                Err(_) => t[i][width - 1],
            };
        }
    }
    let objective = c.iter().zip(&x).map(|(ci, xi)| ci * xi).sum();
    Ok(LinearSolution { x, objective, pivots })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solves_a_small_program() {
        // max 3x + 2y, x + y <= 4, x + 3y <= 6, x <= 3  ->  x = 3, y = 1, value 11
        let c = vec![-3.0, -2.0, 0.0, 0.0, 0.0];
        let a = vec![vec![1.0, 1.0, 1.0, 0.0, 0.0], vec![1.0, 3.0, 0.0, 1.0, 0.0], vec![1.0, 0.0, 0.0, 0.0, 1.0]];
        let s = solve_linear_program(&c, &a, &[4.0, 6.0, 3.0]).unwrap();
        assert!((s.objective + 11.0).abs() < 1e-12 && (s.x[0] - 3.0).abs() < 1e-12 && (s.x[1] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn reports_infeasible_and_unbounded_programs() {
        assert!(solve_linear_program(&[1.0], &[vec![1.0]], &[-1.0]).is_err());
        assert!(solve_linear_program(&[-1.0, 0.0], &[vec![1.0, -1.0]], &[0.0]).is_err());
    }
}
