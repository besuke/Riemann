//! Credit default swaps: survival curves with piecewise flat hazard rates or linear survival
//! probabilities, the MidPoint and ISDA engines, and bootstrapping from running spreads or upfronts.
//!
//! Reproduces QuantLib v1.42 (modified BSD license; see THIRD_PARTY_NOTICES):
//! `ql/instruments/creditdefaultswap.cpp` (Copyright (C) 2008, 2009 Jose Aparicio; (C) 2008 Roland
//! Lichters; (C) 2008 StatPro Italia srl), `ql/pricingengines/credit/midpointcdsengine.cpp`
//! (Copyright (C) 2008, 2009 Jose Aparicio; (C) 2008 Roland Lichters; (C) 2008, 2009 StatPro Italia
//! srl), `ql/pricingengines/credit/isdacdsengine.cpp` (Copyright (C) 2014 Jose Aparicio; (C) 2014
//! Peter Caspers), `ql/termstructures/credit/defaultprobabilityhelpers.cpp` (Copyright (C) 2008,
//! 2009 Jose Aparicio; (C) 2008 Chris Kenyon; (C) 2008 Roland Lichters; (C) 2008 StatPro Italia srl;
//! (C) 2023 Andrea Pellegatta) and `ql/termstructures/credit/interpolatedsurvivalprobabilitycurve.hpp`
//! (Copyright (C) 2009 Ferdinando Ametrano).
//!
//! The engines are generic over [`Operand`], so sensitivities come from automatic differentiation.

use crate::autodiff::{Operand, Recorder};
use crate::cashflows::FixedRateCoupon;
use crate::curves::YieldCurve;
use crate::math::linear::compute_inverse;
use crate::math::solver1d::solve_brent;
use crate::time::{
    generate_schedule, BusinessDayConvention, Date, DateGenerationRule, DayCount, HolidayCalendar,
    Period, ScheduleSpec, TimeUnit,
};
use crate::Error;

fn invalid<T>(reason: String) -> Result<T, Error> {
    Err(Error::InvalidInput { reason })
}

/// What the node values of a [`HazardCurve`] are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CreditCurveShape {
    /// Piecewise flat hazard rates: the rate at index `i` holds from the node before it up to its
    /// node (QuantLib `InterpolatedHazardRateCurve<BackwardFlat>`), and the last rate holds beyond
    /// the last node.
    FlatHazard,
    /// Survival probabilities, linear in time between the nodes, with 1 at the reference date.
    /// Beyond the last node the hazard rate at that node holds (QuantLib
    /// `InterpolatedSurvivalProbabilityCurve<Linear>`, `SurvivalProbabilityCurve` in Python).
    LinearSurvival,
}

/// A survival curve given by nodes after its reference date.
#[derive(Debug, Clone, PartialEq)]
pub struct HazardCurve<T = f64> {
    reference_date: Date,
    day_count: DayCount,
    shape: CreditCurveShape,
    /// After the reference date, increasing.
    dates: Vec<Date>,
    times: Vec<f64>,
    /// Hazard rates or survival probabilities, by `shape`.
    values: Vec<T>,
}

impl<T: Operand> HazardCurve<T> {
    /// A curve with piecewise flat hazard rates, from its nodes after the reference date.
    pub fn new(
        reference_date: Date,
        day_count: DayCount,
        dates: Vec<Date>,
        hazards: Vec<T>,
    ) -> Result<HazardCurve<T>, Error> {
        HazardCurve::with_shape(reference_date, day_count, CreditCurveShape::FlatHazard, dates, hazards)
    }

    /// A curve of survival probabilities linear in time, from its nodes after the reference date.
    /// The probabilities must be positive and must not increase.
    pub fn survival_probabilities(
        reference_date: Date,
        day_count: DayCount,
        dates: Vec<Date>,
        probabilities: Vec<T>,
    ) -> Result<HazardCurve<T>, Error> {
        let mut previous = 1.0;
        for (d, p) in dates.iter().zip(&probabilities) {
            let p = p.primal();
            if !(p > 0.0 && p <= previous) {
                return invalid(format!(
                    "survival probabilities must be positive and must not increase: {p} at {d} after {previous}"
                ));
            }
            previous = p;
        }
        HazardCurve::with_shape(reference_date, day_count, CreditCurveShape::LinearSurvival, dates, probabilities)
    }

    fn with_shape(
        reference_date: Date,
        day_count: DayCount,
        shape: CreditCurveShape,
        dates: Vec<Date>,
        values: Vec<T>,
    ) -> Result<HazardCurve<T>, Error> {
        if dates.is_empty() || dates.len() != values.len() {
            return invalid("a survival curve needs as many values as dates, at least one".into());
        }
        if dates[0] <= reference_date || dates.windows(2).any(|w| w[1] <= w[0]) {
            return invalid("survival curve dates must follow the reference date and increase".into());
        }
        let times = dates
            .iter()
            .map(|&d| day_count.year_fraction(reference_date, d, None, None))
            .collect::<Result<Vec<_>, Error>>()?;
        Ok(HazardCurve { reference_date, day_count, shape, dates, times, values })
    }

    /// One flat rate (QuantLib `FlatHazardRate`).
    pub fn flat(reference_date: Date, day_count: DayCount, hazard: T) -> Result<HazardCurve<T>, Error> {
        let far = Date::from_serial(reference_date.serial() as i64 + 1)?;
        HazardCurve::new(reference_date, day_count, vec![far], vec![hazard])
    }

    pub fn reference_date(&self) -> Date {
        self.reference_date
    }

