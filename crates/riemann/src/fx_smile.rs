//! FX volatility smiles quoted by delta: strikes from deltas and deltas from strikes under the FX
//! conventions (spot or forward delta, with or without the premium), ATM strikes, and a surface of Black
//! volatilities from volatilities at put and call deltas and ATM by expiry.
//!
//! Derived from QuantLib v1.42 `ql/pricingengines/blackdeltacalculator.cpp` (Copyright (C) 2010
//! Dimitri Reiswich), `ql/termstructures/volatility/equityfx/blackvolsurfacedelta.cpp` (Copyright (C)
//! 2019 Quaternion Risk Management Ltd; (C) 2022 Skandinaviska Enskilda Banken AB (publ); (C) 2025
//! Paolo D'Elia), `ql/termstructures/volatility/equityfx/blackvariancecurve.cpp` (Copyright (C) 2002,
//! 2003, 2004 Ferdinando Ametrano; (C) 2003 StatPro Italia srl; (C) 2026 Paolo D'Elia) and
//! `ql/termstructures/volatility/interpolatedsmilesection.hpp` (Copyright (C) 2006 Ferdinando Ametrano;
//! (C) 2006 François du Vignaud; (C) 2015 Peter Caspers); modified BSD license. See
//! THIRD_PARTY_NOTICES. The steps are those of QuantLib, so that the results match it
//! (`docs/spec_prdc.md` 14 章).

// `!(x > 0.0)` is intended: it also rejects NaN
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::curves::YieldCurve;
use crate::math::interpolation::{Linear, MonotonicCubicSpline};
use crate::math::normal::{compute_inverse_normal_cdf, compute_normal_cdf, compute_normal_pdf};
use crate::math::solver1d::{is_close, is_close_enough, solve_brent};
use crate::options::OptionType;
use crate::Error;

fn invalid<T>(reason: String) -> Result<T, Error> {
    Err(Error::InvalidInput { reason })
}

/// How a delta is quoted (QuantLib `DeltaVolQuote::DeltaType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxDeltaType {
    /// `phi P_f N(phi d1)`.
    Spot,
    /// `phi N(phi d1)`.
    Forward,
    /// The spot delta less the premium in foreign currency: `phi P_f N(phi d2) K / F`.
    PremiumAdjustedSpot,
    /// `phi N(phi d2) K / F`.
    PremiumAdjustedForward,
}

impl FxDeltaType {
    fn premium_adjusted(self) -> bool {
        matches!(
            self,
            FxDeltaType::PremiumAdjustedSpot | FxDeltaType::PremiumAdjustedForward
        )
    }
}

/// Where the ATM strike is (QuantLib `DeltaVolQuote::AtmType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxAtmType {
    /// The spot.
    Spot,
    /// The forward.
    Forward,
    /// Where a call and a put have deltas of the same size: `F exp(+v/2)`, or `F exp(-v/2)` for a
    /// premium-adjusted delta, with `v` the variance.
    DeltaNeutral,
    /// Where the forward deltas are 0.5 and -0.5 (`F exp(v/2)`); forward delta only.
    PutCall50,
}

/// The strikes and deltas of an option on the FX rate (QuantLib `BlackDeltaCalculator`): domestic per
/// foreign, with the discount factors to the delivery and the standard deviation `sigma sqrt(T)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FxDeltaCalculator {
    pub option: OptionType,
    pub delta_type: FxDeltaType,
    pub spot: f64,
    pub domestic_discount: f64,
    pub foreign_discount: f64,
    pub std_dev: f64,
}

impl FxDeltaCalculator {
    fn check(&self) -> Result<(), Error> {
        if !(self.spot > 0.0) || !(self.domestic_discount > 0.0) || !(self.foreign_discount > 0.0) {
            return invalid("the spot and the discount factors must be positive".into());
        }
        if !(self.std_dev >= 0.0) || !self.std_dev.is_finite() {
            return invalid(format!(
                "the standard deviation must be finite and non-negative, not {}",
                self.std_dev
            ));
        }
        Ok(())
    }

