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

use crate::autodiff::{Operand, Recorder, Var};
use crate::interest_rate::{Compounding, InterestRate};
use crate::math::interpolation::{Linear, MonotonicCubicSpline};
use crate::math::solver1d::close;
use crate::math::solver1d::close_enough;
use crate::time::{Date, DayCount};
use crate::Error;

/// Time step used for the zero rate at the reference date (QuantLib `dt`).
const DT: f64 = 0.0001;

/// A yield curve on a reference date.
#[derive(Debug, Clone, PartialEq)]
pub struct YieldCurve<T = f64> {
    reference_date: Date,
    day_count: DayCount,
    /// Whether discount factors beyond the last date are allowed.
    pub extrapolate: bool,
    shape: Shape<T>,
}

#[derive(Debug, Clone, PartialEq)]
enum Shape<T> {
    /// QuantLib `FlatForward` with a fixed rate.
    Flat { rate: InterestRate<T> },
    /// QuantLib `ZeroCurve` (`InterpolatedZeroCurve<Linear>`): continuous zero rates.
    Zero { dates: Vec<Date>, zeros: Linear<T> },
    /// QuantLib `InterpolatedDiscountCurve`: an interpolation of the log discounts.
    Discount {
        dates: Vec<Date>,
        discounts: Vec<T>,
        log_discounts: LogDiscounts<T>,
    },
    /// QuantLib `ZeroSpreadedTermStructure`: a spread added to the base zero rate.
    ZeroSpreaded {
        base: Box<YieldCurve<T>>,
        spread: T,
        compounding: Compounding,
        frequency: u32,
    },
}

/// How log discount factors are interpolated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogDiscountInterpolation {
    /// QuantLib `LogLinear`.
    LogLinear,
    /// QuantLib `MonotonicLogCubic`: spline, monotonic, second derivative 0 at both ends.
    MonotonicLogCubic,
}

/// An interpolation of log discount factors, fixed when it is built.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LogDiscounts<T = f64> {
    Linear(Linear<T>),
    MonotonicCubic(MonotonicCubicSpline<T>),
}

impl<T: Operand> LogDiscounts<T> {
    pub(crate) fn new(
        interpolation: LogDiscountInterpolation,
        times: Vec<f64>,
        logs: Vec<T>,
    ) -> Result<LogDiscounts<T>, Error> {
        Ok(match interpolation {
            LogDiscountInterpolation::LogLinear => {
                LogDiscounts::Linear(Linear::new(times, logs)?)
            }
            LogDiscountInterpolation::MonotonicLogCubic => {
                LogDiscounts::MonotonicCubic(MonotonicCubicSpline::new(times, logs)?)
            }
        })
    }

    pub(crate) fn value(&self, t: f64) -> T {
        match self {
            LogDiscounts::Linear(i) => i.value(t),
            LogDiscounts::MonotonicCubic(i) => i.value(t),
        }
    }

    fn derivative(&self, t: f64) -> T {
        match self {
            LogDiscounts::Linear(i) => i.derivative(t),
            LogDiscounts::MonotonicCubic(i) => i.derivative(t),
        }
    }

    fn xs(&self) -> &[f64] {
        match self {
            LogDiscounts::Linear(i) => i.xs(),
            LogDiscounts::MonotonicCubic(i) => i.xs(),
        }
    }

    fn ys(&self) -> &[T] {
        match self {
            LogDiscounts::Linear(i) => i.ys(),
            LogDiscounts::MonotonicCubic(i) => i.ys(),
        }
    }

    fn kind(&self) -> LogDiscountInterpolation {
        match self {
            LogDiscounts::Linear(_) => LogDiscountInterpolation::LogLinear,
            LogDiscounts::MonotonicCubic(_) => LogDiscountInterpolation::MonotonicLogCubic,
        }
    }

    fn last_time(&self) -> f64 {
        *self.xs().last().expect("at least 2 points")
    }
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

impl YieldCurve<f64> {
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
        YieldCurve::interpolated_discounts(
            dates,
            discounts,
            day_count,
            LogDiscountInterpolation::LogLinear,
        )
    }

