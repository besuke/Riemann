//! Yield curves: flat, linear on zero rates, log-linear on discount factors and a zero spread
//! over another curve. A curve is a value; discount factors and zero rates are functions of it.
//!
//! Reproduces QuantLib v1.42 (the version used for verification):
//! `ql/termstructure.cpp` (Copyright (C) 2004, 2005, 2006, 2007 StatPro Italia srl),
//! `ql/termstructures/yieldtermstructure.cpp` (Copyright (C) 2004, 2009 Ferdinando Ametrano;
//! (C) 2000, 2001, 2002, 2003 RiskMap srl; (C) 2003, 2004, 2005, 2006 StatPro Italia srl),
//! `ql/termstructures/yield/zeroyieldstructure.hpp` (Copyright (C) 2000, 2001, 2002, 2003
//! RiskMap srl; (C) 2003, 2004 StatPro Italia srl; (C) 2009 Ferdinando Ametrano),
//! `ql/termstructures/yield/flatforward.hpp` (Copyright (C) 2000, 2001, 2002, 2003 RiskMap
//! srl; (C) 2003, 2004, 2005, 2007 StatPro Italia srl),
//! `ql/termstructures/interpolatedcurve.hpp` (Copyright (C) 2009 StatPro Italia srl),
//! `ql/termstructures/yield/zerocurve.hpp` (Copyright (C) 2003, 2004, 2005, 2006, 2007, 2008
//! StatPro Italia srl; (C) 2009, 2015 Ferdinando Ametrano; (C) 2015 Paolo Mazzocchi),
//! `ql/termstructures/yield/discountcurve.hpp` (Copyright (C) 2002, 2003 Decillion Pty(Ltd);
//! (C) 2005, 2006, 2008, 2009 StatPro Italia srl; (C) 2009, 2015 Ferdinando Ametrano;
//! (C) 2015 Paolo Mazzocchi), `ql/termstructures/yield/zerospreadedtermstructure.hpp`
//! (Copyright (C) 2000, 2001, 2002, 2003 RiskMap srl; (C) 2007, 2008 StatPro Italia srl) and
//! `ql/math/interpolations/loginterpolation.hpp` (Copyright (C) 2002, 2003, 2008, 2009
//! Ferdinando Ametrano; (C) 2004, 2007, 2008 StatPro Italia srl); modified BSD license.
//! See THIRD_PARTY_NOTICES.
//!
//! Differences from QuantLib: no observers, handles, quotes or jumps; the spread is a number
//! and the base curve is held by value. Extrapolation is a field of the curve.

use crate::interest_rate::{Compounding, InterestRate};
use crate::math::interpolation::Linear;
use crate::math::solver1d::close;
use crate::math::solver1d::close_enough;
use crate::time::{Date, DayCount};
use crate::Error;

/// Time step used for the zero rate at the reference date (QuantLib `dt`).
const DT: f64 = 0.0001;

/// A yield curve on a reference date.
#[derive(Debug, Clone, PartialEq)]
pub struct YieldCurve {
    reference_date: Date,
    day_count: DayCount,
    /// Whether discount factors beyond the last date are allowed.
    pub extrapolate: bool,
    shape: Shape,
}

#[derive(Debug, Clone, PartialEq)]
enum Shape {
    /// QuantLib `FlatForward` with a fixed rate.
    Flat { rate: InterestRate },
    /// QuantLib `ZeroCurve` (`InterpolatedZeroCurve<Linear>`): continuous zero rates.
    Zero { dates: Vec<Date>, zeros: Linear },
    /// QuantLib `DiscountCurve` (`InterpolatedDiscountCurve<LogLinear>`): linear on log discounts.
    Discount {
        dates: Vec<Date>,
        discounts: Vec<f64>,
        log_discounts: Linear,
    },
    /// QuantLib `ZeroSpreadedTermStructure`: a spread added to the base zero rate.
    ZeroSpreaded {
        base: Box<YieldCurve>,
        spread: f64,
        compounding: Compounding,
        frequency: u32,
    },
}

/// Times from `dates[0]` (QuantLib `InterpolatedCurve::setupTimes`).
fn curve_times(dates: &[Date], day_count: &DayCount) -> Result<Vec<f64>, Error> {
    let mut times = Vec::with_capacity(dates.len());
    times.push(day_count.year_fraction(dates[0], dates[0], None, None)?);
    for i in 1..dates.len() {
        if dates[i] <= dates[i - 1] {
            return Err(Error::InvalidInput {
                reason: format!(
                    "dates not sorted: {} passed after {}",
                    dates[i],
                    dates[i - 1]
                ),
            });
        }
        let t = day_count.year_fraction(dates[0], dates[i], None, None)?;
        if close(t, times[i - 1]) {
            return Err(Error::InvalidInput {
                reason: format!(
                    "two passed dates ({} and {}) correspond to the same time",
                    dates[i - 1],
                    dates[i]
                ),
            });
        }
        times.push(t);
    }
    Ok(times)
}