    fn phi(&self) -> f64 {
        match self.option {
            OptionType::Call => 1.0,
            OptionType::Put => -1.0,
        }
    }

    /// `S P_f / P_d`.
    pub fn forward(&self) -> f64 {
        self.spot * self.foreign_discount / self.domestic_discount
    }

    /// `N(phi d)` with `d = ln(F / K) / s + shift s` (`shift` 0.5 for `d1`, -0.5 for `d2`), and its
    /// limits at a standard deviation of 0.
    fn cumulative(&self, strike: f64, shift: f64) -> f64 {
        let (phi, s, f) = (self.phi(), self.std_dev, self.forward());
        if s >= f64::EPSILON {
            if strike > 0.0 {
                return compute_normal_cdf(phi * ((f / strike).ln() / s + shift * s));
            }
        } else if f == strike {
            return compute_normal_cdf(phi * shift * s);
        }
        let below = f < strike;
        // N(d) is 1 and N(-d) 0 unless the forward is below the strike
        let (positive, negative) = if s >= f64::EPSILON || !below {
            (1.0, 0.0)
        } else {
            (0.0, 1.0)
        };
        if phi > 0.0 {
            positive
        } else {
            negative
        }
    }

    /// `n(d2)`.
    fn density_d2(&self, strike: f64) -> f64 {
        let s = self.std_dev;
        if s >= f64::EPSILON && strike > 0.0 {
            compute_normal_pdf((self.forward() / strike).ln() / s - 0.5 * s)
        } else {
            0.0
        }
    }

    /// The delta of the strike.
    pub fn delta_from_strike(&self, strike: f64) -> Result<f64, Error> {
        self.check()?;
        if !(strike >= 0.0) {
            return invalid(format!("the strike must be non-negative, not {strike}"));
        }
        let (phi, pf, f) = (self.phi(), self.foreign_discount, self.forward());
        Ok(match self.delta_type {
            FxDeltaType::Spot => phi * pf * self.cumulative(strike, 0.5),
            FxDeltaType::Forward => phi * self.cumulative(strike, 0.5),
            FxDeltaType::PremiumAdjustedSpot => {
                phi * pf * self.cumulative(strike, -0.5) * strike / f
            }
            FxDeltaType::PremiumAdjustedForward => phi * self.cumulative(strike, -0.5) * strike / f,
        })
    }

    /// The strike of the delta (positive for a call, negative for a put). A premium-adjusted delta is
    /// solved by Brent's method to 1e-10, as QuantLib; for a call, between the strike of the largest
    /// premium-adjusted delta and that of the delta without the premium.
    pub fn strike_from_delta(&self, delta: f64) -> Result<f64, Error> {
        self.check()?;
        self.strike_from_delta_of(delta, self.delta_type)
    }

    fn strike_from_delta_of(&self, delta: f64, delta_type: FxDeltaType) -> Result<f64, Error> {
        let (phi, s, f, pf) = (
            self.phi(),
            self.std_dev,
            self.forward(),
            self.foreign_discount,
        );
        if !(delta * phi >= 0.0) {
            return invalid(format!(
                "the option type and the delta {delta} are incoherent"
            ));
        }
        match delta_type {
            FxDeltaType::Spot => {
                if !(delta.abs() <= pf) {
                    return invalid(format!("the spot delta {delta} is out of range"));
                }
                Ok(
                    f * (-phi * compute_inverse_normal_cdf(phi * delta / pf) * s + 0.5 * s * s)
                        .exp(),
                )
            }
            FxDeltaType::Forward => {
                if !(delta.abs() <= 1.0) {
                    return invalid(format!("the forward delta {delta} is out of range"));
                }
                Ok(f * (-phi * compute_inverse_normal_cdf(phi * delta) * s + 0.5 * s * s).exp())
            }
            FxDeltaType::PremiumAdjustedSpot | FxDeltaType::PremiumAdjustedForward => {
                let calculator = FxDeltaCalculator {
                    delta_type,
                    ..*self
                };
                let objective = |k: f64| Ok(calculator.delta_from_strike(k)? - delta);
                let accuracy = 1.0e-10;
                let unadjusted = if delta_type == FxDeltaType::PremiumAdjustedSpot {
                    FxDeltaType::Spot
                } else {
                    FxDeltaType::Forward
                };
                let right = self.strike_from_delta_of(delta, unadjusted)?;
                if phi < 0.0 {
                    solve_brent(objective, accuracy, right, 0.0, self.spot * 100.0, 1000)
                } else {
                    // the premium-adjusted call delta is largest where N(d2) s = n(d2)
                    let peak =
                        |k: f64| Ok(calculator.cumulative(k, -0.5) * s - calculator.density_d2(k));
                    let left = solve_brent(peak, accuracy, right * 0.5, 0.0, right, 1000)?;
                    solve_brent(
                        objective,
                        accuracy,
                        left + (right - left) * 0.5,
                        left,
                        right,
                        1000,
                    )
                }
            }
        }
    }

