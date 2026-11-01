//! OIS par-par asset swaps of fixed-rate bullet bonds, their yen-converted form with one basis
//! quote, and the basis of a bond against a bond future.
//!
//! The definitions follow GaussQuant `ois_asset_swap_GQL`, `ois_asset_swap_jpy_from_legs_GQL` and
//! LagrangeFinance II-2.6. The overnight leg's dates are those of QuantLib `MakeOIS` (see `ois.rs`).
//!
//! The asset swap is a function generic over [`Operand`], so that its derivatives with respect to
//! the curve come from automatic differentiation.

use crate::autodiff::{Operand, Recorder};
use crate::curves::YieldCurve;
use crate::instruments::FixedRateBond;
use crate::math::solver1d::brent;
use crate::ois::{overnight_leg, OisCurve, OvernightIndex};
use crate::time::Date;
use crate::Error;

/// The contract terms of an OIS asset swap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OisAssetSwapTerms {
    pub clean_price: f64,
    /// Payments per year of the overnight leg: 1, 2, 4 or 12.
    pub payment_frequency: u32,
    /// Business days from the end of a period to its payment.
    pub payment_lag: u32,
    /// Spread on the overnight leg, as a decimal.
    pub spread: f64,
}

/// The value of an OIS asset swap to the side that pays the bond and receives the overnight leg.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OisAssetSwapValue {
    pub effective_date: Date,
    pub maturity_date: Date,
    pub accrued: f64,
    pub dirty_price: f64,
    pub nominal: f64,
    /// The spread that makes the value 0, as a decimal.
    pub fair_spread: f64,
    pub spread_annuity: f64,
    pub npv: f64,
    pub bond_leg_pv: f64,
    pub overnight_leg_pv: f64,
    pub principal_pv: f64,
    pub upfront_pv: f64,
    /// The clean price at which the contract spread has a value of 0.
    pub fair_clean_price: f64,
}

struct Legs<T> {
    accrued: f64,
    dirty: f64,
    nominal: f64,
    start: Date,
    maturity: Date,
    annuity: T,
    bond_pv: T,
    float_pv: T,
    principal_pv: T,
    upfront_pv: T,
    start_discount: T,
}

fn invalid<T>(reason: &str) -> Result<T, Error> {
    Err(Error::InvalidInput { reason: reason.into() })
}

/// The legs at zero spread. The amounts are per the bond's face amount.
fn legs<T: Operand>(
    bond: &FixedRateBond,
    curve: &YieldCurve<T>,
    index: &OvernightIndex,
    terms: &OisAssetSwapTerms,
    evaluation_date: Date,
) -> Result<Legs<T>, Error> {
    let start = bond.settlement_date(evaluation_date)?;
    let maturity = *bond.schedule.dates.last().expect("a schedule has dates");
    let nominal = bond.notional(start);
    if nominal == 0.0 {
        return invalid("no remaining bond cash flows");
    }
    if (bond.spec.redemption - 100.0).abs() > 1e-8 {
        return invalid("only bullet bonds with redemption at 100 are supported");
    }
    let accrued = bond.accrued_amount(start)?;
    let dirty = terms.clean_price + accrued;

    let periods = overnight_leg(index, start, maturity, terms.payment_frequency, terms.payment_lag)?;
    let start_discount = curve.discount(start)?;
    let zero = start_discount.like(0.0);
    let (mut float_pv, mut annuity) = (zero, zero);
    for p in &periods {
        let pay = curve.discount(p.overnight_payment)?;
        let compound = curve.discount(p.start)? / curve.discount(p.end)?;
        float_pv = float_pv + (compound - 1.0) * nominal * pay;
        annuity = annuity + pay * (nominal * p.accrual);
    }
    let last_pay = periods.last().expect("a leg has periods").overnight_payment;
    let principal_pv = curve.discount(last_pay)? * nominal;
    let upfront_pv = start_discount * (nominal * (dirty / 100.0 - 1.0));
    let mut bond_pv = zero;
    for cf in bond.cashflows.iter().filter(|cf| cf.date() > start) {
        bond_pv = bond_pv + curve.discount(cf.date())? * cf.amount()?;
    }
    if annuity.primal() <= 0.0 {
        return invalid("invalid spread annuity");
    }
    Ok(Legs {
        accrued,
        dirty,
        nominal,
        start,
        maturity,
        annuity,
        bond_pv,
        float_pv,
        principal_pv,
        upfront_pv,
        start_discount,
    })
}

/// The fair spread of an asset swap as a function of the curve.
fn fair_spread_of<T: Operand>(l: &Legs<T>) -> T {
    let npv_zero = l.float_pv + l.principal_pv + l.upfront_pv - l.bond_pv;
    -npv_zero / l.annuity
}

