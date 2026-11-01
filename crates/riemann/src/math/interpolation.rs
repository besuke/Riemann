//! Linear interpolation (QuantLib `LinearInterpolation`) and monotonic cubic spline (QuantLib
//! `CubicInterpolation`), extrapolating with the end segments.
//!
//! Derived from QuantLib v1.42 `ql/math/interpolations/linearinterpolation.hpp` (Copyright
//! (C) 2000, 2001, 2002, 2003 RiskMap srl; (C) 2003, 2004, 2008 StatPro Italia srl),
//! `ql/math/interpolation.hpp` (Copyright (C) 2002, 2003 Ferdinando Ametrano; (C) 2000,
//! 2001, 2002, 2003 RiskMap srl; (C) 2003, 2004, 2005, 2006 StatPro Italia srl),
//! `ql/math/interpolations/cubicinterpolation.hpp` (Copyright (C) 2000, 2001, 2002, 2003
//! RiskMap srl; (C) 2001, 2002, 2003 Nicolas Di Césaré; (C) 2004, 2008, 2009, 2011 Ferdinando
//! Ametrano; (C) 2009 Sylvain Bertrand; (C) 2013 Peter Caspers; (C) 2016 Nicholas Bertocchi)
//! and `ql/methods/finitedifferences/tridiagonaloperator.cpp` (Copyright (C) 2002, 2003, 2011
//! Ferdinando Ametrano; (C) 2000, 2001, 2002, 2003 RiskMap srl); modified BSD license. See
//! THIRD_PARTY_NOTICES. Values are computed in the same order as QuantLib.

use crate::autodiff::Operand;
use crate::math::solver1d::close;
use crate::Error;

/// Linear interpolation through sorted points; a value, built once.
#[derive(Debug, Clone, PartialEq)]
pub struct Linear<T = f64> {
    x: Vec<f64>,
    y: Vec<T>,
    /// slope of each segment
    s: Vec<T>,
}

impl<T: Operand> Linear<T> {
    /// At least two points with strictly increasing `x`.
    pub fn new(x: Vec<f64>, y: Vec<T>) -> Result<Linear<T>, Error> {
        if x.len() < 2 || x.len() != y.len() {
            return Err(Error::InvalidInput {
                reason: format!(
                    "linear interpolation needs at least 2 points and as many x as y ({} x, {} y)",
                    x.len(),
                    y.len()
                ),
            });
        }
        if x.windows(2).any(|w| w[1] <= w[0]) {
            return Err(Error::InvalidInput {
                reason: "unsorted x values".into(),
            });
        }
        let s = (1..x.len())
            .map(|i| (y[i] - y[i - 1]) / (x[i] - x[i - 1]))
            .collect();
        Ok(Linear { x, y, s })
    }

    /// Segment used for `x`: the first below the range, the last above it.
    fn locate(&self, x: f64) -> usize {
        let n = self.x.len();
        if x < self.x[0] {
            0
        } else if x > self.x[n - 1] {
            n - 2
        } else {
            // upper_bound over x[0..n-1] minus one
            self.x[..n - 1].partition_point(|&v| v <= x) - 1
        }
    }

    pub fn value(&self, x: f64) -> T {
        let i = self.locate(x);
        self.y[i] + self.s[i] * (x - self.x[i])
    }

    pub fn derivative(&self, x: f64) -> T {
        self.s[self.locate(x)]
    }

    pub fn xs(&self) -> &[f64] {
        &self.x
    }

    pub fn ys(&self) -> &[T] {
        &self.y
    }
}

/// Cubic spline with the Hyman monotonicity filter and a second derivative of 0 at both ends
/// (QuantLib `CubicInterpolation` with `Spline`, `monotonic = true` and `SecondDerivative` 0
/// conditions); a value, built once. Outside the range the end segments are extended.
#[derive(Debug, Clone, PartialEq)]
pub struct MonotonicCubicSpline<T = f64> {
    x: Vec<f64>,
    y: Vec<T>,
    a: Vec<T>,
    b: Vec<T>,
    c: Vec<T>,
}

/// Solves the system of the spline: QuantLib `TridiagonalOperator::solveFor`. The matrix depends
/// only on the knots, so only the right-hand side carries derivatives.
fn solve_tridiagonal<T: Operand>(
    lower: &[f64],
    diagonal: &[f64],
    upper: &[f64],
    rhs: &[T],
) -> Result<Vec<T>, Error> {
    let n = diagonal.len();
    let mut result = rhs.to_vec();
    let mut temp = vec![0.0; n];
    let mut bet = diagonal[0];
    if close(bet, 0.0) {
        return Err(Error::InvalidInput {
            reason: format!("diagonal's first element ({bet}) cannot be close to zero"),
        });
    }
    result[0] = rhs[0] / bet;
    for j in 1..n {
        temp[j] = upper[j - 1] / bet;
        bet = diagonal[j] - lower[j - 1] * temp[j];
        if close(bet, 0.0) {
            return Err(Error::InvalidInput {
                reason: "division by zero".into(),
            });
        }
        result[j] = (rhs[j] - result[j - 1] * lower[j - 1]) / bet;
    }
    for j in (0..n - 1).rev() {
        result[j] = result[j] - result[j + 1] * temp[j + 1];
    }
    Ok(result)
}