    /// The ATM strike. The delta-neutral strike is `F exp(v/2)` for a delta without the premium and
    /// `F exp(-v/2)` with it; `PutCall50` needs the forward delta.
    pub fn atm_strike(&self, atm: FxAtmType) -> Result<f64, Error> {
        self.check()?;
        let (f, s) = (self.forward(), self.std_dev);
        Ok(match atm {
            FxAtmType::Spot => self.spot,
            FxAtmType::Forward => f,
            FxAtmType::DeltaNeutral if self.delta_type.premium_adjusted() => {
                f * (-0.5 * s * s).exp()
            }
            FxAtmType::DeltaNeutral => f * (0.5 * s * s).exp(),
            FxAtmType::PutCall50 => {
                if self.delta_type != FxDeltaType::Forward {
                    return invalid(
                        "put and call deltas of 0.5 are possible only for the forward delta".into(),
                    );
                }
                f * (0.5 * s * s).exp()
            }
        })
    }
}

/// The conventions of the deltas of a smile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FxSmileConventions {
    pub delta_type: FxDeltaType,
    pub atm_type: FxAtmType,
    /// The delta type of the ATM strike (QuantLib's default is `delta_type`).
    pub atm_delta_type: FxDeltaType,
}

/// How a smile is interpolated in strike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmileInterpolation {
    Linear,
    /// The natural cubic spline (QuantLib `SmileInterpolationMethod::CubicSpline`).
    CubicSpline,
}

/// Black volatilities from volatilities at deltas by expiry (QuantLib `BlackVolatilitySurfaceDelta`,
/// with the volatility flat in time after the last expiry). For a time `t`, each delta's (or ATM's)
/// variance `sigma^2 T` is interpolated linearly in time (from 0 at time 0), each delta is turned into
/// its strike with its own volatility and the discount factors to `t`, and the smile is interpolated in
/// strike (a strike close to another one already placed is skipped). After the last expiry the smile of
/// the last expiry is used. `volatilities[i]` holds expiry `i`'s volatilities at `put_deltas` (negative),
/// ATM if `has_atm`, then `call_deltas`.
#[derive(Debug, Clone)]
pub struct FxSmileSurface<'c> {
    /// Domestic per foreign.
    pub spot: f64,
    pub domestic: &'c YieldCurve,
    pub foreign: &'c YieldCurve,
    /// In years, positive and increasing.
    pub expiries: Vec<f64>,
    pub put_deltas: Vec<f64>,
    pub call_deltas: Vec<f64>,
    pub has_atm: bool,
    pub volatilities: Vec<Vec<f64>>,
    pub conventions: FxSmileConventions,
    /// The conventions from a time on (QuantLib's switch tenor): spot deltas up to a year and forward
    /// deltas after, for example. At the switch time itself the long-term conventions apply.
    pub long_term: Option<(f64, FxSmileConventions)>,
    pub interpolation: SmileInterpolation,
    /// Below the lowest and above the highest strike, the volatility there; otherwise the
    /// interpolation's own extension.
    pub flat_strike_extrapolation: bool,
}

