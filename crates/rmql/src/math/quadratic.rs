//! Strictly convex quadratic programs by the dual active-set method of Goldfarb and Idnani (1983),
//! "A numerically stable dual method for solving strictly convex quadratic programs", Mathematical
//! Programming 27, 1-33. Written from the paper; the R package quadprog uses the same method.
//!
//! The problem is `min 1/2 x'Gx + a'x` subject to `n_i'x = b_i` for the first `equalities` rows and
//! `n_i'x >= b_i` for the others, with `G` symmetric positive definite. The method starts from the
//! unconstrained minimum and adds violated constraints one at a time, keeping the multipliers of the
//! active inequalities non-negative; it needs no feasible starting point. `J = L^-T Q` and the upper
//! triangular `R` are kept up to date by Givens rotations, where `G = L L'` and `Q R` is the QR
//! factorization of `L^-1 N` for the active normals `N`.
//!
//! Dense, for problems of up to a few hundred variables. The same input always gives the same result:
//! the most violated constraint is added, ties going to the first.

use crate::math::linear::compute_cholesky;
use crate::Error;

/// The constraints `n_i'x = b_i` (the first `equalities` rows) and `n_i'x >= b_i`.
#[derive(Debug, Clone, PartialEq)]
pub struct LinearConstraints {
    pub normals: Vec<Vec<f64>>,
    pub bounds: Vec<f64>,
    pub equalities: usize,
}

/// The solution, the multipliers of the constraints (zero where inactive, in `normals`' order) and
/// the work done.
#[derive(Debug, Clone, PartialEq)]
pub struct QuadraticSolution {
    pub x: Vec<f64>,
    pub objective: f64,
    pub multipliers: Vec<f64>,
    pub active: Vec<usize>,
    pub iterations: usize,
    /// `max |G x + a - sum_i u_i n_i|`: zero at an exact optimum.
    pub stationarity_residual: f64,
}