    pub fn shape(&self) -> CreditCurveShape {
        self.shape
    }

    pub fn dates(&self) -> &[Date] {
        &self.dates
    }

    /// The node values: hazard rates or survival probabilities, by [`HazardCurve::shape`].
    pub fn values(&self) -> &[T] {
        &self.values
    }

    /// The hazard rates of a curve with piecewise flat hazard rates.
    pub fn hazards(&self) -> Result<&[T], Error> {
        match self.shape {
            CreditCurveShape::FlatHazard => Ok(&self.values),
            CreditCurveShape::LinearSurvival => invalid("the curve is given by survival probabilities".into()),
        }
    }

    /// Times and survival probabilities of a linear survival curve, with the reference date.
    fn survival_node(&self, i: usize) -> (f64, T) {
        if i == 0 {
            (0.0, self.values[0].like(1.0))
        } else {
            (self.times[i - 1], self.values[i - 1])
        }
    }

    /// The segment of a linear survival curve that holds `t`: the one starting at the node on or
    /// before it, the last one beyond the last node (QuantLib `Interpolation::locate`).
    fn linear_segment(&self, t: f64) -> usize {
        self.times.iter().take(self.times.len() - 1).take_while(|&&time| time <= t).count()
    }

    /// The probability of surviving to time `t`.
    pub fn survival_at(&self, t: f64) -> T {
        match self.shape {
            CreditCurveShape::FlatHazard => {
                let mut integral = self.values[0].like(0.0);
                let mut previous = 0.0;
                for (&time, &hazard) in self.times.iter().zip(&self.values) {
                    if t <= time {
                        return (-(integral + hazard * (t - previous))).exp();
                    }
                    integral = integral + hazard * (time - previous);
                    previous = time;
                }
                let last = self.values[self.values.len() - 1];
                (-(integral + last * (t - previous))).exp()
            }
            CreditCurveShape::LinearSurvival => {
                let last = self.times.len();
                if t <= self.times[last - 1] {
                    let i = self.linear_segment(t);
                    let ((t0, s0), (t1, s1)) = (self.survival_node(i), self.survival_node(i + 1));
                    s0 + (s1 - s0) * ((t - t0) / (t1 - t0))
                } else {
                    let (t_max, s_max) = self.survival_node(last);
                    s_max * (-(self.hazard_rate_at(t_max) * (t - t_max))).exp()
                }
            }
        }
    }

    /// The hazard rate at time `t`: the rate of the period holding `t`, or the default density
    /// over the survival probability (QuantLib `hazardRate`).
    pub fn hazard_rate_at(&self, t: f64) -> T {
        match self.shape {
            CreditCurveShape::FlatHazard => {
                let i = self.times.iter().position(|&time| t <= time).unwrap_or(self.times.len() - 1);
                self.values[i]
            }
            CreditCurveShape::LinearSurvival => {
                let last = self.times.len();
                let i = self.linear_segment(t.min(self.times[last - 1]));
                let ((t0, s0), (t1, s1)) = (self.survival_node(i), self.survival_node(i + 1));
                let slope = (s1 - s0) / (t1 - t0);
                if t <= self.times[last - 1] {
                    -slope / self.survival_at(t)
                } else {
                    -slope / s1
                }
            }
        }
    }

    fn time_of(&self, d: Date) -> Result<f64, Error> {
        self.day_count.year_fraction(self.reference_date, d, None, None)
    }

    /// The probability of surviving to `d`. Dates before the reference date count as 1.
    pub fn survival(&self, d: Date) -> Result<T, Error> {
        if d <= self.reference_date {
            return Ok(self.values[0].like(1.0));
        }
        Ok(self.survival_at(self.time_of(d)?))
    }

    /// The hazard rate at `d`, which must not be before the reference date.
    pub fn hazard_rate(&self, d: Date) -> Result<T, Error> {
        if d < self.reference_date {
            return invalid(format!("{d} is before the reference date {}", self.reference_date));
        }
        Ok(self.hazard_rate_at(self.time_of(d)?))
    }
}

/// Which side of the swap is valued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtectionSide {
    Buyer,
    Seller,
}

/// The contract of a credit default swap, with face value claims.
#[derive(Debug, Clone, PartialEq)]
pub struct CdsSpec {
    pub side: ProtectionSide,
    pub notional: f64,
    pub running_spread: f64,
    pub schedule: ScheduleSpec,
    pub payment_convention: BusinessDayConvention,
    pub day_count: DayCount,
    /// Used for the last coupon when given (QuantLib `lastPeriodDayCounter`).
    pub last_period_day_count: Option<DayCount>,
    /// Paid at the cash settlement date, per unit of notional.
    pub upfront: Option<f64>,
    /// The first date of the schedule when not given.
    pub protection_start: Option<Date>,
    /// `protection_start` for the CDS rules, the day before it otherwise, when not given.
    pub trade_date: Option<Date>,
    /// Business days from the trade date to the cash settlement date; QuantLib's default is 3.
    pub cash_settlement_days: i32,
    pub rebates_accrual: bool,
}

/// The legs of a swap on a date, independent of the engine.
#[derive(Debug, Clone)]
pub struct Cds {
    spec: CdsSpec,
    calendar: HolidayCalendar,
    coupons: Vec<FixedRateCoupon>,
    protection_start: Date,
    maturity: Date,
    upfront_date: Date,
    upfront_amount: f64,
    /// The accrued amount rebated at the cash settlement date, when `rebates_accrual` holds.
    accrual_rebate: Option<f64>,
}