/// The sign of `d` with its size limited to `limit`, or 0 when `d` does not have the sign
/// of the reference slope.
fn limited<T: Operand>(d: T, reference: T, limit: T) -> T {
    if d.primal() * reference.primal() > 0.0 {
        d / d.abs() * d.abs().min(limit)
    } else {
        d.like(0.0)
    }
}

/// Hyman filter on the first derivatives (QuantLib `CubicInterpolationImpl::update`). The choice
/// of branch reads the values; the derivatives follow the branch taken.
fn hyman_filter<T: Operand>(d: &mut [T], dx: &[f64], s: &[T]) {
    let n = d.len();
    let v = |x: T| x.primal();
    for i in 0..n {
        d[i] = if i == 0 {
            limited(d[i], s[0], (s[0] * 3.0).abs())
        } else if i == n - 1 {
            limited(d[i], s[n - 2], (s[n - 2] * 3.0).abs())
        } else {
            let pm = (s[i - 1] * dx[i] + s[i] * dx[i - 1]) / (dx[i - 1] + dx[i]);
            let mut m = s[i - 1].abs().min(s[i].abs()).min(pm.abs()) * 3.0;
            if i > 1 && (v(s[i - 1]) - v(s[i - 2])) * (v(s[i]) - v(s[i - 1])) > 0.0 {
                let pd = (s[i - 1] * (2.0 * dx[i - 1] + dx[i - 2]) - s[i - 2] * dx[i - 1])
                    / (dx[i - 2] + dx[i - 1]);
                if v(pm) * v(pd) > 0.0 && v(pm) * (v(s[i - 1]) - v(s[i - 2])) > 0.0 {
                    m = m.max(pm.abs().min(pd.abs()) * 1.5);
                }
            }
            if i < n - 2 && (v(s[i]) - v(s[i - 1])) * (v(s[i + 1]) - v(s[i])) > 0.0 {
                let pu =
                    (s[i] * (2.0 * dx[i] + dx[i + 1]) - s[i + 1] * dx[i]) / (dx[i] + dx[i + 1]);
                if v(pm) * v(pu) > 0.0 && -v(pm) * (v(s[i]) - v(s[i - 1])) > 0.0 {
                    m = m.max(pm.abs().min(pu.abs()) * 1.5);
                }
            }
            limited(d[i], pm, m)
        };
    }
}

impl<T: Operand> MonotonicCubicSpline<T> {
    /// At least two points with strictly increasing `x`.
    pub fn new(x: Vec<f64>, y: Vec<T>) -> Result<MonotonicCubicSpline<T>, Error> {
        let n = x.len();
        if n < 2 || n != y.len() {
            return Err(Error::InvalidInput {
                reason: format!(
                    "cubic interpolation needs at least 2 points and as many x as y ({} x, {} y)",
                    n,
                    y.len()
                ),
            });
        }
        if x.windows(2).any(|w| w[1] <= w[0]) {
            return Err(Error::InvalidInput {
                reason: "unsorted x values".into(),
            });
        }
        let dx: Vec<f64> = x.windows(2).map(|w| w[1] - w[0]).collect();
        let s: Vec<T> = (0..n - 1).map(|i| (y[i + 1] - y[i]) / dx[i]).collect();

        let (mut lower, mut diagonal, mut upper) =
            (vec![0.0; n - 1], vec![0.0; n], vec![0.0; n - 1]);
        let mut rhs = y.clone();
        for i in 1..n - 1 {
            lower[i - 1] = dx[i];
            diagonal[i] = 2.0 * (dx[i] + dx[i - 1]);
            upper[i] = dx[i - 1];
            rhs[i] = (s[i - 1] * dx[i] + s[i] * dx[i - 1]) * 3.0;
        }
        // a second derivative of 0 at both ends
        diagonal[0] = 2.0;
        upper[0] = 1.0;
        rhs[0] = s[0] * 3.0;
        lower[n - 2] = 1.0;
        diagonal[n - 1] = 2.0;
        rhs[n - 1] = s[n - 2] * 3.0;
        let mut d = solve_tridiagonal(&lower, &diagonal, &upper, &rhs)?;
        hyman_filter(&mut d, &dx, &s);

        let a = d[..n - 1].to_vec();
        let b = (0..n - 1)
            .map(|i| (s[i] * 3.0 - d[i + 1] - d[i] * 2.0) / dx[i])
            .collect();
        let c = (0..n - 1)
            .map(|i| (d[i + 1] + d[i] - s[i] * 2.0) / (dx[i] * dx[i]))
            .collect();
        Ok(MonotonicCubicSpline { x, y, a, b, c })
    }

    /// Segment used for `x`, as in [`Linear`].
    fn locate(&self, x: f64) -> usize {
        let n = self.x.len();
        if x < self.x[0] {
            0
        } else if x > self.x[n - 1] {
            n - 2
        } else {
            self.x[..n - 1].partition_point(|&v| v <= x) - 1
        }
    }

    pub fn value(&self, x: f64) -> T {
        let j = self.locate(x);
        let dx = x - self.x[j];
        self.y[j] + (self.a[j] + (self.b[j] + self.c[j] * dx) * dx) * dx
    }

    pub fn derivative(&self, x: f64) -> T {
        let j = self.locate(x);
        let dx = x - self.x[j];
        self.a[j] + (self.b[j] * 2.0 + self.c[j] * 3.0 * dx) * dx
    }

    pub fn xs(&self) -> &[f64] {
        &self.x
    }

    pub fn ys(&self) -> &[T] {
        &self.y
    }
}
