//! Sobol low-discrepancy points and the Brownian bridge.
//!
//! Derived from QuantLib v1.42 `ql/math/randomnumbers/sobolrsg.cpp`, `primitivepolynomials.cpp` and
//! `ql/methods/montecarlo/brownianbridge.cpp` and `.hpp` (see THIRD_PARTY_NOTICES for the copyright lines);
//! modified BSD license. The tables are QuantLib's default `Jaeckel` direction integers, which stop at
//! 32 dimensions. Beyond them QuantLib draws random direction integers, and this library stops instead.
//! There is no scrambling and no skipping. As in QuantLib, the first point returned is 0.5 in every
//! dimension: the sequence starts at index 1 and leaves out the origin, the only point with a zero
//! coordinate. Every point therefore lies strictly inside (0, 1), and the inverse normal is finite.
//! Its smallest value is 2^-32. The 2^32 - 1 points after the first exhaust the sequence.

use crate::Error;

/// The largest dimension with tabulated direction integers.
pub const MAX_SOBOL_DIMENSION: usize = 32;

/// For dimensions 2 to 32: the degree of the primitive polynomial, its coefficients and the free
/// direction integers.
const TABLE: [(u32, u32, &[u32]); 31] = [
    (1, 0, &[1]),
    (2, 1, &[1, 1]),
    (3, 1, &[1, 3, 7]),
    (3, 2, &[1, 1, 5]),
    (4, 1, &[1, 3, 1, 1]),
    (4, 4, &[1, 1, 3, 7]),
    (5, 2, &[1, 3, 3, 9, 9]),
    (5, 4, &[1, 3, 7, 7, 21]),
    (5, 7, &[1, 1, 5, 11, 27]),
    (5, 11, &[1, 1, 7, 3, 29]),
    (5, 13, &[1, 3, 7, 13, 3]),
    (5, 14, &[1, 3, 5, 1, 15]),
    (6, 1, &[1, 1, 1, 9, 23, 37]),
    (6, 13, &[1, 1, 3, 13, 11, 7]),
    (6, 16, &[1, 3, 3, 5, 19, 33]),
    (6, 19, &[1, 1, 7, 13, 25, 5]),
    (6, 22, &[1, 1, 1, 3, 13, 39]),
    (6, 25, &[1, 3, 5, 11, 7, 11]),
    (7, 1, &[1, 3, 1, 7, 3, 23, 79]),
    (7, 4, &[1, 3, 1, 15, 17, 63, 13]),
    (7, 7, &[1, 3, 3, 3, 25, 17, 115]),
    (7, 8, &[1, 3, 7, 9, 31, 29, 17]),
    (7, 14, &[1, 1, 3, 15, 29, 15, 41]),
    (7, 19, &[1, 3, 1, 9, 5, 21, 119]),
    (7, 21, &[1, 1, 5, 5, 1, 27, 33]),
    (7, 28, &[1, 1, 3, 1, 23, 13, 75]),
    (7, 31, &[1, 1, 7, 7, 19, 25, 105]),
    (7, 32, &[1, 3, 5, 5, 21, 9, 7]),
    (7, 37, &[1, 1, 1, 15, 5, 49, 59]),
    (7, 41, &[1, 3, 5, 15, 17, 19, 21]),
    (7, 42, &[1, 1, 7, 11, 13, 29, 3]),
];

/// The points of a Sobol sequence, by the Gray code of Antonov and Saleev.
#[derive(Debug, Clone)]
pub struct SobolSequence {
    direction: Vec<[u32; 32]>,
    integers: Vec<u32>,
    counter: u32,
    first: bool,
}

impl SobolSequence {
    pub fn new(dimension: usize) -> Result<Self, Error> {
        if dimension == 0 || dimension > MAX_SOBOL_DIMENSION {
            return Err(Error::InvalidInput {
                reason: format!("a Sobol sequence has 1 to {MAX_SOBOL_DIMENSION} dimensions, not {dimension}"),
            });
        }
        let mut direction = vec![[0u32; 32]; dimension];
        for (j, value) in direction[0].iter_mut().enumerate() {
            *value = 1u32 << (31 - j);
        }
        for (k, column) in direction.iter_mut().enumerate().skip(1) {
            let (degree, polynomial, initial) = TABLE[k - 1];
            for (j, &value) in initial.iter().enumerate() {
                column[j] = value << (31 - j);
            }
            let gk = degree as usize;
            for l in gk..32 {
                let mut n = column[l - gk] >> gk;
                for j in 1..gk {
                    if (polynomial >> (gk - j - 1)) & 1 != 0 {
                        n ^= column[l - j];
                    }
                }
                n ^= column[l - gk];
                column[l] = n;
            }
        }
        let integers = direction.iter().map(|c| c[0]).collect();
        Ok(SobolSequence { direction, integers, counter: 0, first: true })
    }