/// Values an OIS asset swap: pay the bond, receive the overnight leg plus the spread, and exchange
/// the difference between the dirty price and par at the start. The curve forecasts and discounts.
pub fn ois_asset_swap(
    bond: &FixedRateBond,
    curve: &YieldCurve,
    index: &OvernightIndex,
    terms: &OisAssetSwapTerms,
    evaluation_date: Date,
) -> Result<OisAssetSwapValue, Error> {
    let l = legs(bond, curve, index, terms, evaluation_date)?;
    let npv_zero = l.float_pv + l.principal_pv + l.upfront_pv - l.bond_pv;
    let npv = npv_zero + terms.spread * l.annuity;
    Ok(OisAssetSwapValue {
        effective_date: l.start,
        maturity_date: l.maturity,
        accrued: l.accrued,
        dirty_price: l.dirty,
        nominal: l.nominal,
        fair_spread: fair_spread_of(&l),
        spread_annuity: l.annuity,
        npv,
        bond_leg_pv: -l.bond_pv,
        overnight_leg_pv: l.float_pv + terms.spread * l.annuity,
        principal_pv: l.principal_pv,
        upfront_pv: l.upfront_pv,
        fair_clean_price: terms.clean_price - npv / (l.nominal * l.start_discount / 100.0),
    })
}

/// The derivative of a fair spread with respect to one quote of the curve, in the same units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AssetSwapQuoteSensitivity {
    /// Position in `OisCurveSpec::instruments`.
    pub instrument: usize,
    pub pillar_date: Date,
    pub quote: f64,
    pub d_fair_spread: f64,
}

/// How the fair spread moves with each quote of the curve, by automatic differentiation on the
/// nodes and the implicit function theorem at the solved curve (see [`OisCurve::quote_jacobian`]).
pub fn asset_swap_quote_sensitivity(
    bond: &FixedRateBond,
    ois: &OisCurve,
    terms: &OisAssetSwapTerms,
    evaluation_date: Date,
) -> Result<Vec<AssetSwapQuoteSensitivity>, Error> {
    let recorder = Recorder::new();
    let lifted = ois.curve.lift(&recorder)?;
    let l = legs(bond, &lifted.curve, &ois.spec.index, terms, evaluation_date)?;
    let vars: Vec<_> = lifted.inputs.iter().map(|(_, v)| *v).collect();
    let node = recorder.gradient(fair_spread_of(&l), &vars)?;
    let jacobian = ois.quote_jacobian()?;
    let n = jacobian.instruments.len();
    if node.len() != n {
        return invalid("the curve's nodes do not match its instruments");
    }
    Ok((0..n)
        .map(|k| AssetSwapQuoteSensitivity {
            instrument: jacobian.instruments[k],
            pillar_date: jacobian.node_dates[k],
            quote: ois.quote_repricing[jacobian.instruments[k]].rate,
            d_fair_spread: (0..n)
                .map(|i| node[i].mean() * jacobian.d_log_discount_d_quote[i][k])
                .sum(),
        })
        .collect())
}

/// The market inputs of a yen-converted asset swap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YenBasisTerms {
    /// Yen per unit of the foreign currency, for exchange on the effective date.
    pub fx_spot: f64,
    /// Cross-currency basis on the yen leg, as a decimal.
    pub basis: f64,
    /// Contract spread on the yen leg, as a decimal.
    pub spread: f64,
}

/// A yen-converted OIS asset swap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YenAssetSwapValue {
    pub native_fair_spread: f64,
    pub yen_fair_spread: f64,
    /// `native_fair_spread + basis`, for comparison.
    pub simple_sum: f64,
    pub npv: f64,
    pub hedged_bond_pv: f64,
    pub yen_base_leg_pv: f64,
    pub upfront_pv: f64,
    pub spread_annuity: f64,
    /// The constant spread on the foreign discounting that makes the basis swap par.
    pub calibrated_discount_spread: f64,
}

