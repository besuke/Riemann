//! Fixed-rate bonds (QuantLib-compatible `FixedRateBond`, single redemption).
//!
//! Construction, notional, settlement date and accrued interest reproduce
//! `ql/instruments/bonds/fixedratebond.cpp`, `ql/instruments/bond.cpp` and
//! `ql/pricingengines/bond/bondfunctions.cpp` (accrued amount), and are treated as derived
//! from QuantLib source code (modified BSD license). QuantLib commit
//! 70f240f29ddb84927ff654502f1d75b3de24ba43. See THIRD_PARTY_NOTICES.
//!
//! Valuation on a curve (`value_on_curve`) reproduces QuantLib v1.42
//! `ql/pricingengines/bond/discountingbondengine.cpp` (Copyright (C) 2007 Giorgio
//! Facchinetti; (C) 2009 StatPro Italia srl) with `Bond::isExpired/dirtyPrice/cleanPrice`.
//!
//! Differences from QuantLib: the evaluation date is an argument (no global settings);
//! a single coupon rate and notional; no amortization, ex-coupon period or payment lag.

use crate::cashflow_analysis::{self as analysis, DurationType, YieldConvention, YieldSolver};
use crate::cashflows::{self, CashFlow, FixedRateLegSpec};
use crate::curves::YieldCurve;
use crate::interest_rate::{Compounding, InterestRate};
use crate::math::solver1d::brent;
use crate::time::{
    generate_schedule, BusinessDayConvention, Date, DayCount, HolidayCalendar, Period, Schedule,
    ScheduleSpec, TimeUnit,
};
use crate::Error;

/// Everything needed to build a fixed-rate bond.
#[derive(Debug, Clone, PartialEq)]
pub struct FixedRateBondSpec {
    pub settlement_days: u32,
    pub face_amount: f64,
    pub schedule: ScheduleSpec,
    pub coupon_rate: f64,
    pub day_count: DayCount,
    pub payment_convention: BusinessDayConvention,
    /// Redemption per 100 of notional.
    pub redemption: f64,
    pub issue_date: Option<Date>,
    /// Defaults to the schedule's calendar.
    pub payment_calendar: Option<HolidayCalendar>,
}

/// A built fixed-rate bond: its inputs, schedule and cash flows (coupons, then redemption).
#[derive(Debug, Clone, PartialEq)]
pub struct FixedRateBond {
    pub spec: FixedRateBondSpec,
    pub schedule: Schedule,
    pub cashflows: Vec<CashFlow>,
    /// Payment date of the last coupon; the notional is repaid on this date.
    pub maturity_payment_date: Date,
}

impl FixedRateBond {
    /// Builds the bond (QuantLib `FixedRateBond` constructor).
    pub fn new(spec: FixedRateBondSpec) -> Result<FixedRateBond, Error> {
        let schedule = generate_schedule(&spec.schedule)?;
        let payment_calendar = spec.payment_calendar.clone().unwrap_or_else(|| spec.schedule.calendar.clone());
        let leg = FixedRateLegSpec {
            notional: spec.face_amount,
            rate: spec.coupon_rate,
            day_count: spec.day_count.clone(),
            payment_calendar,
            payment_convention: spec.payment_convention,
        };
        let mut cashflows = cashflows::fixed_rate_leg(&spec.schedule, &schedule, &leg)?;
        // the notional is repaid on the payment date of the last coupon
        let maturity_payment_date = cashflows.last().expect("at least one coupon").date();
        cashflows.push(CashFlow::Redemption {
            date: maturity_payment_date,
            amount: (spec.redemption / 100.0) * (spec.face_amount - 0.0),
        });
        // stable sort by date: redemptions follow coupons paid on the same date
        cashflows.sort_by_key(|cf| cf.date());
        Ok(FixedRateBond { spec, schedule, cashflows, maturity_payment_date })
    }

    fn payment_calendar(&self) -> &HolidayCalendar {
        self.spec.payment_calendar.as_ref().unwrap_or(&self.spec.schedule.calendar)
    }

    /// Settlement date for trades on `evaluation_date`: T + settlement days, not before issue.
    pub fn settlement_date(&self, evaluation_date: Date) -> Result<Date, Error> {
        let settlement = self.payment_calendar().advance(
            evaluation_date,
            Period::new(self.spec.settlement_days as i32, TimeUnit::Days),
            BusinessDayConvention::Following,
            false,
        )?;
        Ok(match self.spec.issue_date {
            Some(issue) => settlement.max(issue),
            None => settlement,
        })
    }

