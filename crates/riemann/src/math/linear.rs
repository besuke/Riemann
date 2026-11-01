//! Dense linear algebra: Gaussian elimination with partial pivoting.

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
pub fn inverse(a: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, Error> {
    let identity: Vec<Vec<f64>> = (0..a.len())
        .map(|i| (0..a.len()).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
        .collect();
    solve(a, &identity)
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
        let inv = inverse(&a).unwrap();
        for i in 0..3 {
            for j in 0..3 {
                let product: f64 = (0..3).map(|k| a[i][k] * inv[k][j]).sum();
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!((product - expected).abs() < 1e-14, "{i} {j} {product}");
            }
        }
    }

    #[test]
    fn a_singular_matrix_is_an_error() {
        assert!(inverse(&[vec![1.0, 2.0], vec![2.0, 4.0]]).is_err());
        assert!(inverse(&[vec![1.0, 2.0]]).is_err());
    }
}