    /// Discount factors with the given interpolation (QuantLib `InterpolatedDiscountCurve`).
    /// The first date is the reference date and its discount factor must be 1.
    pub fn interpolated_discounts(
        dates: Vec<Date>,
        discounts: Vec<f64>,
        day_count: DayCount,
        interpolation: LogDiscountInterpolation,
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
                log_discounts: LogDiscounts::new(interpolation, times, logs)?,
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

    /// The same curve with its inputs as independent variables of `recorder`, and those inputs.
    /// The dates, times and day count stay fixed. On a discount-factor curve the inputs are the
    /// logarithms of the discount factors after the reference date; on a zero curve, the
    /// continuous zero rates held by the curve.
    pub fn lift<'t>(&self, recorder: &'t Recorder) -> Result<LiftedCurve<'t>, Error> {
        let mut inputs = Vec::new();
        let shape = match &self.shape {
            Shape::Flat { rate } => {
                let var = recorder.input(rate.rate);
                inputs.push((CurveInput::FlatRate, var));
                Shape::Flat {
                    rate: InterestRate::new(
                        var,
                        rate.day_count.clone(),
                        rate.compounding,
                        rate.frequency,
                    )?,
                }
            }
            Shape::Zero { dates, zeros } => {
                let vars: Vec<Var<'t>> = zeros.ys().iter().map(|&z| recorder.input(z)).collect();
                for (&date, &var) in dates.iter().zip(&vars) {
                    inputs.push((CurveInput::ZeroRate { date }, var));
                }
                Shape::Zero {
                    dates: dates.clone(),
                    zeros: Linear::new(zeros.xs().to_vec(), vars)?,
                }
            }
            Shape::Discount { dates, log_discounts, .. } => {
                let mut logs = vec![recorder.constant(log_discounts.ys()[0])];
                for (i, &date) in dates.iter().enumerate().skip(1) {
                    let var = recorder.input(log_discounts.ys()[i]);
                    let time = log_discounts.xs()[i];
                    inputs.push((CurveInput::LogDiscount { date, time }, var));
                    logs.push(var);
                }
                let discounts = logs.iter().map(|l| l.exp()).collect();
                Shape::Discount {
                    dates: dates.clone(),
                    discounts,
                    log_discounts: LogDiscounts::new(
                        log_discounts.kind(),
                        log_discounts.xs().to_vec(),
                        logs,
                    )?,
                }
            }
            Shape::ZeroSpreaded { base, spread, compounding, frequency } => {
                let lifted = base.lift(recorder)?;
                inputs.extend(lifted.inputs);
                let var = recorder.input(*spread);
                inputs.push((CurveInput::ZeroSpread, var));
                Shape::ZeroSpreaded {
                    base: Box::new(lifted.curve),
                    spread: var,
                    compounding: *compounding,
                    frequency: *frequency,
                }
            }
        };
        Ok(LiftedCurve {
            curve: YieldCurve {
                reference_date: self.reference_date,
                day_count: self.day_count.clone(),
                extrapolate: self.extrapolate,
                shape,
            },
            inputs,
        })
    }
}

/// An input of a curve that derivatives are taken with respect to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CurveInput {
    FlatRate,
    ZeroRate { date: Date },
    /// The logarithm of the discount factor at `date`, `time` years from the reference date.
    LogDiscount { date: Date, time: f64 },
    ZeroSpread,
}

/// A curve whose inputs are recorded variables.
#[derive(Debug, Clone)]
pub struct LiftedCurve<'t> {
    pub curve: YieldCurve<Var<'t>>,
    pub inputs: Vec<(CurveInput, Var<'t>)>,
}

impl<T: Operand> YieldCurve<T> {
    /// A constant in the same calculation as the curve.
    pub(crate) fn like(&self, x: f64) -> T {
        match &self.shape {
            Shape::Flat { rate } => rate.rate.like(x),
            Shape::Zero { zeros, .. } => zeros.ys()[0].like(x),
            Shape::Discount { discounts, .. } => discounts[0].like(x),
            Shape::ZeroSpreaded { spread, .. } => spread.like(x),
        }
    }

    /// The same curve with extrapolation allowed or not.
    pub fn with_extrapolation(mut self, extrapolate: bool) -> YieldCurve<T> {
        self.extrapolate = extrapolate;
        self
    }

    pub fn reference_date(&self) -> Date {
        self.reference_date
    }

    pub fn day_count(&self) -> &DayCount {
        &self.day_count
    }