impl Cds {
    pub fn new(spec: CdsSpec) -> Result<Cds, Error> {
        let schedule = generate_schedule(&spec.schedule)?;
        if schedule.dates.len() < 2 {
            return invalid("a credit default swap needs at least one period".into());
        }
        let post_big_bang = matches!(
            spec.schedule.rule,
            DateGenerationRule::Cds | DateGenerationRule::Cds2015
        );
        let protection_start = spec.protection_start.unwrap_or(schedule.dates[0]);
        if !post_big_bang && protection_start > schedule.dates[0] {
            return invalid("protection can not start after accrual".into());
        }
        let calendar = spec.schedule.calendar.clone();
        let n = schedule.dates.len() - 1;
        let mut coupons = Vec::with_capacity(n);
        for i in 0..n {
            let (start, end) = (schedule.dates[i], schedule.dates[i + 1]);
            let day_count = match (&spec.last_period_day_count, i == n - 1) {
                (Some(last), true) => last.clone(),
                _ => spec.day_count.clone(),
            };
            coupons.push(FixedRateCoupon {
                payment_date: calendar.adjust(end, spec.payment_convention)?,
                nominal: spec.notional,
                rate: spec.running_spread,
                day_count,
                accrual_start: start,
                accrual_end: end,
                ref_start: start,
                ref_end: end,
            });
        }
        let trade_date = match spec.trade_date {
            Some(d) => d,
            None if post_big_bang => protection_start,
            None => Date::from_serial(protection_start.serial() as i64 - 1)?,
        };
        let upfront_date = calendar.advance(
            trade_date,
            Period::new(spec.cash_settlement_days, TimeUnit::Days),
            spec.payment_convention,
            false,
        )?;
        if upfront_date < protection_start {
            return invalid("the cash settlement date must not be before the protection start".into());
        }
        let accrual_rebate = if spec.rebates_accrual {
            let mut rebate = 0.0;
            let reference = Date::from_serial(trade_date.serial() as i64 + 1)?;
            if trade_date >= schedule.dates[0] {
                for (i, c) in coupons.iter().enumerate() {
                    if reference > c.payment_date {
                        continue;
                    }
                    rebate = if reference == c.payment_date {
                        if i < n - 1 { 0.0 } else { c.amount()? }
                    } else {
                        c.accrued_amount(reference)?
                    };
                    break;
                }
            }
            Some(rebate)
        } else {
            None
        };
        Ok(Cds {
            upfront_amount: spec.upfront.unwrap_or(0.0) * spec.notional,
            maturity: schedule.dates[n],
            spec,
            calendar,
            coupons,
            protection_start,
            upfront_date,
            accrual_rebate,
        })
    }

    pub fn calendar(&self) -> &HolidayCalendar {
        &self.calendar
    }

    pub fn maturity(&self) -> Date {
        self.maturity
    }
}

/// The value of a swap to its side and its components.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CdsValue<T = f64> {
    pub npv: T,
    pub coupon_leg_npv: T,
    pub default_leg_npv: T,
    pub upfront_npv: T,
    pub accrual_rebate_npv: T,
    pub fair_spread: T,
    pub fair_upfront: Option<T>,
}

fn discount_on<T: Operand>(curve: &YieldCurve<T>, d: Date) -> Result<T, Error> {
    curve.discount_at(curve.time_from_reference(d)?, true)
}

fn finish<T: Operand>(
    cds: &Cds,
    mut coupon_leg: T,
    mut default_leg: T,
    mut upfront_npv: T,
    mut rebate: T,
    upfront_discount: Option<T>,
) -> CdsValue<T> {
    let mut upfront_sign = 1.0;
    match cds.spec.side {
        ProtectionSide::Seller => {
            default_leg = -default_leg;
            rebate = -rebate;
        }
        ProtectionSide::Buyer => {
            coupon_leg = -coupon_leg;
            upfront_npv = -upfront_npv;
            upfront_sign = -1.0;
        }
    }
    let npv = default_leg + coupon_leg + upfront_npv + rebate;
    let fair_spread = -default_leg * cds.spec.running_spread / (coupon_leg + rebate);
    let fair_upfront = upfront_discount.filter(|d| d.primal() > 0.0).map(|d| {
        -(default_leg + coupon_leg + rebate) * upfront_sign / (d * cds.spec.notional)
    });
    CdsValue {
        npv,
        coupon_leg_npv: coupon_leg,
        default_leg_npv: default_leg,
        upfront_npv,
        accrual_rebate_npv: rebate,
        fair_spread,
        fair_upfront,
    }
}