/// A smile at one time: its strikes, in increasing order, and volatilities.
#[derive(Debug, Clone, PartialEq)]
pub struct FxSmile {
    pub time: f64,
    pub strikes: Vec<f64>,
    pub volatilities: Vec<f64>,
}

impl FxSmileSurface<'_> {
    pub fn check(&self) -> Result<(), Error> {
        if !(self.spot > 0.0) || !self.spot.is_finite() {
            return invalid("the spot must be positive and finite".into());
        }
        let pillars = self.put_deltas.len() + self.call_deltas.len() + usize::from(self.has_atm);
        if pillars == 0 {
            return invalid("a smile needs at least one delta or ATM".into());
        }
        if self.expiries.is_empty()
            || !(self.expiries[0] > 0.0)
            || self.expiries.windows(2).any(|w| !(w[1] > w[0]))
        {
            return invalid("the expiries must be positive and increasing".into());
        }
        if self.volatilities.len() != self.expiries.len()
            || self.volatilities.iter().any(|row| row.len() != pillars)
        {
            return invalid(format!(
                "give {} volatilities (puts, ATM, calls) for each of the {} expiries",
                pillars,
                self.expiries.len()
            ));
        }
        if !self
            .volatilities
            .iter()
            .flatten()
            .all(|v| *v >= 0.0 && v.is_finite())
        {
            return invalid("the volatilities must be finite and non-negative".into());
        }
        if !self.put_deltas.iter().all(|d| *d < 0.0 && *d > -1.0)
            || !self.call_deltas.iter().all(|d| *d > 0.0 && *d < 1.0)
        {
            return invalid("put deltas must be in (-1, 0) and call deltas in (0, 1)".into());
        }
        if let Some((switch, _)) = self.long_term {
            if !(switch > 0.0) {
                return invalid(format!("the switch time must be positive, not {switch}"));
            }
        }
        Ok(())
    }

    /// The volatility of pillar `j` at time `t`: its variance interpolated linearly in time from 0 at
    /// time 0 (QuantLib `BlackVarianceCurve`).
    fn pillar_volatility(&self, t: f64, j: usize) -> Result<f64, Error> {
        let mut times = Vec::with_capacity(self.expiries.len() + 1);
        let mut variances = Vec::with_capacity(self.expiries.len() + 1);
        times.push(0.0);
        variances.push(0.0);
        for (i, &e) in self.expiries.iter().enumerate() {
            times.push(e);
            variances.push(e * self.volatilities[i][j] * self.volatilities[i][j]);
        }
        let variance = Linear::new(times, variances)?.value(t).max(0.0);
        Ok((variance / t).sqrt())
    }

    /// The smile at time `t` (after the last expiry, at the last expiry).
    pub fn smile_at(&self, t: f64) -> Result<FxSmile, Error> {
        self.check()?;
        if !(t > 0.0) || !t.is_finite() {
            return invalid(format!("the time must be positive and finite, not {t}"));
        }
        let t = t.min(*self.expiries.last().expect("checked"));
        let (pd, pf) = (
            self.domestic.discount_at(t, true)?,
            self.foreign.discount_at(t, true)?,
        );
        let sqrt_t = t.sqrt();
        let conventions = match self.long_term {
            Some((switch, long)) if !(t < switch && !is_close_enough(t, switch)) => long,
            _ => self.conventions,
        };
        let calculator =
            |option: OptionType, delta_type: FxDeltaType, vol: f64| FxDeltaCalculator {
                option,
                delta_type,
                spot: self.spot,
                domestic_discount: pd,
                foreign_discount: pf,
                std_dev: vol * sqrt_t,
            };
        // strikes in the order of the pillars, a strike close to one already placed skipped
        let mut points: Vec<(f64, f64)> = Vec::new();
        let mut place = |strike: f64, vol: f64| {
            if !points.iter().any(|&(k, _)| is_close(k, strike)) {
                points.push((strike, vol));
            }
        };
        let mut j = 0;
        for &delta in &self.put_deltas {
            let vol = self.pillar_volatility(t, j)?;
            place(
                calculator(OptionType::Put, conventions.delta_type, vol)
                    .strike_from_delta(delta)?,
                vol,
            );
            j += 1;
        }
        if self.has_atm {
            let vol = self.pillar_volatility(t, j)?;
            place(
                calculator(OptionType::Put, conventions.atm_delta_type, vol)
                    .atm_strike(conventions.atm_type)?,
                vol,
            );
            j += 1;
        }
        for &delta in &self.call_deltas {
            let vol = self.pillar_volatility(t, j)?;
            place(
                calculator(OptionType::Call, conventions.delta_type, vol)
                    .strike_from_delta(delta)?,
                vol,
            );
            j += 1;
        }
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        // the smile section keeps standard deviations and divides them back by sqrt(t)
        Ok(FxSmile {
            time: t,
            strikes: points.iter().map(|p| p.0).collect(),
            volatilities: points.iter().map(|p| p.1 * sqrt_t / sqrt_t).collect(),
        })
    }

    /// The Black volatility at time `t` and `strike`.
    pub fn volatility(&self, t: f64, strike: f64) -> Result<f64, Error> {
        if !(strike > 0.0) || !strike.is_finite() {
            return invalid(format!(
                "the strike must be positive and finite, not {strike}"
            ));
        }
        let smile = self.smile_at(t)?;
        smile.volatility(strike, self.interpolation, self.flat_strike_extrapolation)
    }

    /// The Black variance `sigma^2 t` at time `t` and `strike`.
    pub fn variance(&self, t: f64, strike: f64) -> Result<f64, Error> {
        let v = self.volatility(t, strike)?;
        Ok(v * v * t)
    }
}

