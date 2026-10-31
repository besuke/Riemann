//! One-dimensional root finding: QuantLib's bracketing search followed by NewtonSafe.
//!
//! Derived from QuantLib v1.42 `ql/math/solver1d.hpp` and `ql/math/solvers1d/newtonsafe.hpp`
//! (Copyright (C) 2000, 2001, 2002, 2003 RiskMap srl) and `ql/math/comparison.hpp`
//! (Copyright (C) 2003, 2004, 2005, 2007 StatPro Italia srl); modified BSD license.
//! See THIRD_PARTY_NOTICES. The same steps are taken so that results match QuantLib exactly.
//!
//! The solver is a pure function: the objective and its derivative are passed in,
//! and no state survives the call.

use crate::Error;

/// QuantLib `close(x, y)` with the default tolerance of 42 machine epsilons.
pub fn close(x: f64, y: f64) -> bool {
    if x == y {
        return true;
    }
    let diff = (x - y).abs();
    let tolerance = 42.0 * f64::EPSILON;
    if x == 0.0 || y == 0.0 {
        return diff < tolerance * tolerance;
    }
    diff <= tolerance * x.abs() && diff <= tolerance * y.abs()
}

/// Finds a root of `f` near `guess` (QuantLib `Solver1D::solve(f, accuracy, guess, step)`
/// with `NewtonSafe`). `f` returns the value and `df` the derivative.
pub fn newton_safe<F, D>(
    f: F,
    df: D,
    accuracy: f64,
    guess: f64,
    step: f64,
    max_evaluations: usize,
) -> Result<f64, Error>
where
    F: Fn(f64) -> Result<f64, Error>,
    D: Fn(f64) -> Result<f64, Error>,
{
    if accuracy <= 0.0 {
        return Err(Error::InvalidInput { reason: format!("accuracy ({accuracy}) must be positive") });
    }
    let accuracy = accuracy.max(f64::EPSILON);
    let growth_factor = 1.6;
    let mut flipflop = -1;
    let mut root = guess;
    let mut fx_max = f(root)?;
    let (mut x_min, mut fx_min, mut x_max);
    // monotonically crescent bias, as in optionValue(volatility)
    if close(fx_max, 0.0) {
        return Ok(root);
    } else if fx_max > 0.0 {
        x_min = root - step;
        fx_min = f(x_min)?;
        x_max = root;
    } else {
        x_min = root;
        fx_min = fx_max;
        x_max = root + step;
        fx_max = f(x_max)?;
    }
    let mut evaluations = 2;
    while evaluations <= max_evaluations {
        if fx_min * fx_max <= 0.0 {
            if close(fx_min, 0.0) {
                return Ok(x_min);
            }
            if close(fx_max, 0.0) {
                return Ok(x_max);
            }
            root = (x_max + x_min) / 2.0;
            return newton_safe_impl(&f, &df, accuracy, root, x_min, fx_min, x_max, evaluations, max_evaluations);
        }
        if fx_min.abs() < fx_max.abs() {
            x_min += growth_factor * (x_min - x_max);
            fx_min = f(x_min)?;
        } else if fx_min.abs() > fx_max.abs() {
            x_max += growth_factor * (x_max - x_min);
            fx_max = f(x_max)?;
        } else if flipflop == -1 {
            x_min += growth_factor * (x_min - x_max);
            fx_min = f(x_min)?;
            evaluations += 1;
            flipflop = 1;
        } else {
            x_max += growth_factor * (x_max - x_min);
            fx_max = f(x_max)?;
            flipflop = -1;
        }
        evaluations += 1;
    }
    Err(Error::NoConvergence {
        reason: format!(
            "unable to bracket root in {max_evaluations} function evaluations (last bracket attempt: f[{x_min},{x_max}] -> [{fx_min},{fx_max}])"
        ),
    })
}

#[allow(clippy::too_many_arguments)]
fn newton_safe_impl<F, D>(
    f: &F,
    df: &D,
    x_accuracy: f64,
    mut root: f64,
    x_min: f64,
    fx_min: f64,
    x_max: f64,
    mut evaluations: usize,
    max_evaluations: usize,
) -> Result<f64, Error>
where
    F: Fn(f64) -> Result<f64, Error>,
    D: Fn(f64) -> Result<f64, Error>,
{
    // Orient the search so that f(xl) < 0
    let (mut xl, mut xh) = if fx_min < 0.0 { (x_min, x_max) } else { (x_max, x_min) };
    // the "stepsize before last"
    let mut dxold = x_max - x_min;
    let mut dx = dxold;
    let mut froot = f(root)?;
    let mut dfroot = df(root)?;
    evaluations += 1;
    while evaluations <= max_evaluations {
        // Bisect if (out of range || not decreasing fast enough)
        if ((root - xh) * dfroot - froot) * ((root - xl) * dfroot - froot) > 0.0
            || (2.0 * froot).abs() > (dxold * dfroot).abs()
        {
            dxold = dx;
            dx = (xh - xl) / 2.0;
            root = xl + dx;
        } else {
            dxold = dx;
            dx = froot / dfroot;
            root -= dx;
        }
        // Convergence criterion
        if dx.abs() < x_accuracy {
            f(root)?;
            return Ok(root);
        }
        froot = f(root)?;
        dfroot = df(root)?;
        evaluations += 1;
        if froot < 0.0 {
            xl = root;
        } else {
            xh = root;
        }
    }
    Err(Error::NoConvergence { reason: format!("maximum number of function evaluations ({max_evaluations}) exceeded") })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_square_root_of_two() {
        let r = newton_safe(|x| Ok(x * x - 2.0), |x| Ok(2.0 * x), 1e-12, 1.0, 0.1, 100).unwrap();
        assert!((r - 2f64.sqrt()).abs() < 1e-12);
    }
}