/// The MidPoint engine: default in the middle of each coupon period, with accrual and the claim
/// paid at default. `today` is the evaluation date. Flows on or before the curve's reference date
/// have occurred.
pub fn compute_midpoint_value<T: Operand>(
    cds: &Cds,
    hazard: &HazardCurve<T>,
    recovery: T,
    curve: &YieldCurve<T>,
    today: Date,
) -> Result<CdsValue<T>, Error> {
    let settlement = curve.reference_date();
    let zero = recovery.like(0.0);
    let (mut upfront_npv, mut upfront_discount) = (zero, None);
    if cds.upfront_date > settlement {
        let d = discount_on(curve, cds.upfront_date)?;
        upfront_discount = Some(d);
        upfront_npv = d * cds.upfront_amount;
    }
    let mut rebate = zero;
    if let Some(amount) = cds.accrual_rebate {
        if cds.upfront_date > settlement {
            rebate = discount_on(curve, cds.upfront_date)? * amount;
        }
    }
    let (mut coupon_leg, mut default_leg) = (zero, zero);
    let claim = (-recovery + 1.0) * cds.spec.notional;
    for (i, c) in cds.coupons.iter().enumerate() {
        if c.payment_date <= settlement {
            continue;
        }
        let start = if i == 0 { cds.protection_start } else { c.accrual_start };
        let end = c.accrual_end;
        let effective = if start <= today && today <= end { today } else { start };
        let default_date = Date::from_serial(
            effective.serial() as i64 + (end.serial() as i64 - effective.serial() as i64) / 2,
        )?;
        let s = hazard.survival(c.payment_date)?;
        let p = hazard.survival(effective)? - hazard.survival(end)?;
        let pay = discount_on(curve, c.payment_date)?;
        let at_default = discount_on(curve, default_date)?;
        coupon_leg = coupon_leg + s * c.amount()? * pay;
        coupon_leg = coupon_leg + p * c.accrued_amount(default_date)? * at_default;
        default_leg = default_leg + p * claim * at_default;
    }
    Ok(finish(cds, coupon_leg, default_leg, upfront_npv, rebate, upfront_discount))
}

/// The ISDA engine, with QuantLib's defaults: Taylor expansion for small rates, half-day bias on
/// accrual and piecewise forwards in coupon periods. Both curves must be Actual/365 (Fixed), with
/// the reference date on `today`, and the discount curve must be flat or log-linear on discounts.
pub fn compute_isda_value<T: Operand>(
    cds: &Cds,
    hazard: &HazardCurve<T>,
    recovery: T,
    curve: &YieldCurve<T>,
    today: Date,
) -> Result<CdsValue<T>, Error> {
    if curve.reference_date() != today || hazard.reference_date() != today {
        return invalid("the ISDA engine needs both curves on the evaluation date".into());
    }
    let day = |d: Date, n: i64| Date::from_serial(d.serial() as i64 + n);
    let maturity = cds.maturity;
    let effective = cds.protection_start.max(day(today, 1)?);
    let mut nodes: Vec<Date> = curve.discount_nodes().map_or(Vec::new(), |(d, _)| d.to_vec());
    // a flat curve's single artificial node is one day after its reference date, and has no effect
    if hazard.dates().len() > 1 || hazard.shape() == CreditCurveShape::LinearSurvival {
        nodes.extend(std::iter::once(hazard.reference_date()).chain(hazard.dates().iter().copied()));
    }
    nodes.sort();
    nodes.dedup();
    if nodes.is_empty() {
        nodes.push(maturity);
    }
    let survival_of = |d: Date| hazard.survival(d);
    let zero = recovery.like(0.0);

    // protection leg
    let mut protection = zero;
    let d0 = day(effective, -1)?;
    let mut p0 = discount_on(curve, d0)?;
    let mut q0 = survival_of(d0)?;
    let first = nodes.partition_point(|&n| n <= effective);
    for &node in &nodes[first..] {
        let d1 = node.min(maturity);
        let p1 = discount_on(curve, d1)?;
        let q1 = survival_of(d1)?;
        let fhat = p0.ln() - p1.ln();
        let hhat = q0.ln() - q1.ln();
        let f = fhat + hhat;
        protection = protection
            + if f.primal() < 1e-4 {
                let f2 = f * f;
                p0 * q0 * hhat * (f2 * f2 * (1.0 / 120.0) - f2 * f * (1.0 / 24.0) + f2 * (1.0 / 6.0) - f * 0.5 + 1.0)
            } else {
                hhat / f * (p0 * q0 - p1 * q1)
            };

        p0 = p1;
        q0 = q1;
        if node > maturity {
            break;
        }
    }
    let default_leg = protection * ((-recovery + 1.0) * cds.spec.notional);

    // premium leg
    let (mut premium, mut default_accrual) = (zero, zero);
    for c in &cds.coupons {
        if c.payment_date > effective {
            premium = premium
                + discount_on(curve, c.payment_date)? * c.amount()? * survival_of(day(c.payment_date, -1)?)?;
        }
        if c.accrual_end > effective {
            let start = day(c.accrual_start.max(effective), -1)?;
            let end = day(c.payment_date, -1)?;
            let tstart = curve.time_from_reference(day(c.accrual_start, -1)?)? - 1.0 / 730.0;
            let mut local = vec![start];
            let a = nodes.partition_point(|&n| n <= start);
            let b = nodes.partition_point(|&n| n < end);
            if a < b {
                local.extend_from_slice(&nodes[a..b]);
            }
            local.push(end);
            let mut accrual = zero;
            let mut t0 = curve.time_from_reference(local[0])?;
            let mut p0 = discount_on(curve, local[0])?;
            let mut q0 = survival_of(local[0])?;
            for &node in &local[1..] {
                let t1 = curve.time_from_reference(node)?;
                let p1 = discount_on(curve, node)?;
                let q1 = survival_of(node)?;
                let fhat = p0.ln() - p1.ln();
                let hhat = q0.ln() - q1.ln();
                let f = fhat + hhat;
                accrual = accrual
                    + if f.primal() < 1e-4 {
                        let f2 = f * f;
                        hhat * p0 * q0
                            * ((f2 * (1.0 / 6.0) - f2 * f * (1.0 / 24.0) - f * 0.5 + 1.0) * (t0 - tstart)
                                + (f2 * (1.0 / 8.0) - f2 * f * (1.0 / 30.0) - f * (1.0 / 3.0) + 0.5) * (t1 - t0))
                    } else {
                        hhat / f * (((p0 * q0 - p1 * q1) / f - p1 * q1) * (t1 - t0) + (p0 * q0 - p1 * q1) * (t0 - tstart))
                    };
                t0 = t1;
                p0 = p1;
                q0 = q1;
            }
            default_accrual = default_accrual + accrual * (cds.spec.notional * c.rate * 365.0 / 360.0);
        }
    }
    let coupon_leg = premium + default_accrual;

    let (mut upfront_npv, mut upfront_discount) = (zero, None);
    if cds.upfront_date > today {
        let d = discount_on(curve, cds.upfront_date)?;
        upfront_discount = Some(d);
        if cds.upfront_amount != 0.0 {
            upfront_npv = d * cds.upfront_amount;
        }
    }
    let mut rebate = zero;
    if let Some(amount) = cds.accrual_rebate {
        if amount != 0.0 && cds.upfront_date > today {
            rebate = discount_on(curve, cds.upfront_date)? * amount;
        }
    }
    Ok(finish(cds, coupon_leg, default_leg, upfront_npv, rebate, upfront_discount))
}

