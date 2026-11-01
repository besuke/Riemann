//! Random numbers by counter: the number for a path and a dimension depends only on the seed, the
//! path and the dimension, so it does not change with the number of threads or the order of
//! evaluation. The mixing function is the finalizer of SplitMix64 (Steele, Lea and Flood, 2014).

use crate::math::normal::inverse_normal_cdf;

const GAMMA_PATH: u64 = 0x9E37_79B9_7F4A_7C15;
const GAMMA_DIMENSION: u64 = 0xD1B5_4A32_D192_ED03;

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A uniform number strictly between 0 and 1.
pub fn uniform(seed: u64, path: u64, dimension: u64) -> f64 {
    let stream = mix(seed.wrapping_add((path.wrapping_add(1)).wrapping_mul(GAMMA_PATH)));
    let bits = mix(stream.wrapping_add((dimension.wrapping_add(1)).wrapping_mul(GAMMA_DIMENSION)));
    ((bits >> 11) as f64 + 0.5) * (1.0 / (1u64 << 53) as f64)
}

/// A standard normal number, by inverting the distribution function.
pub fn standard_normal(seed: u64, path: u64, dimension: u64) -> f64 {
    inverse_normal_cdf(uniform(seed, path, dimension))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moments_of_the_normal_numbers() {
        let n = 200_000u64;
        let z: Vec<f64> = (0..n).map(|p| standard_normal(42, p, 0)).collect();
        let mean = z.iter().sum::<f64>() / n as f64;
        let variance = z.iter().map(|x| x * x).sum::<f64>() / n as f64 - mean * mean;
        let kurtosis = z.iter().map(|x| x.powi(4)).sum::<f64>() / n as f64;
        assert!(mean.abs() < 4.0 / (n as f64).sqrt(), "{mean}");
        assert!((variance - 1.0).abs() < 0.02, "{variance}");
        assert!((kurtosis - 3.0).abs() < 0.1, "{kurtosis}");
    }

    #[test]
    fn dimensions_and_paths_are_uncorrelated() {
        let n = 200_000u64;
        let across_dimensions: f64 = (0..n)
            .map(|p| standard_normal(7, p, 0) * standard_normal(7, p, 1))
            .sum::<f64>()
            / n as f64;
        let across_paths: f64 = (0..n)
            .map(|p| standard_normal(7, p, 0) * standard_normal(7, p + 1, 0))
            .sum::<f64>()
            / n as f64;
        let limit = 4.0 / (n as f64).sqrt();
        assert!(across_dimensions.abs() < limit, "{across_dimensions}");
        assert!(across_paths.abs() < limit, "{across_paths}");
    }

    #[test]
    fn the_same_inputs_give_the_same_number() {
        assert_eq!(uniform(1, 2, 3), uniform(1, 2, 3));
        assert_ne!(uniform(1, 2, 3), uniform(2, 2, 3));
        assert_ne!(uniform(1, 2, 3), uniform(1, 3, 3));
        assert_ne!(uniform(1, 2, 3), uniform(1, 2, 4));
    }
}