    /// Node dates and discount factors of a discount-factor curve.
    pub fn discount_nodes(&self) -> Option<(&[Date], &[T])> {
        match &self.shape {
            Shape::Discount { dates, discounts, .. } => Some((dates, discounts)),
            _ => None,
        }
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
    pub fn discount(&self, d: Date) -> Result<T, Error> {
        self.discount_at(self.time_from_reference(d)?, false)
    }

    /// Discount factor at time `t`; `extrapolate` allows times beyond the curve for this call.
    pub fn discount_at(&self, t: f64, extrapolate: bool) -> Result<T, Error> {
        self.check_range(t, extrapolate)?;
        self.discount_impl(t)
    }

    fn discount_impl(&self, t: f64) -> Result<T, Error> {
        match &self.shape {
            Shape::Flat { rate } => rate.discount_factor(t),
            Shape::Discount {
                discounts,
                log_discounts,
                ..
            } => {
                let t_max = log_discounts.last_time();
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
                    return Ok(self.like(1.0));
                }
                Ok((-self.zero_yield(t)? * t).exp())
            }
        }
    }

    /// Continuous zero yield at `t` > 0 of a curve defined by zero rates (QuantLib `zeroYieldImpl`).
    fn zero_yield(&self, t: f64) -> Result<T, Error> {
        match &self.shape {
            Shape::Zero { zeros, .. } => {
                let t_max = *zeros.xs().last().expect("at least 2 points");
                if t <= t_max {
                    return Ok(zeros.value(t));
                }
                // flat forward extrapolation
                let z_max = *zeros.ys().last().expect("at least 2 points");
                let inst_fwd_max = z_max + zeros.derivative(t_max) * t_max;
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
                    zero.rate + *spread,
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

    /// Zero rate to the date `d` (QuantLib `zeroRate(d, dayCounter, comp, freq)`); on the
    /// reference date the rate over 0.0001 years is returned.
    pub fn zero_rate(
        &self,
        d: Date,
        day_count: &DayCount,
        compounding: Compounding,
        frequency: u32,
    ) -> Result<InterestRate<T>, Error> {
        let t = self.time_from_reference(d)?;
        if t == 0.0 {
            let compound = self.discount_at(DT, false)?.recip();
            return InterestRate::implied_rate(
                compound,
                day_count.clone(),
                compounding,
                frequency,
                DT,
            );
        }
        let compound = self.discount_at(t, false)?.recip();
        let t = day_count.year_fraction(self.reference_date, d, None, None)?;
        InterestRate::implied_rate(compound, day_count.clone(), compounding, frequency, t)
    }

    /// Forward rate from `d1` to `d2` (QuantLib `forwardRate(d1, d2, dayCounter, comp, freq)`);
    /// when the dates are equal, the rate over 0.0001 years around `d1`.
    pub fn forward_rate(
        &self,
        d1: Date,
        d2: Date,
        day_count: &DayCount,
        compounding: Compounding,
        frequency: u32,
    ) -> Result<InterestRate<T>, Error> {
        if d1 == d2 {
            let t = self.time_from_reference(d1)?;
            self.check_range(t, false)?;
            let t1 = (t - DT / 2.0).max(0.0);
            let t2 = t1 + DT;
            let compound = self.discount_at(t1, true)? / self.discount_at(t2, true)?;
            return InterestRate::implied_rate(
                compound,
                day_count.clone(),
                compounding,
                frequency,
                DT,
            );
        }
        if d1 > d2 {
            return Err(Error::InvalidInput {
                reason: format!("{d1} later than {d2}"),
            });
        }
        let compound = self.discount(d1)? / self.discount(d2)?;
        let t = day_count.year_fraction(d1, d2, None, None)?;
        InterestRate::implied_rate(compound, day_count.clone(), compounding, frequency, t)
    }

    /// The instantaneous forward at time `t`, from a rate over 0.0001 years (QuantLib
    /// `forwardRate(t, t, comp)`).
    pub fn forward_rate_at_time(
        &self,
        t: f64,
        compounding: Compounding,
    ) -> Result<InterestRate<T>, Error> {
        let t1 = (t - DT / 2.0).max(0.0);
        let t2 = t1 + DT;
        let compound = self.discount_at(t1, true)? / self.discount_at(t2, true)?;
        InterestRate::implied_rate(compound, self.day_count.clone(), compounding, 1, DT)
    }

    /// Zero rate at time `t` in the given compounding (QuantLib `zeroRate(t, comp, freq,
    /// extrapolate)`); at `t` = 0 the rate over 0.0001 years is returned.
    pub fn zero_rate_at(
        &self,
        t: f64,
        compounding: Compounding,
        frequency: u32,
        extrapolate: bool,
    ) -> Result<InterestRate<T>, Error> {
        let t = if t == 0.0 { DT } else { t };
        let compound = self.discount_at(t, extrapolate)?.recip();
        InterestRate::implied_rate(compound, self.day_count.clone(), compounding, frequency, t)
    }
}