/// Which engine prices a swap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CdsEngine {
    MidPoint,
    Isda,
}

/// A value with either engine.
pub fn compute_cds_value<T: Operand>(
    engine: CdsEngine,
    cds: &Cds,
    hazard: &HazardCurve<T>,
    recovery: T,
    curve: &YieldCurve<T>,
    today: Date,
) -> Result<CdsValue<T>, Error> {
    match engine {
        CdsEngine::MidPoint => compute_midpoint_value(cds, hazard, recovery, curve, today),
        CdsEngine::Isda => compute_isda_value(cds, hazard, recovery, curve, today),
    }
}

/// One quote to fit: a running spread or an upfront, by [`CdsCurveSpec::quote_type`].
#[derive(Debug, Clone, PartialEq)]
pub struct CdsQuote {
    pub maturity: Date,
    pub value: f64,
}

/// What the quotes of a curve are.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CdsQuoteType {
    /// Running spreads (QuantLib `SpreadCdsHelper`).
    RunningSpread,
    /// Upfronts per unit of notional, paid by the buyer, on top of a fixed running spread, paid
    /// `settlement_days` business days after the evaluation date (QuantLib `UpfrontCdsHelper`).
    Upfront { running_spread: f64, settlement_days: i32 },
}

/// The conventions shared by the quotes of a curve.
#[derive(Debug, Clone, PartialEq)]
pub struct CdsCurveSpec {
    pub evaluation_date: Date,
    pub settlement_days: u32,
    pub calendar: HolidayCalendar,
    pub tenor: Period,
    pub payment_convention: BusinessDayConvention,
    pub rule: DateGenerationRule,
    pub day_count: DayCount,
    pub last_period_day_count: Option<DayCount>,
    pub rebates_accrual: bool,
    pub recovery: f64,
    pub engine: CdsEngine,
    pub curve_day_count: DayCount,
    pub accuracy: f64,
    pub quote_type: CdsQuoteType,
    pub quotes: Vec<CdsQuote>,
}

/// A bootstrapped hazard curve.
#[derive(Debug, Clone, PartialEq)]
pub struct CdsCurve {
    pub hazard: HazardCurve,
    pub spec: CdsCurveSpec,
}

impl CdsCurveSpec {
    /// The swap behind one quote: buyer, notional 100, running spread 1% for a spread quote
    /// (QuantLib `CdsHelper`), the fixed running spread and the upfront settlement for an upfront
    /// quote (QuantLib `UpfrontCdsHelper`, whose upfront is a placeholder for the fair upfront).
    fn helper(&self, maturity: Date) -> Result<Cds, Error> {
        let protection_start = Date::from_serial(self.evaluation_date.serial() as i64 + self.settlement_days as i64)?;
        let start = match self.rule {
            DateGenerationRule::Cds | DateGenerationRule::Cds2015 => protection_start,
            _ => self.calendar.adjust(protection_start, self.payment_convention)?,
        };
        let (running_spread, upfront, cash_settlement_days) = match self.quote_type {
            CdsQuoteType::RunningSpread => (0.01, None, 3),
            CdsQuoteType::Upfront { running_spread, settlement_days } => (running_spread, Some(0.01), settlement_days),
        };
        Cds::new(CdsSpec {
            side: ProtectionSide::Buyer,
            notional: 100.0,
            running_spread,
            schedule: ScheduleSpec {
                effective_date: start,
                termination_date: maturity,
                tenor: self.tenor,
                calendar: self.calendar.clone(),
                convention: self.payment_convention,
                termination_convention: BusinessDayConvention::Unadjusted,
                rule: self.rule,
                end_of_month: false,
                first_date: None,
                next_to_last_date: None,
            },
            payment_convention: self.payment_convention,
            day_count: self.day_count.clone(),
            last_period_day_count: self.last_period_day_count.clone(),
            upfront,
            protection_start: Some(protection_start),
            trade_date: Some(self.evaluation_date),
            cash_settlement_days,
            rebates_accrual: self.rebates_accrual,
        })
    }

    /// The quoted value implied by a curve: the fair spread or the fair upfront.
    fn implied<T: Operand>(&self, v: &CdsValue<T>, maturity: Date) -> Result<T, Error> {
        match self.quote_type {
            CdsQuoteType::RunningSpread => Ok(v.fair_spread),
            CdsQuoteType::Upfront { .. } => v.fair_upfront.ok_or_else(|| Error::InvalidInput {
                reason: format!("the upfront of the quote at {maturity} is not paid after the evaluation date"),
            }),
        }
    }