fn invalid<T>(reason: &str) -> Result<T, Error> {
    Err(Error::InvalidInput { reason: reason.into() })
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Rotates columns `i` and `j` of `m` by `(c, s)`: `m_i <- c m_i + s m_j`, `m_j <- -s m_i + c m_j`.
fn rotate_columns(m: &mut [Vec<f64>], i: usize, j: usize, c: f64, s: f64) {
    for row in m.iter_mut() {
        let (p, q) = (row[i], row[j]);
        row[i] = c * p + s * q;
        row[j] = -s * p + c * q;
    }
}

/// Solves the program. `g` is `n x n`, `a` has length `n`; every normal has length `n`.
#[allow(clippy::needless_range_loop)]
pub fn solve_quadratic_program(g: &[Vec<f64>], a: &[f64], constraints: &LinearConstraints) -> Result<QuadraticSolution, Error> {
    let n = a.len();
    let m = constraints.normals.len();
    if n == 0 || g.len() != n || g.iter().any(|row| row.len() != n) {
        return invalid("a quadratic program needs a square matrix of the size of the linear term");
    }
    if constraints.bounds.len() != m || constraints.equalities > m || constraints.normals.iter().any(|row| row.len() != n) {
        return invalid("each constraint needs a normal of the size of the problem and one bound");
    }
    let finite = g.iter().flatten().chain(a).chain(constraints.normals.iter().flatten()).chain(&constraints.bounds);
    if finite.into_iter().any(|v| !v.is_finite()) {
        return invalid("the quadratic program must be finite");
    }
    let l = compute_cholesky(g).map_err(|_| Error::InvalidInput {
        reason: "the matrix of the quadratic program is not positive definite (for a covariance matrix: fewer observations than assets, or assets that move together exactly)".into(),
    })?;

    // J = L^-T: solve L' J = I column by column (back substitution).
    let mut j = vec![vec![0.0; n]; n];
    for col in 0..n {
        for row in (0..n).rev() {
            let mut v = if row == col { 1.0 } else { 0.0 };
            for k in row + 1..n {
                v -= l[k][row] * j[k][col];
            }
            j[row][col] = v / l[row][row];
        }
    }
    // The unconstrained minimum x = -G^-1 a = -J J' a.
    let jta: Vec<f64> = (0..n).map(|c| (0..n).map(|r| j[r][c] * a[r]).sum()).collect();
    let mut x: Vec<f64> = (0..n).map(|r| -dot(&j[r], &jta)).collect();

    let mut r = vec![vec![0.0; n]; n]; // R[row][col], the first q columns used
    let mut active: Vec<usize> = Vec::new();
    let mut sign: Vec<f64> = Vec::new(); // +1, or -1 for an equality entered from the other side
    let mut u: Vec<f64> = Vec::new();
    let scale = 1.0 + constraints.bounds.iter().fold(0.0_f64, |s, b| s.max(b.abs()));
    let tolerance = 1e-12;
    let mut iterations = 0usize;
    let limit = 50 * (n + m) + 100;

    let violation = |x: &[f64], i: usize| dot(&constraints.normals[i], x) - constraints.bounds[i];

    loop {
        // Choose the constraint to add: the next equality, or the most violated inequality.
        let mut chosen: Option<(usize, f64)> = None;
        let equalities_done = active.iter().filter(|&&k| k < constraints.equalities).count();
        if equalities_done < constraints.equalities {
            let i = (0..constraints.equalities).find(|i| !active.contains(i)).unwrap();
            let s = violation(&x, i);
            chosen = Some((i, if s > 0.0 { -1.0 } else { 1.0 }));
        } else {
            let mut worst = 0.0;
            for i in constraints.equalities..m {
                if active.contains(&i) {
                    continue;
                }
                let s = violation(&x, i);
                let size = 1.0 + constraints.normals[i].iter().fold(0.0_f64, |t, v| t.max(v.abs())) * x.iter().fold(0.0_f64, |t, v| t.max(v.abs()));
                if s < -tolerance * size.max(scale) && s < worst {
                    worst = s;
                    chosen = Some((i, 1.0));
                }
            }
        }
        let Some((p, sp)) = chosen else { break };
        let np: Vec<f64> = constraints.normals[p].iter().map(|v| sp * v).collect();
        let bp = sp * constraints.bounds[p];
        let is_equality = p < constraints.equalities;
        let mut u_new = 0.0;

        // Steps towards satisfying constraint p, dropping blocking constraints on the way.
        loop {
            iterations += 1;
            if iterations > limit {
                return Err(Error::NoConvergence { reason: "the quadratic program did not finish within its iteration limit".into() });
            }
            let q = active.len();
            let d: Vec<f64> = (0..n).map(|c| (0..n).map(|row| j[row][c] * np[row]).sum()).collect();
            let z: Vec<f64> = (0..n).map(|row| (q..n).map(|c| j[row][c] * d[c]).sum()).collect();
            let mut rr = vec![0.0; q];
            for k in (0..q).rev() {
                let mut v = d[k];
                for c in k + 1..q {
                    v -= r[k][c] * rr[c];
                }
                rr[k] = v / r[k][k];
            }
            // partial step: the largest the inequality multipliers allow
            let mut t1 = f64::INFINITY;
            let mut blocking = None;
            for k in 0..q {
                if active[k] >= constraints.equalities && rr[k] > 0.0 {
                    let t = u[k] / rr[k];
                    if t < t1 {
                        t1 = t;
                        blocking = Some(k);
                    }
                }
            }
            // full step: constraint p becomes satisfied
            let zn = dot(&z, &np);
            let s = dot(&np, &x) - bp;
            let t2 = if zn.abs() > 1e-14 * (1.0 + dot(&np, &np).sqrt()) { -s / zn } else { f64::INFINITY };
            let t = if is_equality { t2 } else { t1.min(t2) };
            if !t.is_finite() {
                return invalid(if is_equality {
                    "the equality constraints of the quadratic program are linearly dependent"
                } else {
                    "the constraints of the quadratic program cannot all be met (no feasible point)"
                });
            }
            if !t2.is_finite() {
                // dual step only, then drop the blocking constraint
                for k in 0..q {
                    u[k] -= t * rr[k];
                }
                u_new += t;
                drop_constraint(&mut r, &mut j, &mut active, &mut sign, &mut u, blocking.unwrap());
                continue;
            }
            for (xi, zi) in x.iter_mut().zip(&z) {
                *xi += t * zi;
            }
            for k in 0..q {
                u[k] -= t * rr[k];
            }
            u_new += t;
            if t == t2 {
                // add p: rotate d so that only its first q + 1 entries are non-zero
                let mut d = d;
                for c in (q + 1..n).rev() {
                    let (a1, b1) = (d[c - 1], d[c]);
                    if b1 == 0.0 {
                        continue;
                    }
                    let h = a1.hypot(b1);
                    let (cs, sn) = (a1 / h, b1 / h);
                    d[c - 1] = h;
                    d[c] = 0.0;
                    rotate_columns(&mut j, c - 1, c, cs, sn);
                }
                if d[q].abs() <= 1e-14 * (1.0 + dot(&np, &np).sqrt()) {
                    return invalid("an active constraint of the quadratic program depends linearly on the others");
                }
                for k in 0..=q {
                    r[k][q] = d[k];
                }
                active.push(p);
                sign.push(sp);
                u.push(u_new);
                break;
            }
            drop_constraint(&mut r, &mut j, &mut active, &mut sign, &mut u, blocking.unwrap());
        }
    }

    let objective = 0.5 * (0..n).map(|i| x[i] * dot(&g[i], &x)).sum::<f64>() + dot(a, &x);
    let mut multipliers = vec![0.0; m];
    for (k, &i) in active.iter().enumerate() {
        multipliers[i] = sign[k] * u[k];
    }
    let stationarity_residual = (0..n)
        .map(|i| {
            let mut v = dot(&g[i], &x) + a[i];
            for (k, &c) in active.iter().enumerate() {
                v -= sign[k] * u[k] * constraints.normals[c][i];
            }
            v.abs()
        })
        .fold(0.0, f64::max);
    let mut active_sorted = active.clone();
    active_sorted.sort_unstable();
    Ok(QuadraticSolution { x, objective, multipliers, active: active_sorted, iterations, stationarity_residual })
}

/// Removes the active constraint at position `l` and restores `R` to upper triangular form.
#[allow(clippy::needless_range_loop)]
fn drop_constraint(r: &mut [Vec<f64>], j: &mut [Vec<f64>], active: &mut Vec<usize>, sign: &mut Vec<f64>, u: &mut Vec<f64>, l: usize) {
    let q = active.len();
    for row in r.iter_mut() {
        for c in l..q - 1 {
            row[c] = row[c + 1];
        }
        row[q - 1] = 0.0;
    }
    for k in l..q - 1 {
        let (a1, b1) = (r[k][k], r[k + 1][k]);
        if b1 == 0.0 {
            continue;
        }
        let h = a1.hypot(b1);
        let (cs, sn) = (a1 / h, b1 / h);
        for c in k..q - 1 {
            let (p, s) = (r[k][c], r[k + 1][c]);
            r[k][c] = cs * p + sn * s;
            r[k + 1][c] = -sn * p + cs * s;
        }
        rotate_columns(j, k, k + 1, cs, sn);
    }
    active.remove(l);
    sign.remove(l);
    u.remove(l);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solves_a_small_program_with_an_active_bound() {
        // min (x - 1)^2 + (y - 2)^2 s.t. x + y = 1, x >= 0, y <= 0.5 -> x = 0.5, y = 0.5
        let g = vec![vec![2.0, 0.0], vec![0.0, 2.0]];
        let a = vec![-2.0, -4.0];
        let c = LinearConstraints { normals: vec![vec![1.0, 1.0], vec![1.0, 0.0], vec![0.0, -1.0]], bounds: vec![1.0, 0.0, -0.5], equalities: 1 };
        let s = solve_quadratic_program(&g, &a, &c).unwrap();
        assert!((s.x[0] - 0.5).abs() < 1e-14 && (s.x[1] - 0.5).abs() < 1e-14, "{:?}", s.x);
        assert_eq!(s.active, vec![0, 2]);
        assert!(s.multipliers[2] > 0.0 && s.stationarity_residual < 1e-14);
    }

    #[test]
    fn reports_an_infeasible_program() {
        let g = vec![vec![1.0]];
        let c = LinearConstraints { normals: vec![vec![1.0], vec![-1.0]], bounds: vec![1.0, 0.0], equalities: 0 };
        assert!(solve_quadratic_program(&g, &[0.0], &c).is_err());
    }
}