fn check_points(dates: &[Date], values: &[f64]) -> Result<(), Error> {
    if dates.len() < 2 {
        return Err(Error::InvalidInput {
            reason: "not enough input dates given".into(),
        });
    }
    if dates.len() != values.len() {
        return Err(Error::InvalidInput {
            reason: "dates/data count mismatch".into(),
        });
    }
    Ok(())
}

impl YieldCurve {
    /// A flat curve at `rate` from `reference_date` (QuantLib `FlatForward`); its day count is the rate's.
    pub fn flat(reference_date: Date, rate: InterestRate) -> YieldCurve {
        YieldCurve {
            reference_date,
            day_count: rate.day_count.clone(),
            extrapolate: false,
            shape: Shape::Flat { rate },
        }
    }

    /// Linear interpolation of zero rates given in `compounding` (QuantLib `ZeroCurve`).
    /// The first date is the reference date. Rates that are not continuous are converted to
    /// continuous rates at each node, the first one over about one day (1/365).
    pub fn zero(
        dates: Vec<Date>,
        rates: Vec<f64>,
        day_count: DayCount,
        compounding: Compounding,
        frequency: u32,
    ) -> Result<YieldCurve, Error> {
        check_points(&dates, &rates)?;
        let times = curve_times(&dates, &day_count)?;
        let mut zeros = rates;
        if compounding != Compounding::Continuous {
            for i in 0..zeros.len() {
                let t = if i == 0 { 1.0 / 365.0 } else { times[i] };
                let r = InterestRate::new(zeros[i], day_count.clone(), compounding, frequency)?;
                zeros[i] = r.equivalent_rate(Compounding::Continuous, 0, t)?.rate;
            }
        }
        Ok(YieldCurve {
            reference_date: dates[0],
            day_count,
            extrapolate: false,
            shape: Shape::Zero {
                zeros: Linear::new(times, zeros)?,
                dates,
            },
        })
    }

    /// Log-linear interpolation of discount factors (QuantLib `DiscountCurve`). The first
    /// date is the reference date and its discount factor must be 1.
    pub fn discount_factors(
        dates: Vec<Date>,
        discounts: Vec<f64>,
        day_count: DayCount,
    ) -> Result<YieldCurve, Error> {
        check_points(&dates, &discounts)?;
        if discounts[0] != 1.0 {
            return Err(Error::InvalidInput {
                reason: "the first discount must be == 1.0 to flag the corresponding date as reference date".into(),
            });
        }
        if discounts[1..].iter().any(|&d| d <= 0.0) {
            return Err(Error::InvalidInput {
                reason: "negative discount".into(),
            });
        }
        let times = curve_times(&dates, &day_count)?;
        let logs = discounts.iter().map(|d| d.ln()).collect();
        Ok(YieldCurve {
            reference_date: dates[0],
            day_count,
            extrapolate: false,
            shape: Shape::Discount {
                log_discounts: Linear::new(times, logs)?,
                discounts,
                dates,
            },
        })
    }

    /// `spread` added to the zero rates of `base` in the given compounding (QuantLib
    /// `ZeroSpreadedTermStructure`). Reference date, day count and last date are the base's;
    /// the base is always read with extrapolation.
    pub fn zero_spreaded(
        base: YieldCurve,
        spread: f64,
        compounding: Compounding,
        frequency: u32,
    ) -> Result<YieldCurve, Error> {
        if compounding == Compounding::Compounded && frequency == 0 {
            return Err(Error::InvalidInput {
                reason: "compounded rate needs a frequency of at least 1".into(),
            });
        }
        Ok(YieldCurve {
            reference_date: base.reference_date,
            day_count: base.day_count.clone(),
            extrapolate: false,
            shape: Shape::ZeroSpreaded {
                base: Box::new(base),
                spread,
                compounding,
                frequency,
            },
        })
    }

    /// The same curve with extrapolation allowed or not.
    pub fn with_extrapolation(mut self, extrapolate: bool) -> YieldCurve {
        self.extrapolate = extrapolate;
        self
    }

    pub fn reference_date(&self) -> Date {
        self.reference_date
    }

    pub fn day_count(&self) -> &DayCount {
        &self.day_count
    }