    /// The node of a quote: its last date, one day later for the ISDA engine.
    fn pillar(&self, maturity: Date) -> Result<Date, Error> {
        let adjusted = self.calendar.adjust(maturity, self.payment_convention)?;
        Ok(match self.engine {
            CdsEngine::Isda => Date::from_serial(adjusted.serial() as i64 + 1)?,
            CdsEngine::MidPoint => adjusted,
        })
    }
}

const AVERAGE_HAZARD: f64 = 0.01;
const MAX_HAZARD: f64 = 1.0;

/// Bootstraps piecewise flat hazard rates from running spreads or upfronts, one Brent search per
/// node from the shortest quote (QuantLib `PiecewiseFlatHazardRate`, whose interpolation needs a single pass).
pub fn bootstrap_hazard_curve(spec: &CdsCurveSpec, curve: &YieldCurve) -> Result<CdsCurve, Error> {
    if spec.quotes.is_empty() {
        return invalid("no spreads given".into());
    }
    let mut order: Vec<usize> = (0..spec.quotes.len()).collect();
    order.sort_by_key(|&i| spec.quotes[i].maturity);
    let today = spec.evaluation_date;
    let mut dates = Vec::with_capacity(order.len());
    let mut hazards: Vec<f64> = Vec::with_capacity(order.len());
    for (position, &i) in order.iter().enumerate() {
        let quote = &spec.quotes[i];
        let cds = spec.helper(quote.maturity)?;
        let pillar = spec.pillar(quote.maturity)?;
        if dates.last().is_some_and(|&d| d >= pillar) {
            return invalid(format!("more than one quote with the pillar {pillar}"));
        }
        dates.push(pillar);
        let guess = if position == 0 { AVERAGE_HAZARD } else { hazards[position - 1] };
        hazards.push(guess);
        let at = position;
        let objective = |h: f64| {
            let mut trial = hazards.clone();
            trial[at] = h;
            let curve_h = HazardCurve::new(today, spec.curve_day_count.clone(), dates.clone(), trial)?;
            let v = compute_cds_value(spec.engine, &cds, &curve_h, spec.recovery, curve, today)?;
            Ok(quote.value - spec.implied(&v, quote.maturity)?)
        };
        let root = solve_brent(objective, spec.accuracy, guess, f64::EPSILON, MAX_HAZARD, 100).map_err(|e| {
            Error::NoConvergence { reason: format!("quote at {}: {e}", quote.maturity) }
        })?;
        hazards[position] = root;
    }
    Ok(CdsCurve {
        hazard: HazardCurve::new(today, spec.curve_day_count.clone(), dates, hazards)?,
        spec: spec.clone(),
    })
}

/// The derivatives of one swap's value with respect to the quotes of its curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CdsQuoteSensitivity {
    /// Position in `CdsCurveSpec::quotes`.
    pub quote: usize,
    pub maturity: Date,
    /// The quoted running spread or upfront.
    pub value: f64,
    pub d_npv: f64,
}

/// A swap's value and its sensitivities: to each quote of the curve, with the curve
/// following by the implicit function theorem, and to the recovery rate with the hazard curve fixed.
#[derive(Debug, Clone, PartialEq)]
pub struct CdsSensitivity {
    pub value: CdsValue,
    pub quotes: Vec<CdsQuoteSensitivity>,
    pub d_npv_d_recovery: f64,
}