    /// The largest number of points one sequence gives.
    pub const MAX_POINTS: u64 = 1 << 32;

    /// Moves to the next point and returns its integers; divide by 2^32 for the point in (0, 1).
    /// Panics after [`SobolSequence::MAX_POINTS`] points, where QuantLib fails too: callers check the count.
    pub fn next_integers(&mut self) -> &[u32] {
        if self.first {
            self.first = false;
            return &self.integers;
        }
        self.counter = self.counter.checked_add(1).expect("the Sobol sequence is exhausted");
        let j = self.counter.trailing_ones() as usize;
        for (value, column) in self.integers.iter_mut().zip(&self.direction) {
            *value ^= column[j];
        }
        &self.integers
    }

    /// Writes the next point to `out`.
    pub fn next_point(&mut self, out: &mut [f64]) {
        for (o, &v) in out.iter_mut().zip(self.next_integers()) {
            *o = f64::from(v) * (0.5 / 2_147_483_648.0);
        }
    }
}

/// Turns standard normal variates into increments of unit variance, largest scales first: the first
/// variate sets the end of the path, the next ones the midpoints.
#[derive(Debug, Clone)]
pub struct BrownianBridge {
    sqrt_dt: Vec<f64>,
    bridge_index: Vec<usize>,
    left_index: Vec<usize>,
    right_index: Vec<usize>,
    left_weight: Vec<f64>,
    right_weight: Vec<f64>,
    std_dev: Vec<f64>,
}

impl BrownianBridge {
    /// `times` are the increasing, positive times of the path, without zero.
    pub fn new(times: &[f64]) -> Self {
        let size = times.len();
        let mut sqrt_dt = vec![0.0; size];
        sqrt_dt[0] = times[0].sqrt();
        for i in 1..size {
            sqrt_dt[i] = (times[i] - times[i - 1]).sqrt();
        }
        let mut bridge = BrownianBridge {
            sqrt_dt,
            bridge_index: vec![0; size],
            left_index: vec![0; size],
            right_index: vec![0; size],
            left_weight: vec![0.0; size],
            right_weight: vec![0.0; size],
            std_dev: vec![0.0; size],
        };
        let mut map = vec![0usize; size];
        map[size - 1] = 1;
        bridge.bridge_index[0] = size - 1;
        bridge.std_dev[0] = times[size - 1].sqrt();
        let mut j = 0;
        for i in 1..size {
            while map[j] != 0 {
                j += 1;
            }
            let mut k = j;
            while map[k] == 0 {
                k += 1;
            }
            let l = j + ((k - 1 - j) >> 1);
            map[l] = i;
            bridge.bridge_index[i] = l;
            bridge.left_index[i] = j;
            bridge.right_index[i] = k;
            if j != 0 {
                let span = times[k] - times[j - 1];
                bridge.left_weight[i] = (times[k] - times[l]) / span;
                bridge.right_weight[i] = (times[l] - times[j - 1]) / span;
                bridge.std_dev[i] = ((times[l] - times[j - 1]) * (times[k] - times[l]) / span).sqrt();
            } else {
                bridge.left_weight[i] = (times[k] - times[l]) / times[k];
                bridge.right_weight[i] = times[l] / times[k];
                bridge.std_dev[i] = (times[l] * (times[k] - times[l]) / times[k]).sqrt();
            }
            j = k + 1;
            if j >= size {
                j = 0;
            }
        }
        bridge
    }