    /// Outstanding notional at `d` (QuantLib `Bond::notional`); zero from the redemption date on.
    pub fn notional(&self, d: Date) -> f64 {
        if d > self.maturity_payment_date {
            return 0.0;
        }
        if d < self.maturity_payment_date {
            self.spec.face_amount
        } else {
            // on the redemption date the payment has occurred and the notional changed
            0.0
        }
    }

    /// Accrued interest per 100 of notional at `settlement` (QuantLib `BondFunctions::accruedAmount`).
    pub fn accrued_amount(&self, settlement: Date) -> Result<f64, Error> {
        let notional = self.notional(settlement);
        if notional == 0.0 {
            return Ok(0.0);
        }
        Ok(cashflows::accrued_amount(&self.cashflows, settlement, false)? * 100.0 / notional)
    }

    fn tradable_notional(&self, settlement: Date) -> Result<f64, Error> {
        let notional = self.notional(settlement);
        if notional == 0.0 {
            return Err(Error::InvalidInput {
                reason: format!("non tradable at {settlement} (maturity being {})", self.maturity_payment_date),
            });
        }
        Ok(notional)
    }

    /// Dirty price per 100 at yield `y` (QuantLib `BondFunctions::dirtyPrice`).
    pub fn dirty_price(&self, y: &InterestRate, settlement: Date) -> Result<f64, Error> {
        let notional = self.tradable_notional(settlement)?;
        Ok(analysis::npv(&self.cashflows, y, false, settlement, settlement)? * 100.0 / notional)
    }

    /// Clean price per 100 at yield `y` (QuantLib `BondFunctions::cleanPrice`).
    pub fn clean_price(&self, y: &InterestRate, settlement: Date) -> Result<f64, Error> {
        Ok(self.dirty_price(y, settlement)? - self.accrued_amount(settlement)?)
    }

    /// Yield giving `price` (QuantLib `BondFunctions::yield` with NewtonSafe).
    pub fn yield_from_price(
        &self,
        price: f64,
        price_type: PriceType,
        convention: &YieldConvention,
        settlement: Date,
        solver: YieldSolver,
    ) -> Result<f64, Error> {
        let notional = self.tradable_notional(settlement)?;
        let mut amount = price;
        if price_type == PriceType::Clean {
            amount += self.accrued_amount(settlement)?;
        }
        amount /= 100.0 / notional;
        analysis::irr(&self.cashflows, amount, convention, false, settlement, settlement, solver)
    }

    /// Duration at yield `y` (QuantLib `BondFunctions::duration`).
    pub fn duration(&self, y: &InterestRate, kind: DurationType, settlement: Date) -> Result<f64, Error> {
        self.tradable_notional(settlement)?;
        analysis::duration(&self.cashflows, y, kind, false, settlement, settlement)
    }

    /// Convexity at yield `y` (QuantLib `BondFunctions::convexity`).
    pub fn convexity(&self, y: &InterestRate, settlement: Date) -> Result<f64, Error> {
        self.tradable_notional(settlement)?;
        analysis::convexity(&self.cashflows, y, false, settlement, settlement)
    }

    /// Basis point value at yield `y`, for the whole notional (QuantLib `BondFunctions::basisPointValue`).
    pub fn basis_point_value(&self, y: &InterestRate, settlement: Date) -> Result<f64, Error> {
        self.tradable_notional(settlement)?;
        analysis::basis_point_value(&self.cashflows, y, false, settlement, settlement)
    }

    /// Yield value of a basis point at yield `y` (QuantLib `BondFunctions::yieldValueBasisPoint`).
    pub fn yield_value_basis_point(&self, y: &InterestRate, settlement: Date) -> Result<f64, Error> {
        self.tradable_notional(settlement)?;
        analysis::yield_value_basis_point(&self.cashflows, y, false, settlement, settlement)
    }