/// The sensitivities of a swap valued on a bootstrapped curve.
pub fn compute_cds_sensitivity(
    engine: CdsEngine,
    cds: &Cds,
    curve_h: &CdsCurve,
    curve: &YieldCurve,
    today: Date,
) -> Result<CdsSensitivity, Error> {
    let spec = &curve_h.spec;
    let mut order: Vec<usize> = (0..spec.quotes.len()).collect();
    order.sort_by_key(|&i| spec.quotes[i].maturity);
    let n = order.len();
    let rec = Recorder::new();
    let inputs: Vec<_> = curve_h.hazard.hazards()?.iter().map(|&h| rec.input(h)).collect();
    let lifted_hazard = HazardCurve::new(today, spec.curve_day_count.clone(), curve_h.hazard.dates().to_vec(), inputs.clone())?;
    let lifted_curve = curve.lift(&rec)?;
    let recovery = rec.input(spec.recovery);
    let value = compute_cds_value(engine, cds, &lifted_hazard, recovery, &lifted_curve.curve, today)?;

    // d implied quote / d hazard: lower triangular
    let mut rows = Vec::with_capacity(n);
    for &i in &order {
        let maturity = spec.quotes[i].maturity;
        let helper = spec.helper(maturity)?;
        let v = compute_cds_value(spec.engine, &helper, &lifted_hazard, rec.constant(spec.recovery), &lifted_curve.curve, today)?;
        rows.push(rec.gradient(spec.implied(&v, maturity)?, &inputs)?.iter().map(|g| g.mean()).collect::<Vec<f64>>());
    }
    let inverse_rows = compute_inverse(&rows)?;
    let g = rec.gradient(value.npv, &inputs)?;
    let quotes = (0..n)
        .map(|k| CdsQuoteSensitivity {
            quote: order[k],
            maturity: spec.quotes[order[k]].maturity,
            value: spec.quotes[order[k]].value,
            d_npv: (0..n).map(|i| g[i].mean() * inverse_rows[i][k]).sum(),
        })
        .collect();
    let d_recovery = rec.gradient(value.npv, &[recovery])?[0].mean();
    let plain = |x: crate::autodiff::Var| x.primal();
    Ok(CdsSensitivity {
        value: CdsValue {
            npv: plain(value.npv),
            coupon_leg_npv: plain(value.coupon_leg_npv),
            default_leg_npv: plain(value.default_leg_npv),
            upfront_npv: plain(value.upfront_npv),
            accrual_rebate_npv: plain(value.accrual_rebate_npv),
            fair_spread: plain(value.fair_spread),
            fair_upfront: value.fair_upfront.map(plain),
        },
        quotes,
        d_npv_d_recovery: d_recovery,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interest_rate::{Compounding, InterestRate};
    use crate::time::{Actual365Kind, Calendar};

    fn d(y: i32, m: u32, day: u32) -> Date {
        Date::from_ymd(y, m, day).unwrap()
    }

    fn act365() -> DayCount {
        DayCount::Actual365Fixed(Actual365Kind::Standard)
    }

    fn act360() -> DayCount {
        DayCount::Actual360 { include_last_day: false }
    }

    fn curve(rate: f64) -> YieldCurve {
        YieldCurve::flat(d(2026, 11, 2), InterestRate::new(rate, act365(), Compounding::Continuous, 0).unwrap())
    }

    fn spec(spread: f64, side: ProtectionSide) -> CdsSpec {
        CdsSpec {
            side,
            notional: 1_000_000.0,
            running_spread: spread,
            schedule: ScheduleSpec {
                effective_date: d(2026, 11, 3),
                termination_date: d(2031, 12, 20),
                tenor: Period::new(3, TimeUnit::Months),
                calendar: HolidayCalendar::from(Calendar::WeekendsOnly),
                convention: BusinessDayConvention::Following,
                termination_convention: BusinessDayConvention::Unadjusted,
                rule: DateGenerationRule::Backward,
                end_of_month: false,
                first_date: None,
                next_to_last_date: None,
            },
            payment_convention: BusinessDayConvention::Following,
            day_count: act360(),
            last_period_day_count: None,
            upfront: None,
            protection_start: None,
            trade_date: None,
            cash_settlement_days: 3,
            rebates_accrual: true,
        }
    }

    #[test]
    fn buyer_and_seller_are_opposite() {
        let h = HazardCurve::flat(d(2026, 11, 2), act365(), 0.02).unwrap();
        let today = d(2026, 11, 2);
        for engine in [CdsEngine::MidPoint, CdsEngine::Isda] {
            let buyer = compute_cds_value(engine, &Cds::new(spec(0.01, ProtectionSide::Buyer)).unwrap(), &h, 0.4, &curve(0.02), today).unwrap();
            let seller = compute_cds_value(engine, &Cds::new(spec(0.01, ProtectionSide::Seller)).unwrap(), &h, 0.4, &curve(0.02), today).unwrap();
            assert!((buyer.npv + seller.npv).abs() < 1e-8, "{engine:?}");
            assert!((buyer.fair_spread - seller.fair_spread).abs() < 1e-14);
        }
    }

    #[test]
    fn a_swap_at_the_fair_spread_is_worth_nothing() {
        let h = HazardCurve::flat(d(2026, 11, 2), act365(), 0.02).unwrap();
        let today = d(2026, 11, 2);
        for engine in [CdsEngine::MidPoint, CdsEngine::Isda] {
            let first = compute_cds_value(engine, &Cds::new(spec(0.01, ProtectionSide::Buyer)).unwrap(), &h, 0.4, &curve(0.02), today).unwrap();
            let at_fair = compute_cds_value(engine, &Cds::new(spec(first.fair_spread, ProtectionSide::Buyer)).unwrap(), &h, 0.4, &curve(0.02), today).unwrap();
            assert!(at_fair.npv.abs() < 1e-6, "{engine:?}: {}", at_fair.npv);
        }
    }

    #[test]
    fn the_credit_triangle_holds_approximately() {
        // fair spread is close to hazard * (1 - recovery)
        let h = HazardCurve::flat(d(2026, 11, 2), act365(), 0.02).unwrap();
        let v = compute_midpoint_value(&Cds::new(spec(0.01, ProtectionSide::Buyer)).unwrap(), &h, 0.4, &curve(0.02), d(2026, 11, 2)).unwrap();
        assert!((v.fair_spread - 0.012).abs() < 5e-4, "{}", v.fair_spread);
    }

    fn curve_spec(engine: CdsEngine, spreads: &[(i32, f64)]) -> CdsCurveSpec {
        quoted_spec(engine, CdsQuoteType::RunningSpread, spreads)
    }

    fn quoted_spec(engine: CdsEngine, quote_type: CdsQuoteType, quotes: &[(i32, f64)]) -> CdsCurveSpec {
        CdsCurveSpec {
            evaluation_date: d(2026, 11, 2),
            settlement_days: 0,
            calendar: HolidayCalendar::from(Calendar::WeekendsOnly),
            tenor: Period::new(3, TimeUnit::Months),
            payment_convention: BusinessDayConvention::Following,
            rule: DateGenerationRule::Backward,
            day_count: act360(),
            last_period_day_count: None,
            rebates_accrual: true,
            recovery: 0.4,
            engine,
            curve_day_count: act365(),
            accuracy: 1e-12,
            quote_type,
            quotes: quotes.iter().map(|&(y, value)| CdsQuote { maturity: d(2026 + y, 12, 20), value }).collect(),
        }
    }

    #[test]
    fn bootstrapping_reprices_the_quotes_and_sensitivities_agree_with_rebuilding() {
        let upfront = CdsQuoteType::Upfront { running_spread: 0.01, settlement_days: 3 };
        let cases = [
            (CdsQuoteType::RunningSpread, [(1, 0.006), (3, 0.009), (5, 0.012), (10, 0.015)]),
            (upfront, [(1, -0.002), (3, -0.008), (5, -0.004), (10, 0.011)]),
        ];
        let rates = curve(0.02);
        let today = d(2026, 11, 2);
        for (quote_type, quotes) in cases {
            for engine in [CdsEngine::MidPoint, CdsEngine::Isda] {
                let s = quoted_spec(engine, quote_type, &quotes);
                let built = bootstrap_hazard_curve(&s, &rates).unwrap();
                for q in &s.quotes {
                    let helper = s.helper(q.maturity).unwrap();
                    let v = compute_cds_value(engine, &helper, &built.hazard, 0.4, &rates, today).unwrap();
                    let implied = s.implied(&v, q.maturity).unwrap();
                    assert!((implied - q.value).abs() < 1e-9, "{engine:?} {quote_type:?}: {implied} {}", q.value);
                }
                // a coupon unlike the quotes', so that the value depends on more than the 5-year quote
                let swap = Cds::new(CdsSpec { upfront: Some(0.0), ..spec(0.013, ProtectionSide::Buyer) }).unwrap();
                sensitivities_agree_with_rebuilding(engine, quote_type, quotes, &swap, &built);
            }
        }
    }

    fn sensitivities_agree_with_rebuilding(
        engine: CdsEngine,
        quote_type: CdsQuoteType,
        quotes: [(i32, f64); 4],
        swap: &Cds,
        built: &CdsCurve,
    ) {
        let rates = curve(0.02);
        let today = d(2026, 11, 2);
        {
            let sens = compute_cds_sensitivity(engine, swap, built, &rates, today).unwrap();
            assert_eq!(sens.quotes.len(), 4);
            for row in &sens.quotes {
                let value_with = |bump: f64| {
                    let mut bumped = quotes;
                    bumped[row.quote].1 += bump;
                    let b = bootstrap_hazard_curve(&quoted_spec(engine, quote_type, &bumped), &rates).unwrap();
                    compute_cds_value(engine, swap, &b.hazard, 0.4, &rates, today).unwrap().npv
                };
                let h = 1e-6;
                let numeric = (value_with(h) - value_with(-h)) / (2.0 * h);
                assert!(
                    (row.d_npv - numeric).abs() < 1e-5 * numeric.abs().max(1.0),
                    "{engine:?} {quote_type:?} {}: {} {numeric}",
                    row.quote,
                    row.d_npv
                );
            }
            let with_recovery = |r: f64| compute_cds_value(engine, swap, &built.hazard, r, &rates, today).unwrap().npv;
            let numeric = (with_recovery(0.4 + 1e-6) - with_recovery(0.4 - 1e-6)) / 2e-6;
            assert!((sens.d_npv_d_recovery - numeric).abs() < 1e-5 * numeric.abs().max(1.0));
        }
    }

    #[test]
    fn a_linear_survival_curve_interpolates_probabilities_and_extrapolates_the_last_hazard() {
        let today = d(2025, 9, 22);
        let dates = vec![d(2026, 9, 22), d(2027, 9, 22), d(2030, 9, 22)];
        let c = HazardCurve::survival_probabilities(today, act365(), dates.clone(), vec![0.995, 0.985, 0.97]).unwrap();
        assert_eq!(c.survival(d(2027, 9, 22)).unwrap(), 0.985);
        // halfway in time between the first two nodes
        let t1 = 1.0;
        let t2 = 730.0 / 365.0;
        let mid = (t1 + t2) / 2.0;
        assert!((c.survival_at(mid) - 0.99).abs() < 1e-15);
        assert!((c.hazard_rate_at(mid) - (0.01 / (t2 - t1)) / 0.99).abs() < 1e-15);
        // beyond the last node the hazard at that node holds
        let t3 = c.times[2];
        let h = (0.015 / (t3 - t2)) / 0.97;
        assert!((c.hazard_rate_at(t3 + 1.0) - h).abs() < 1e-15);
        assert!((c.survival_at(t3 + 1.0) - 0.97 * (-h).exp()).abs() < 1e-15);
        assert!(c.hazards().is_err());
        assert!(HazardCurve::survival_probabilities(today, act365(), dates.clone(), vec![0.995, 0.996, 0.97]).is_err());
        assert!(HazardCurve::survival_probabilities(today, act365(), dates, vec![0.995, 0.985, 0.0]).is_err());
    }

    #[test]
    fn a_flat_hazard_curve_reports_the_rate_of_each_period() {
        let today = d(2026, 11, 2);
        let c = HazardCurve::new(today, act365(), vec![d(2027, 11, 2), d(2028, 11, 2)], vec![0.01, 0.03]).unwrap();
        assert_eq!(c.hazard_rate(today).unwrap(), 0.01);
        assert_eq!(c.hazard_rate(d(2027, 11, 2)).unwrap(), 0.01);
        assert_eq!(c.hazard_rate(d(2027, 11, 3)).unwrap(), 0.03);
        assert_eq!(c.hazard_rate(d(2040, 1, 1)).unwrap(), 0.03);
        assert!(c.hazard_rate(d(2026, 11, 1)).is_err());
    }

    #[test]
    fn invalid_inputs_are_errors() {
        assert!(HazardCurve::new(d(2026, 11, 2), act365(), vec![], Vec::<f64>::new()).is_err());
        assert!(bootstrap_hazard_curve(&curve_spec(CdsEngine::MidPoint, &[]), &curve(0.02)).is_err());
    }
}
