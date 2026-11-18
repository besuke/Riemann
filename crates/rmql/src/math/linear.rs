//! Dense linear algebra: Gaussian elimination with partial pivoting, least squares by QR, Cholesky, and
//! the eigenvalues of a symmetric matrix by Jacobi rotations.

use crate::Error;

fn invalid(reason: &str) -> Error {
    Error::InvalidInput {
        reason: reason.into(),
    }
}

/// Solves `a x = b` for several right-hand sides, the columns of `b`. The pivots are chosen in a
/// fixed way, so the same input always gives the same result.
#[allow(clippy::needless_range_loop)]
pub fn solve(a: &[Vec<f64>], b: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, Error> {
    let n = a.len();
    if n == 0 || a.iter().any(|row| row.len() != n) {
        return Err(invalid("a linear system needs a square matrix"));
    }
    if b.len() != n {
        return Err(invalid("the right-hand side has the wrong number of rows"));
    }
    let scale = a
        .iter()
        .flatten()
        .fold(0.0_f64, |largest, x| largest.max(x.abs()));
    let (mut a, mut x) = (a.to_vec(), b.to_vec());
    for k in 0..n {
        let mut pivot = k;
        for i in k + 1..n {
            if a[i][k].abs() > a[pivot][k].abs() {
                pivot = i;
            }
        }
        if a[pivot][k].abs() <= f64::EPSILON * scale {
            return Err(invalid("the matrix of the linear system is singular"));
        }
        a.swap(k, pivot);
        x.swap(k, pivot);
        for i in k + 1..n {
            let factor = a[i][k] / a[k][k];
            if factor != 0.0 {
                for j in k..n {
                    a[i][j] -= factor * a[k][j];
                }
                for j in 0..x[i].len() {
                    x[i][j] -= factor * x[k][j];
                }
            }
        }
    }
    for k in (0..n).rev() {
        for j in 0..x[k].len() {
            let mut sum = x[k][j];
            for i in k + 1..n {
                sum -= a[k][i] * x[i][j];
            }
            x[k][j] = sum / a[k][k];
        }
    }
    Ok(x)
}

/// The inverse of a square matrix.
pub fn compute_inverse(a: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, Error> {
    let identity: Vec<Vec<f64>> = (0..a.len())
        .map(|i| (0..a.len()).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
        .collect();
    solve(a, &identity)
}

/// The least-squares solution of `design x = target`, where `design` holds rows of `columns` values.
/// Householder reflections with column pivoting, on columns scaled to unit length. A column whose
/// remaining norm is at most `rows * EPSILON` adds nothing up to rounding, for example one that is a
/// combination of the others: it gets a coefficient of zero, and the fitted values are still the
/// projection of `target` on the columns.
pub fn solve_least_squares(design: &[f64], columns: usize, target: &[f64]) -> Result<Vec<f64>, Error> {
    let rows = target.len();
    if columns == 0 || design.len() != rows * columns || rows < columns {
        return Err(invalid("least squares needs as many values as rows times columns, and rows >= columns"));
    }
    let mut a: Vec<Vec<f64>> = (0..columns).map(|c| (0..rows).map(|r| design[r * columns + c]).collect()).collect();
    let scale: Vec<f64> = a.iter().map(|c| c.iter().map(|v| v * v).sum::<f64>().sqrt()).collect();
    if scale.iter().any(|s| !s.is_finite()) || target.iter().any(|t| !t.is_finite()) {
        return Err(invalid("least squares needs finite values"));
    }
    for (column, s) in a.iter_mut().zip(&scale) {
        if *s > 0.0 {
            column.iter_mut().for_each(|v| *v /= s);
        }
    }
    let mut y = target.to_vec();
    let mut order: Vec<usize> = (0..columns).collect();
    let threshold = rows as f64 * f64::EPSILON;
    let mut rank = 0;
    for k in 0..columns {
        let remaining = |c: &[f64]| c[k..].iter().map(|v| v * v).sum::<f64>();
        let mut best = k;
        let mut best_norm = remaining(&a[k]);
        for (c, column) in a.iter().enumerate().skip(k + 1) {
            let norm = remaining(column);
            if norm > best_norm {
                best = c;
                best_norm = norm;
            }
        }
        if best_norm.sqrt() <= threshold {
            break;
        }
        a.swap(k, best);
        order.swap(k, best);
        let norm = best_norm.sqrt();
        let alpha = if a[k][k] > 0.0 { -norm } else { norm };
        let mut v: Vec<f64> = a[k][k..].to_vec();
        v[0] -= alpha;
        let vv: f64 = v.iter().map(|x| x * x).sum();
        if vv > 0.0 {
            for column in a.iter_mut().skip(k + 1) {
                let dot: f64 = v.iter().zip(&column[k..]).map(|(p, q)| p * q).sum();
                let factor = 2.0 * dot / vv;
                column[k..].iter_mut().zip(&v).for_each(|(q, p)| *q -= factor * p);
            }
            let dot: f64 = v.iter().zip(&y[k..]).map(|(p, q)| p * q).sum();
            let factor = 2.0 * dot / vv;
            y[k..].iter_mut().zip(&v).for_each(|(q, p)| *q -= factor * p);
        }
        a[k][k] = alpha;
        rank = k + 1;
    }
    let mut solution = vec![0.0; columns];
    for k in (0..rank).rev() {
        let mut sum = y[k];
        for j in k + 1..rank {
            sum -= a[j][k] * solution[order[j]];
        }
        solution[order[k]] = sum / a[k][k];
    }
    for (value, s) in solution.iter_mut().zip(&scale) {
        if *s > 0.0 {
            *value /= s;
        }
    }
    Ok(solution)
}

/// The lower-triangular `L` with `L L^T = a`, for a symmetric positive definite `a`. A pivot at or
/// below `n * EPSILON` times the largest diagonal entry is an error: the matrix is not positive definite.
#[allow(clippy::needless_range_loop)]
pub fn compute_cholesky(a: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, Error> {
    let n = a.len();
    if n == 0 || a.iter().any(|row| row.len() != n) {
        return Err(invalid("Cholesky needs a square matrix"));
    }
    let largest = (0..n).map(|i| a[i][i].abs()).fold(0.0, f64::max);
    let mut l = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..=i {
            let sum: f64 = (0..j).map(|k| l[i][k] * l[j][k]).sum();
            if i == j {
                let pivot = a[i][i] - sum;
                if pivot.is_nan() || pivot <= n as f64 * f64::EPSILON * largest {
                    return Err(invalid("the matrix is not positive definite"));
                }
                l[i][i] = pivot.sqrt();
            } else {
                l[i][j] = (a[i][j] - sum) / l[j][j];
            }
        }
    }
    Ok(l)
}

/// The eigenvalues of a symmetric matrix, in decreasing order, and its eigenvectors: column `k` of
/// `vectors` (`vectors[i][k]`) is the unit eigenvector of value `k`. Cyclic Jacobi rotations, until the
/// off-diagonal entries are negligible next to the matrix; the same input always gives the same result.
#[allow(clippy::needless_range_loop)]
pub fn compute_symmetric_eigen(a: &[Vec<f64>]) -> Result<(Vec<f64>, Vec<Vec<f64>>), Error> {
    let n = a.len();
    if n == 0 || a.iter().any(|row| row.len() != n || row.iter().any(|x| !x.is_finite())) {
        return Err(invalid("the eigen decomposition needs a finite square matrix"));
    }
    for i in 0..n {
        for j in 0..i {
            if (a[i][j] - a[j][i]).abs() > 1e-12 * (a[i][j].abs() + a[j][i].abs()).max(1.0) {
                return Err(invalid("the eigen decomposition needs a symmetric matrix"));
            }
        }
    }
    let mut m: Vec<Vec<f64>> = (0..n).map(|i| (0..n).map(|j| (a[i][j] + a[j][i]) / 2.0).collect()).collect();
    let mut v: Vec<Vec<f64>> = (0..n).map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect()).collect();
    let scale: f64 = m.iter().flatten().map(|x| x * x).sum();
    let mut converged = scale == 0.0;
    for _sweep in 0..100 {
        let off: f64 = (0..n).flat_map(|i| (0..i).map(move |j| (i, j))).map(|(i, j)| m[i][j] * m[i][j]).sum();
        if off <= 1e-32 * scale {
            converged = true;
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                if m[p][q] == 0.0 {
                    continue;
                }
                // the rotation that zeroes m[p][q]: tan of its angle, the smaller root
                let theta = (m[q][q] - m[p][p]) / (2.0 * m[p][q]);
                let t = if theta.abs() > 1e150 { 0.5 / theta } else { theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt()) };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..n {
                    let (x, y) = (m[k][p], m[k][q]);
                    m[k][p] = c * x - s * y;
                    m[k][q] = s * x + c * y;
                }
                for k in 0..n {
                    let (x, y) = (m[p][k], m[q][k]);
                    m[p][k] = c * x - s * y;
                    m[q][k] = s * x + c * y;
                }
                m[p][q] = 0.0;
                m[q][p] = 0.0;
                for k in 0..n {
                    let (x, y) = (v[k][p], v[k][q]);
                    v[k][p] = c * x - s * y;
                    v[k][q] = s * x + c * y;
                }
            }
        }
    }
    if !converged {
        return Err(invalid("the Jacobi rotations did not converge"));
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&x, &y| m[y][y].total_cmp(&m[x][x]).then(x.cmp(&y)));
    let values = order.iter().map(|&k| m[k][k]).collect();
    let vectors = (0..n).map(|i| order.iter().map(|&k| v[i][k]).collect()).collect();
    Ok((values, vectors))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::needless_range_loop)]
    fn inverse_times_matrix_is_the_identity() {
        let a = vec![
            vec![0.0, 2.0, 1.0],
            vec![1.0, -2.0, -3.0],
            vec![-1.0, 1.0, 2.0],
        ];
        let inv = compute_inverse(&a).unwrap();
        for i in 0..3 {
            for j in 0..3 {
                let product: f64 = (0..3).map(|k| a[i][k] * inv[k][j]).sum();
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!((product - expected).abs() < 1e-14, "{i} {j} {product}");
            }
        }
    }

    #[test]
    #[allow(clippy::needless_range_loop)]
    fn symmetric_eigen_decomposition_rebuilds_the_matrix() {
        let a = vec![
            vec![4.0, 1.0, -2.0, 0.5],
            vec![1.0, 3.0, 0.0, 1.0],
            vec![-2.0, 0.0, 5.0, -1.0],
            vec![0.5, 1.0, -1.0, 2.0],
        ];
        let (values, vectors) = compute_symmetric_eigen(&a).unwrap();
        assert!(values.windows(2).all(|w| w[0] >= w[1]));
        assert!((values.iter().sum::<f64>() - 14.0).abs() < 1e-13);
        for i in 0..4 {
            for j in 0..4 {
                let rebuilt: f64 = (0..4).map(|k| vectors[i][k] * values[k] * vectors[j][k]).sum();
                let dot: f64 = (0..4).map(|k| vectors[k][i] * vectors[k][j]).sum();
                assert!((rebuilt - a[i][j]).abs() < 1e-13, "{i} {j} {rebuilt}");
                assert!((dot - if i == j { 1.0 } else { 0.0 }).abs() < 1e-13);
            }
        }
        // [[2, 1], [1, 2]] has the values 3 and 1
        let (two, _) = compute_symmetric_eigen(&[vec![2.0, 1.0], vec![1.0, 2.0]]).unwrap();
        assert!((two[0] - 3.0).abs() < 1e-15 && (two[1] - 1.0).abs() < 1e-15);
        assert!(compute_symmetric_eigen(&[vec![1.0, 2.0], vec![0.0, 1.0]]).is_err());
    }

    #[test]
    fn least_squares_fits_and_gives_zero_to_a_dependent_column() {
        // y = 1 + 2 x + noise; the third column is 3 - x, a combination of the first two
        let xs = [1.0, 2.0, 3.0, 4.0, 5.0];
        let noise = [0.1, -0.2, 0.05, 0.1, -0.05];
        let target: Vec<f64> = xs.iter().zip(&noise).map(|(x, e)| 1.0 + 2.0 * x + e).collect();
        let full: Vec<f64> = xs.iter().flat_map(|&x| [1.0, x, 3.0 - x]).collect();
        let plain: Vec<f64> = xs.iter().flat_map(|&x| [1.0, x]).collect();
        let b3 = solve_least_squares(&full, 3, &target).unwrap();
        let b2 = solve_least_squares(&plain, 2, &target).unwrap();
        assert_eq!(b3.iter().filter(|b| **b == 0.0).count(), 1, "{b3:?}");
        for x in xs {
            let fitted3 = b3[0] + b3[1] * x + b3[2] * (3.0 - x);
            let fitted2 = b2[0] + b2[1] * x;
            assert!((fitted3 - fitted2).abs() < 1e-13, "{fitted3} {fitted2}");
        }
        // the normal equations: the residual is orthogonal to the columns
        let residual: Vec<f64> = xs.iter().zip(&target).map(|(x, y)| y - b2[0] - b2[1] * x).collect();
        assert!(residual.iter().sum::<f64>().abs() < 1e-13);
        assert!(xs.iter().zip(&residual).map(|(x, r)| x * r).sum::<f64>().abs() < 1e-12);
        assert!(solve_least_squares(&full, 3, &target[..2]).is_err());
    }

    #[test]
    fn cholesky_rebuilds_the_matrix() {
        let a = vec![vec![1.0, 0.8, 0.5], vec![0.8, 1.0, 0.7], vec![0.5, 0.7, 1.0]];
        let l = compute_cholesky(&a).unwrap();
        for i in 0..3 {
            assert_eq!(l[i][(i + 1)..].iter().filter(|v| **v != 0.0).count(), 0);
            for j in 0..3 {
                let product: f64 = (0..3).map(|k| l[i][k] * l[j][k]).sum();
                assert!((product - a[i][j]).abs() < 1e-15, "{i} {j} {product}");
            }
        }
        assert!(compute_cholesky(&[vec![1.0, 2.0], vec![2.0, 1.0]]).is_err());
    }

    #[test]
    fn a_singular_matrix_is_an_error() {
        assert!(compute_inverse(&[vec![1.0, 2.0], vec![2.0, 4.0]]).is_err());
        assert!(compute_inverse(&[vec![1.0, 2.0]]).is_err());
    }
}