    /// Last date of the curve (the largest date for a flat curve).
    pub fn max_date(&self) -> Date {
        match &self.shape {
            Shape::Flat { .. } => Date::MAX,
            Shape::Zero { dates, .. } | Shape::Discount { dates, .. } => {
                *dates.last().expect("at least 2 dates")
            }
            Shape::ZeroSpreaded { base, .. } => base.max_date(),
        }
    }

    pub fn max_time(&self) -> Result<f64, Error> {
        self.time_from_reference(self.max_date())
    }

    /// Year fraction from the reference date in the curve's day count.
    pub fn time_from_reference(&self, d: Date) -> Result<f64, Error> {
        self.day_count
            .year_fraction(self.reference_date, d, None, None)
    }

    /// QuantLib `TermStructure::checkRange(t, extrapolate)`.
    fn check_range(&self, t: f64, extrapolate: bool) -> Result<(), Error> {
        if t < 0.0 {
            return Err(Error::InvalidInput {
                reason: format!("negative time ({t}) given"),
            });
        }
        if !(extrapolate || self.extrapolate) {
            let max_time = self.max_time()?;
            if !(t <= max_time || close_enough(t, max_time)) {
                return Err(Error::OutsideCurveRange {
                    reason: format!("time ({t}) is past max curve time ({max_time})"),
                });
            }
        }
        Ok(())
    }

    /// Discount factor for `d`.
    pub fn discount(&self, d: Date) -> Result<f64, Error> {
        self.discount_at(self.time_from_reference(d)?, false)
    }

    /// Discount factor at time `t`; `extrapolate` allows times beyond the curve for this call.
    pub fn discount_at(&self, t: f64, extrapolate: bool) -> Result<f64, Error> {
        self.check_range(t, extrapolate)?;
        self.discount_impl(t)
    }

    fn discount_impl(&self, t: f64) -> Result<f64, Error> {
        match &self.shape {
            Shape::Flat { rate } => rate.discount_factor(t),
            Shape::Discount {
                discounts,
                log_discounts,
                ..
            } => {
                let t_max = *log_discounts.xs().last().expect("at least 2 points");
                if t <= t_max {
                    return Ok(log_discounts.value(t).exp());
                }
                // flat forward extrapolation
                let d_max = *discounts.last().expect("at least 2 points");
                let derivative = log_discounts.value(t_max).exp() * log_discounts.derivative(t_max);
                let inst_fwd_max = -derivative / d_max;
                Ok(d_max * (-inst_fwd_max * (t - t_max)).exp())
            }
            Shape::Zero { .. } | Shape::ZeroSpreaded { .. } => {
                // this acts as a safe guard in cases where the zero yield at 0 would fail
                if t == 0.0 {
                    return Ok(1.0);
                }
                Ok((-self.zero_yield(t)? * t).exp())
            }
        }
    }

    /// Continuous zero yield at `t` > 0 of a curve defined by zero rates (QuantLib `zeroYieldImpl`).
    fn zero_yield(&self, t: f64) -> Result<f64, Error> {
        match &self.shape {
            Shape::Zero { zeros, .. } => {
                let t_max = *zeros.xs().last().expect("at least 2 points");
                if t <= t_max {
                    return Ok(zeros.value(t));
                }
                // flat forward extrapolation
                let z_max = *zeros.ys().last().expect("at least 2 points");
                let inst_fwd_max = z_max + t_max * zeros.derivative(t_max);
                Ok((z_max * t_max + inst_fwd_max * (t - t_max)) / t)
            }
            Shape::ZeroSpreaded {
                base,
                spread,
                compounding,
                frequency,
            } => {
                let zero = base.zero_rate_at(t, *compounding, *frequency, true)?;
                let spreaded = InterestRate::new(
                    zero.rate + spread,
                    zero.day_count,
                    zero.compounding,
                    zero.frequency,
                )?;
                Ok(spreaded
                    .equivalent_rate(Compounding::Continuous, 0, t)?
                    .rate)
            }
            Shape::Flat { .. } | Shape::Discount { .. } => unreachable!("not a zero-yield curve"),
        }
    }

    /// Zero rate at time `t` in the given compounding (QuantLib `zeroRate(t, comp, freq,
    /// extrapolate)`); at `t` = 0 the rate over 0.0001 years is returned.
    pub fn zero_rate_at(
        &self,
        t: f64,
        compounding: Compounding,
        frequency: u32,
        extrapolate: bool,
    ) -> Result<InterestRate, Error> {
        let t = if t == 0.0 { DT } else { t };
        let compound = 1.0 / self.discount_at(t, extrapolate)?;
        InterestRate::implied_rate(compound, self.day_count.clone(), compounding, frequency, t)
    }
}