impl FxSmile {
    /// The volatility at `strike`, interpolated (QuantLib `InterpolatedSmileSection`, or a flat smile for
    /// one strike), at least 0.
    pub fn volatility(
        &self,
        strike: f64,
        interpolation: SmileInterpolation,
        flat_extrapolation: bool,
    ) -> Result<f64, Error> {
        if self.strikes.len() == 1 {
            return Ok(self.volatilities[0]);
        }
        let n = self.strikes.len();
        let k = if flat_extrapolation {
            strike.clamp(self.strikes[0], self.strikes[n - 1])
        } else {
            strike
        };
        let v = match interpolation {
            SmileInterpolation::Linear => {
                Linear::new(self.strikes.clone(), self.volatilities.clone())?.value(k)
            }
            SmileInterpolation::CubicSpline => {
                MonotonicCubicSpline::natural(self.strikes.clone(), self.volatilities.clone())?
                    .value(k)
            }
        };
        Ok(v.max(0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curves::YieldCurve;
    use crate::interest_rate::{Compounding, InterestRate};
    use crate::time::{Actual365Kind, Date, DayCount};

    fn flat(rate: f64) -> YieldCurve {
        let dc = DayCount::Actual365Fixed(Actual365Kind::Standard);
        YieldCurve::flat(
            Date::from_ymd(2026, 11, 2).unwrap(),
            InterestRate::new(rate, dc, Compounding::Continuous, 0).unwrap(),
        )
    }

    #[test]
    fn deltas_and_strikes_invert_each_other() {
        for delta_type in [
            FxDeltaType::Spot,
            FxDeltaType::Forward,
            FxDeltaType::PremiumAdjustedSpot,
            FxDeltaType::PremiumAdjustedForward,
        ] {
            for (option, delta) in [
                (OptionType::Call, 0.25),
                (OptionType::Put, -0.25),
                (OptionType::Call, 0.1),
                (OptionType::Put, -0.4),
            ] {
                let c = FxDeltaCalculator {
                    option,
                    delta_type,
                    spot: 150.0,
                    domestic_discount: 0.99,
                    foreign_discount: 0.96,
                    std_dev: 0.12,
                };
                let k = c.strike_from_delta(delta).unwrap();
                assert!(
                    (c.delta_from_strike(k).unwrap() - delta).abs() < 1e-9,
                    "{delta_type:?} {option:?} {delta}"
                );
            }
        }
        let c = FxDeltaCalculator {
            option: OptionType::Call,
            delta_type: FxDeltaType::Spot,
            spot: 150.0,
            domestic_discount: 0.99,
            foreign_discount: 0.96,
            std_dev: 0.12,
        };
        assert!(
            c.strike_from_delta(-0.25).is_err()
                && c.strike_from_delta(0.97).is_err()
                && c.atm_strike(FxAtmType::PutCall50).is_err()
        );
    }

    #[test]
    fn a_flat_smile_is_its_volatility_and_the_conventions_switch() {
        let (d, f) = (flat(0.012), flat(0.035));
        let conventions = FxSmileConventions {
            delta_type: FxDeltaType::Spot,
            atm_type: FxAtmType::DeltaNeutral,
            atm_delta_type: FxDeltaType::Spot,
        };
        let surface = FxSmileSurface {
            spot: 150.0,
            domestic: &d,
            foreign: &f,
            expiries: vec![0.5, 1.0, 2.0],
            put_deltas: vec![-0.1, -0.25],
            call_deltas: vec![0.25, 0.1],
            has_atm: true,
            volatilities: vec![vec![0.11; 5], vec![0.12; 5], vec![0.12; 5]],
            conventions,
            long_term: None,
            interpolation: SmileInterpolation::CubicSpline,
            flat_strike_extrapolation: false,
        };
        for (t, v) in [(0.3, 0.11), (0.5, 0.11), (2.0, 0.12), (5.0, 0.12)] {
            for k in [100.0, 150.0, 220.0] {
                assert!((surface.volatility(t, k).unwrap() - v).abs() < 1e-14);
            }
        }
        // between expiries, the variance is linear in time
        let w = (0.11f64.powi(2) * 0.5 * 0.5 + 0.12f64.powi(2) * 1.0 * 0.5) / 0.75;
        assert!((surface.volatility(0.75, 150.0).unwrap() - w.sqrt()).abs() < 1e-14);
        // the long-term conventions from the switch on
        let long = FxSmileConventions {
            delta_type: FxDeltaType::PremiumAdjustedForward,
            atm_type: FxAtmType::DeltaNeutral,
            atm_delta_type: FxDeltaType::PremiumAdjustedForward,
        };
        let switched = FxSmileSurface {
            long_term: Some((1.0, long)),
            volatilities: vec![vec![0.1, 0.11, 0.12, 0.13, 0.14]; 3],
            ..surface.clone()
        };
        let before = FxSmileSurface {
            volatilities: switched.volatilities.clone(),
            ..surface.clone()
        };
        let after = FxSmileSurface {
            conventions: long,
            volatilities: switched.volatilities.clone(),
            ..surface.clone()
        };
        assert_eq!(
            switched.smile_at(0.9).unwrap(),
            before.smile_at(0.9).unwrap()
        );
        assert_eq!(
            switched.smile_at(1.0).unwrap(),
            after.smile_at(1.0).unwrap()
        );
        assert_ne!(before.smile_at(1.0).unwrap(), after.smile_at(1.0).unwrap());
        // invalid surfaces
        assert!(FxSmileSurface {
            volatilities: vec![vec![0.1; 4]; 3],
            ..surface.clone()
        }
        .volatility(1.0, 150.0)
        .is_err());
        assert!(FxSmileSurface {
            expiries: vec![1.0, 0.5, 2.0],
            ..surface.clone()
        }
        .volatility(1.0, 150.0)
        .is_err());
        assert!(FxSmileSurface {
            put_deltas: vec![0.1, -0.25],
            ..surface.clone()
        }
        .volatility(1.0, 150.0)
        .is_err());
        assert!(surface.volatility(0.0, 150.0).is_err() && surface.volatility(1.0, -1.0).is_err());
    }
}
