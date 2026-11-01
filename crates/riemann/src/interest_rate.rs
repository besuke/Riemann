//! Interest rates with a compounding rule (QuantLib-compatible `InterestRate`, v1.42).
//!
//! Derived from QuantLib `ql/interestrate.cpp` at v1.42 (Copyright (C) 2004 Ferdinando
//! Ametrano; modified BSD license). See THIRD_PARTY_NOTICES.
//!
//! Only Simple, Compounded and Continuous compounding are implemented; QuantLib changed the
//! mixed rules (SimpleThenCompounded, CompoundedThenSimple) after v1.42.

use crate::autodiff::Operand;
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
pub struct InterestRate<T = f64> {
    pub rate: T,
    pub day_count: DayCount,
    pub compounding: Compounding,
    pub frequency: u32,
}

impl<T: Operand> InterestRate<T> {
    pub fn new(rate: T, day_count: DayCount, compounding: Compounding, frequency: u32) -> Result<Self, Error> {
        if compounding == Compounding::Compounded && frequency == 0 {
            return Err(Error::InvalidInput { reason: "compounded rate needs a frequency of at least 1".into() });
        }
        Ok(InterestRate { rate, day_count, compounding, frequency })
    }

    /// Growth of 1 over `t` years.
    pub fn compound_factor(&self, t: f64) -> Result<T, Error> {
        if t < 0.0 {
            return Err(Error::InvalidInput { reason: format!("negative time ({t}) not allowed") });
        }
        let r = self.rate;
        let f = self.frequency as f64;
        Ok(match self.compounding {
            Compounding::Simple => r * t + 1.0,
            Compounding::Compounded => (r / f + 1.0).powf(f * t),
            Compounding::Continuous => (r * t).exp(),
        })
    }

    /// Present value of 1 paid in `t` years.
    pub fn discount_factor(&self, t: f64) -> Result<T, Error> {
        Ok(self.compound_factor(t)?.recip())
    }

    /// The rate that grows 1 to `compound` over `t` years (QuantLib `impliedRate`).
    /// The frequency is ignored unless the compounding is `Compounded`.
    pub fn implied_rate(
        compound: T,
        day_count: DayCount,
        compounding: Compounding,
        frequency: u32,
        t: f64,
    ) -> Result<InterestRate<T>, Error> {
        if compound.primal() <= 0.0 {
            return Err(Error::InvalidInput {
                reason: "positive compound factor required".into(),
            });
        }
        let rate = if compound.primal() == 1.0 {
            if t < 0.0 {
                return Err(Error::InvalidInput {
                    reason: format!("non negative time ({t}) required"),
                });
            }
            compound.like(0.0)
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
    ) -> Result<InterestRate<T>, Error> {
        InterestRate::implied_rate(
            self.compound_factor(t)?,
            self.day_count.clone(),
            compounding,
            frequency,
            t,
        )
    }
}