/// A foreign bond swapped into yen, with one basis quote (the simplified model of GaussQuant
/// `ois_asset_swap_jpy_from_legs_GQL`): yen collateral, constant notionals, and an FX forward
/// `S [Pf(t)/Pf(t0)] / [Pj(t)/Pj(t0)] exp(-z (t - t0))`, where `z` makes the maturity swap par.
/// There is no basis term structure, no notional reset, and no credit or funding adjustment.
#[allow(clippy::too_many_arguments)]
pub fn yen_asset_swap(
    bond: &FixedRateBond,
    foreign_curve: &YieldCurve,
    foreign_index: &OvernightIndex,
    foreign_terms: &OisAssetSwapTerms,
    yen_curve: &YieldCurve,
    yen_index: &OvernightIndex,
    yen_terms: &OisAssetSwapTerms,
    basis_terms: &YenBasisTerms,
    evaluation_date: Date,
) -> Result<YenAssetSwapValue, Error> {
    let YenBasisTerms { fx_spot, basis, spread } = *basis_terms;
    if fx_spot <= 0.0 || fx_spot.is_nan() {
        return invalid("the FX spot must be positive");
    }
    let foreign = ois_asset_swap(bond, foreign_curve, foreign_index, foreign_terms, evaluation_date)?;
    let yen = ois_asset_swap(bond, yen_curve, yen_index, yen_terms, evaluation_date)?;
    let start = foreign.effective_date;
    if start < evaluation_date {
        return invalid("the effective date precedes the evaluation date");
    }
    let years = |d: Date| (d.serial() - start.serial()) as f64 / 365.0;
    let foreign_start = foreign_curve.discount(start)?;
    let yen_start = yen_curve.discount(start)?;

    let base_flows = |curve: &YieldCurve,
                      index: &OvernightIndex,
                      terms: &OisAssetSwapTerms|
     -> Result<Vec<(Date, f64)>, Error> {
        let periods = overnight_leg(index, start, foreign.maturity_date, terms.payment_frequency, terms.payment_lag)?;
        let mut flows = Vec::with_capacity(periods.len() + 1);
        for p in &periods {
            let compound = curve.discount(p.start)? / curve.discount(p.end)?;
            flows.push((p.overnight_payment, (compound - 1.0) * foreign.nominal));
        }
        let last = flows.last().expect("a leg has periods").0;
        flows.push((last, foreign.nominal));
        Ok(flows)
    };
    let foreign_base = base_flows(foreign_curve, foreign_index, foreign_terms)?;
    let yen_base = base_flows(yen_curve, yen_index, yen_terms)?;
    let mut yen_base_value = 0.0;
    for (d, amount) in &yen_base {
        yen_base_value += amount * yen_curve.discount(*d)?;
    }
    let base_jpy_pv = fx_spot * yen_base_value;
    let jpy_annuity = fx_spot * yen.spread_annuity;

    let objective = |z: f64| -> Result<f64, Error> {
        let mut sum = 0.0;
        for (d, amount) in &foreign_base {
            sum += amount * foreign_curve.discount(*d)? / foreign_start * (-z * years(*d)).exp();
        }
        Ok((fx_spot * yen_start * sum - base_jpy_pv - basis * jpy_annuity) / (fx_spot * foreign.nominal))
    };
    let z = brent(objective, 1e-12, 0.0, -1.0, 1.0, 200)?;

    let mut bond_pv = 0.0;
    for cf in bond.cashflows.iter().filter(|cf| cf.date() > start) {
        let d = cf.date();
        let forward = fx_spot * (foreign_curve.discount(d)? / foreign_start)
            / (yen_curve.discount(d)? / yen_start)
            * (-z * years(d)).exp();
        bond_pv += cf.amount()? * forward * yen_curve.discount(d)?;
    }
    let upfront_pv = fx_spot * yen.upfront_pv;
    let fair = (bond_pv - base_jpy_pv - upfront_pv) / jpy_annuity;
    Ok(YenAssetSwapValue {
        native_fair_spread: foreign.fair_spread,
        yen_fair_spread: fair,
        simple_sum: foreign.fair_spread + basis,
        npv: base_jpy_pv + upfront_pv + spread * jpy_annuity - bond_pv,
        hedged_bond_pv: bond_pv,
        yen_base_leg_pv: base_jpy_pv,
        upfront_pv,
        spread_annuity: jpy_annuity,
        calibrated_discount_spread: z,
    })
}

/// The basis of a deliverable bond against a bond future (LagrangeFinance II-2.6.3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FutureBasis {
    pub accrued_start: f64,
    pub accrued_end: f64,
    pub coupon_income: f64,
    pub futures_invoice: f64,
    pub gross_basis: f64,
    pub funding_cost: f64,
    pub carry: f64,
    pub net_basis: f64,
    pub implied_repo: f64,
}

/// The inputs of [`future_basis`]. The conversion factor is an input: it is set by the exchange.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FutureBasisTerms {
    pub clean_price: f64,
    pub futures_price: f64,
    pub conversion_factor: f64,
    /// Simple rate on Actual/365 from the valuation date to the delivery date.
    pub funding_rate: f64,
    pub valuation_date: Date,
    pub delivery_date: Date,
}

/// Gross basis, carry, net basis and implied repo of one bond. Among the bonds of a basket, the
/// cheapest to deliver has the smallest net basis under the same inputs.
pub fn future_basis(bond: &FixedRateBond, terms: &FutureBasisTerms) -> Result<FutureBasis, Error> {
    let FutureBasisTerms { clean_price, futures_price, conversion_factor, funding_rate, valuation_date, delivery_date } = *terms;
    if delivery_date <= valuation_date {
        return invalid("the delivery date must follow the valuation date");
    }
    let years = (delivery_date.serial() - valuation_date.serial()) as f64 / 365.0;
    let accrued_start = bond.accrued_amount(valuation_date)?;
    let accrued_end = bond.accrued_amount(delivery_date)?;
    let mut coupon_income = 0.0;
    for cf in &bond.cashflows {
        if cf.date() > valuation_date && cf.date() <= delivery_date {
            coupon_income += cf.amount()?;
        }
    }
    let dirty = clean_price + accrued_start;
    let futures_invoice = futures_price * conversion_factor;
    let gross_basis = clean_price - futures_invoice;
    let funding_cost = dirty * funding_rate * years;
    let carry = coupon_income + accrued_end - accrued_start - funding_cost;
    Ok(FutureBasis {
        accrued_start,
        accrued_end,
        coupon_income,
        futures_invoice,
        gross_basis,
        funding_cost,
        carry,
        net_basis: gross_basis - carry,
        implied_repo: (futures_invoice + accrued_end + coupon_income - dirty) / dirty / years,
    })
}