    /// Value on a discount curve for trades on `evaluation_date` (QuantLib v1.42
    /// `DiscountingBondEngine` with `Bond::NPV`, `dirtyPrice` and `cleanPrice`). The NPV is
    /// at the curve's reference date; the prices are per 100 of notional at settlement.
    /// Cash flows on the reference or settlement date are not included.
    pub fn value_on_curve(
        &self,
        curve: &YieldCurve,
        evaluation_date: Date,
    ) -> Result<BondCurveValue, Error> {
        let settlement = self.settlement_date(evaluation_date)?;
        // QuantLib Bond::isExpired: every cash flow has occurred at the evaluation date
        if self
            .cashflows
            .iter()
            .all(|cf| cf.has_occurred(evaluation_date, false))
        {
            return Ok(BondCurveValue {
                npv: 0.0,
                settlement_value: 0.0,
                dirty_price: 0.0,
                clean_price: 0.0,
            });
        }
        let valuation_date = curve.reference_date();
        let npv = analysis::npv_on_curve(
            &self.cashflows,
            curve,
            false,
            valuation_date,
            valuation_date,
        )?;
        let settlement_value = if valuation_date == settlement {
            npv
        } else {
            analysis::npv_on_curve(&self.cashflows, curve, false, settlement, settlement)?
        };
        let notional = self.notional(settlement);
        let dirty_price = if notional == 0.0 {
            0.0
        } else {
            settlement_value * 100.0 / notional
        };
        let clean_price = dirty_price - self.accrued_amount(settlement)?;
        Ok(BondCurveValue {
            npv,
            settlement_value,
            dirty_price,
            clean_price,
        })
    }

    /// The value on `curve` with `spread` added to its zero rates (QuantLib
    /// `ZeroSpreadedTermStructure` with `DiscountingBondEngine`). The spread's rate uses the
    /// curve's day count.
    pub fn value_with_zero_spread(
        &self,
        curve: &YieldCurve,
        spread: f64,
        compounding: Compounding,
        frequency: u32,
        evaluation_date: Date,
    ) -> Result<BondCurveValue, Error> {
        let spreaded = YieldCurve::zero_spreaded(curve.clone(), spread, compounding, frequency)?
            .with_extrapolation(curve.allows_extrapolation());
        self.value_on_curve(&spreaded, evaluation_date)
    }

    /// The zero spread that reprices `price` at `settlement`. The search widens from `guess`
    /// until it brackets a root, and then runs Brent's method. Written without QuantLib's source
    /// and checked by results against `BondFunctions_zSpread` of its R package.
    #[allow(clippy::too_many_arguments)]
    pub fn z_spread_from_price(
        &self,
        curve: &YieldCurve,
        price: f64,
        price_type: PriceType,
        compounding: Compounding,
        frequency: u32,
        settlement: Date,
        accuracy: f64,
        max_evaluations: usize,
        guess: f64,
    ) -> Result<f64, Error> {
        let notional = self.notional(settlement);
        if notional == 0.0 {
            return Err(Error::InvalidInput { reason: "the bond has no notional at settlement".into() });
        }
        let dirty = match price_type {
            PriceType::Clean => price + self.accrued_amount(settlement)?,
            PriceType::Dirty => price,
        };
        let target = dirty * notional / 100.0;
        let objective = |spread: f64| -> Result<f64, Error> {
            let spreaded = YieldCurve::zero_spreaded(curve.clone(), spread, compounding, frequency)?
            .with_extrapolation(curve.allows_extrapolation());
            Ok(analysis::npv_on_curve(&self.cashflows, &spreaded, false, settlement, settlement)? - target)
        };
        let step = 0.01;
        let (mut low, mut high) = (guess - step, guess + step);
        let (mut f_low, mut f_high) = (objective(low)?, objective(high)?);
        let mut evaluations = 2;
        while f_low * f_high > 0.0 {
            if evaluations >= max_evaluations {
                return Err(Error::NoConvergence {
                    reason: format!("no zero spread brackets the price {price} in {max_evaluations} evaluations"),
                });
            }
            if f_low.abs() < f_high.abs() {
                low -= 1.6 * (high - low);
                f_low = objective(low)?;
            } else {
                high += 1.6 * (high - low);
                f_high = objective(high)?;
            }
            evaluations += 1;
        }
        brent(objective, accuracy, guess.clamp(low, high), low, high, max_evaluations)
    }
}

/// A bond's value on a discount curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BondCurveValue {
    /// Present value at the curve's reference date, in currency.
    pub npv: f64,
    /// Present value at the settlement date, in currency.
    pub settlement_value: f64,
    /// Per 100 of notional.
    pub dirty_price: f64,
    /// Per 100 of notional.
    pub clean_price: f64,
}

/// Whether a quoted price includes accrued interest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PriceType {
    Clean,
    Dirty,
}
