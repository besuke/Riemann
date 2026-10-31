//! Interest rates with a compounding rule (QuantLib-compatible `InterestRate`, v1.42).
//!
//! Derived from QuantLib `ql/interestrate.cpp` at v1.42 (Copyright (C) 2004 Ferdinando
//! Ametrano; modified BSD license). See THIRD_PARTY_NOTICES.
//!
//! Only Simple, Compounded and Continuous compounding are implemented; QuantLib changed the
//! mixed rules (SimpleThenCompounded, CompoundedThenSimple) after v1.42.

use crate::time::DayCount;
use crate::Error;

/// How interest compounds over time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Compounding {
    /// 1 + r t
    Simple,
    /// (1 + r / f)^(f t)
    Compounded,
    /// exp(r t)
    Continuous,
}

/// A rate with its day count, compounding and compounding frequency (periods per year).
#[derive(Debug, Clone, PartialEq)]
pub struct InterestRate {
    pub rate: f64,
    pub day_count: DayCount,
    pub compounding: Compounding,
    pub frequency: u32,
}

impl InterestRate {
    pub fn new(rate: f64, day_count: DayCount, compounding: Compounding, frequency: u32) -> Result<Self, Error> {
        if compounding == Compounding::Compounded && frequency == 0 {
            return Err(Error::InvalidInput { reason: "compounded rate needs a frequency of at least 1".into() });
        }
        Ok(InterestRate { rate, day_count, compounding, frequency })
    }

    /// Growth of 1 over `t` years.
    pub fn compound_factor(&self, t: f64) -> Result<f64, Error> {
        if t < 0.0 {
            return Err(Error::InvalidInput { reason: format!("negative time ({t}) not allowed") });
        }
        let r = self.rate;
        let f = self.frequency as f64;
        Ok(match self.compounding {
            Compounding::Simple => 1.0 + r * t,
            Compounding::Compounded => (1.0 + r / f).powf(f * t),
            Compounding::Continuous => (r * t).exp(),
        })
    }

    /// Present value of 1 paid in `t` years.
    pub fn discount_factor(&self, t: f64) -> Result<f64, Error> {
        Ok(1.0 / self.compound_factor(t)?)
    }

    /// The rate that grows 1 to `compound` over `t` years (QuantLib `impliedRate`).
    /// The frequency is ignored unless the compounding is `Compounded`.
    pub fn implied_rate(
        compound: f64,
        day_count: DayCount,
        compounding: Compounding,
        frequency: u32,
        t: f64,
    ) -> Result<InterestRate, Error> {
        if compound <= 0.0 {
            return Err(Error::InvalidInput {
                reason: "positive compound factor required".into(),
            });
        }
        let rate = if compound == 1.0 {
            if t < 0.0 {
                return Err(Error::InvalidInput {
                    reason: format!("non negative time ({t}) required"),
                });
            }
            0.0
        } else {
            if t <= 0.0 {
                return Err(Error::InvalidInput {
                    reason: format!("positive time ({t}) required"),
                });
            }
            let f = frequency as f64;
            match compounding {
                Compounding::Simple => (compound - 1.0) / t,
                Compounding::Compounded => (compound.powf(1.0 / (f * t)) - 1.0) * f,
                Compounding::Continuous => compound.ln() / t,
            }
        };
        InterestRate::new(rate, day_count, compounding, frequency)
    }

    /// The rate with another compounding that gives the same growth over `t` years
    /// (QuantLib `equivalentRate(comp, freq, t)`).
    pub fn equivalent_rate(
        &self,
        compounding: Compounding,
        frequency: u32,
        t: f64,
    ) -> Result<InterestRate, Error> {
        InterestRate::implied_rate(
            self.compound_factor(t)?,
            self.day_count.clone(),
            compounding,
            frequency,
            t,
        )
    }
}
