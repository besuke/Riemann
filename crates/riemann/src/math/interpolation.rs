//! Linear interpolation (QuantLib `LinearInterpolation`, extrapolating with the end segments).
//!
//! Derived from QuantLib v1.42 `ql/math/interpolations/linearinterpolation.hpp` (Copyright
//! (C) 2000, 2001, 2002, 2003 RiskMap srl; (C) 2003, 2004, 2008 StatPro Italia srl) and
//! `ql/math/interpolation.hpp` (Copyright (C) 2002, 2003 Ferdinando Ametrano; (C) 2000,
//! 2001, 2002, 2003 RiskMap srl; (C) 2003, 2004, 2005, 2006 StatPro Italia srl); modified BSD
//! license. See THIRD_PARTY_NOTICES. Values are computed in the same order as QuantLib.

use crate::Error;

/// Linear interpolation through sorted points; a value, built once.
#[derive(Debug, Clone, PartialEq)]
pub struct Linear {
    x: Vec<f64>,
    y: Vec<f64>,
    /// slope of each segment
    s: Vec<f64>,
}

impl Linear {
    /// At least two points with strictly increasing `x`.
    pub fn new(x: Vec<f64>, y: Vec<f64>) -> Result<Linear, Error> {
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

    pub fn value(&self, x: f64) -> f64 {
        let i = self.locate(x);
        self.y[i] + (x - self.x[i]) * self.s[i]
    }

    pub fn derivative(&self, x: f64) -> f64 {
        self.s[self.locate(x)]
    }

    pub fn xs(&self) -> &[f64] {
        &self.x
    }

    pub fn ys(&self) -> &[f64] {
        &self.y
    }
}
