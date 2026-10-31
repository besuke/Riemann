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
}