    /// Writes to `out` the unit-variance increments that the variates `input` build.
    // the loop follows QuantLib: the index picks a row of several tables at once
    #[allow(clippy::needless_range_loop)]
    pub fn transform(&self, input: &[f64], out: &mut [f64]) {
        let size = self.sqrt_dt.len();
        out[size - 1] = self.std_dev[0] * input[0];
        for i in 1..size {
            let (j, k, l) = (self.left_index[i], self.right_index[i], self.bridge_index[i]);
            out[l] = if j != 0 {
                self.left_weight[i] * out[j - 1] + self.right_weight[i] * out[k] + self.std_dev[i] * input[i]
            } else {
                self.right_weight[i] * out[k] + self.std_dev[i] * input[i]
            };
        }
        for i in (1..size).rev() {
            out[i] -= out[i - 1];
            out[i] /= self.sqrt_dt[i];
        }
        out[0] /= self.sqrt_dt[0];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_point_is_the_middle_and_no_coordinate_is_zero() {
        // the first point is 0.5 in every dimension (not 0): its normal variates are all 0
        let mut s = SobolSequence::new(32).unwrap();
        let mut p = vec![0.0; 32];
        s.next_point(&mut p);
        assert!(p.iter().all(|&x| x == 0.5));
        for _ in 1..(1 << 16) {
            let integers = s.next_integers();
            assert!(integers.iter().all(|&v| v != 0));
        }
        s.next_point(&mut p);
        assert!(p.iter().all(|&x| x > 0.0 && x < 1.0));
        assert!(SobolSequence::new(0).is_err());
        assert!(SobolSequence::new(33).is_err());
    }

    #[test]
    fn each_dimension_is_balanced_over_a_power_of_two() {
        // The points 0 to 2^12 - 1 of the sequence put 2^12 / 64 of them in each interval [i / 64, (i + 1) / 64)
        // of every dimension. QuantLib returns the points from index 1 on, leaving out the origin, so the
        // first 4095 points returned, with the origin added, must show this balance.
        let mut s = SobolSequence::new(32).unwrap();
        let mut p = vec![0.0; 32];
        let mut cells = vec![[0usize; 64]; 32];
        for column in cells.iter_mut() {
            column[0] += 1; // the origin
        }
        for _ in 0..4095 {
            s.next_point(&mut p);
            for (column, x) in cells.iter_mut().zip(&p) {
                column[(x * 64.0) as usize] += 1;
            }
        }
        assert!(cells.iter().all(|c| c.iter().all(|&n| n == 64)), "{cells:?}");
    }

    /// The bridge is linear: column i of `m` is the output for the i-th unit vector.
    fn bridge_matrix(times: &[f64]) -> Vec<Vec<f64>> {
        let n = times.len();
        let bridge = BrownianBridge::new(times);
        let mut m = vec![vec![0.0; n]; n];
        for i in 0..n {
            let mut e = vec![0.0; n];
            e[i] = 1.0;
            let mut out = vec![0.0; n];
            bridge.transform(&e, &mut out);
            for (row, value) in m.iter_mut().zip(&out) {
                row[i] = *value;
            }
        }
        m
    }

    #[test]
    fn the_bridge_gives_the_covariance_of_brownian_motion() {
        // For independent standard normal inputs, the covariance of the outputs is m m^T. It must be the
        // identity: unit-variance, independent increments. Then W(t_i) = sum of sqrt(dt) z has the
        // variance t_i and the covariance min(t_i, t_j). Checked on a uniform and an uneven grid.
        let uneven = [0.1, 0.25, 0.3, 0.7, 1.0, 1.05, 1.6, 2.5, 3.0, 4.2, 5.0];
        let uniform: Vec<f64> = (1..=12).map(|i| f64::from(i) / 12.0).collect();
        for times in [uniform.as_slice(), &uneven[..]] {
            let n = times.len();
            let m = bridge_matrix(times);
            let dt: Vec<f64> = (0..n).map(|i| times[i] - if i == 0 { 0.0 } else { times[i - 1] }).collect();
            for i in 0..n {
                for j in 0..n {
                    let increments: f64 = (0..n).map(|k| m[i][k] * m[j][k]).sum();
                    let expected = if i == j { 1.0 } else { 0.0 };
                    assert!((increments - expected).abs() < 1e-13, "{i} {j} {increments}");
                    // the covariance of the path levels W(t_i) and W(t_j)
                    let level = |a: usize, k: usize| (0..=a).map(|r| dt[r].sqrt() * m[r][k]).sum::<f64>();
                    let covariance: f64 = (0..n).map(|k| level(i, k) * level(j, k)).sum();
                    assert!((covariance - times[i.min(j)]).abs() < 1e-13, "{i} {j} {covariance}");
                }
            }
            // the first variate alone sets the end of the path, with the standard deviation sqrt(T)
            let end: f64 = (0..n).map(|r| dt[r].sqrt() * m[r][0]).sum();
            assert!((end - times[n - 1].sqrt()).abs() < 1e-13, "{end}");
        }
    }
}
