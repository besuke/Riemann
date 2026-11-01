//! Sensitivities of bonds by automatic differentiation: to the yield, to the inputs of a curve and
//! to the quotes of an OIS curve.

use crate::autodiff::{Operand, Recorder, Value};
use crate::cashflow_analysis as analysis;
use crate::curves::{CurveInput, YieldCurve};
use crate::instruments::{BondCurveValue, FixedRateBond};
use crate::interest_rate::InterestRate;
use crate::ois::OisCurve;
use crate::time::Date;
use crate::Error;

/// A bond's dirty price per 100 and its derivatives with respect to the yield.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YieldSensitivity {
    pub dirty_price: f64,
    pub d_price_d_yield: f64,
    pub d2_price_d_yield2: f64,
}

/// The derivatives of a bond's values with respect to one input of a curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurveInputSensitivity {
    pub input: CurveInput,
    /// Of the present value at the curve's reference date, in currency.
    pub d_npv: f64,
    /// Of the dirty price per 100 of notional at settlement.
    pub d_dirty_price: f64,
}

/// The derivatives of a bond's values with respect to one quote of an OIS curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuoteSensitivity {
    /// Position in `OisCurveSpec::instruments`.
    pub instrument: usize,
    pub pillar_date: Date,
    pub quote: f64,
    pub d_npv: f64,
    pub d_dirty_price: f64,
}

/// First and second derivatives of the dirty price with respect to the yield. Agrees with the
/// duration and convexity formulas where they are the derivatives of the price: compounded and
/// continuous yields (`docs/spec_autodiff.md` 5 章).
pub fn yield_sensitivity(
    bond: &FixedRateBond,
    y: &InterestRate,
    settlement: Date,
) -> Result<YieldSensitivity, Error> {
    let notional = tradable_notional(bond, settlement)?;
    let recorder = Recorder::new();
    let rate = recorder.input(y.rate);
    let lifted = InterestRate::new(rate, y.day_count.clone(), y.compounding, y.frequency)?;
    let price = analysis::npv(&bond.cashflows, &lifted, false, settlement, settlement)? * 100.0 / notional;
    let first = recorder.gradient_vars(price, &[rate])?[0];
    let second = recorder.gradient(first, &[rate])?;
    Ok(YieldSensitivity {
        dirty_price: price.primal(),
        d_price_d_yield: first.primal(),
        d2_price_d_yield2: second[0].mean(),
    })
}

fn tradable_notional(bond: &FixedRateBond, settlement: Date) -> Result<f64, Error> {
    let notional = bond.notional(settlement);
    if notional == 0.0 {
        return Err(Error::InvalidInput {
            reason: format!("non tradable at {settlement}"),
        });
    }
    Ok(notional)
}

/// The bond's value on `curve` and its derivatives with respect to each input of the curve, in the
/// same terms as [`FixedRateBond::value_on_curve`].
pub fn curve_sensitivity(
    bond: &FixedRateBond,
    curve: &YieldCurve,
    evaluation_date: Date,
) -> Result<(BondCurveValue, Vec<CurveInputSensitivity>), Error> {
    let value = bond.value_on_curve(curve, evaluation_date)?;
    let recorder = Recorder::new();
    let lifted = curve.lift(&recorder)?;
    let vars: Vec<_> = lifted.inputs.iter().map(|(_, v)| *v).collect();
    let expired = bond
        .cashflows
        .iter()
        .all(|cf| cf.has_occurred(evaluation_date, false));
    if expired {
        let zero = lifted.inputs.iter().map(|(input, _)| CurveInputSensitivity {
            input: *input,
            d_npv: 0.0,
            d_dirty_price: 0.0,
        });
        return Ok((value, zero.collect()));
    }
    let settlement = bond.settlement_date(evaluation_date)?;
    let valuation_date = curve.reference_date();
    let npv = analysis::npv_on_curve(
        &bond.cashflows,
        &lifted.curve,
        false,
        valuation_date,
        valuation_date,
    )?;
    let settlement_value = analysis::npv_on_curve(
        &bond.cashflows,
        &lifted.curve,
        false,
        settlement,
        settlement,
    )?;
    let notional = bond.notional(settlement);
    let price_scale = if notional == 0.0 { 0.0 } else { 100.0 / notional };
    let d_npv = recorder.gradient(npv, &vars)?;
    let d_settlement = recorder.gradient(settlement_value, &vars)?;
    let rows = lifted
        .inputs
        .iter()
        .zip(d_npv.iter().zip(&d_settlement))
        .map(|((input, _), (n, s)): (_, (&Value, &Value))| CurveInputSensitivity {
            input: *input,
            d_npv: n.mean(),
            d_dirty_price: s.mean() * price_scale,
        })
        .collect();
    Ok((value, rows))
}

/// The bond's value on an OIS curve and its derivatives with respect to the quotes, with the
/// curve rebuilt in the limit: the derivatives on the nodes times [`OisCurve::quote_jacobian`].
/// Instruments dropped from the bootstrap have no row.
pub fn quote_sensitivity(
    bond: &FixedRateBond,
    ois: &OisCurve,
    evaluation_date: Date,
) -> Result<(BondCurveValue, Vec<QuoteSensitivity>), Error> {
    let (value, nodes) = curve_sensitivity(bond, &ois.curve, evaluation_date)?;
    let jacobian = ois.quote_jacobian()?;
    let n = jacobian.instruments.len();
    if nodes.len() != n {
        return Err(Error::InvalidInput {
            reason: "the curve's nodes do not match its instruments".into(),
        });
    }
    let rows = (0..n)
        .map(|k| {
            let instrument = jacobian.instruments[k];
            let along = |f: fn(&CurveInputSensitivity) -> f64| -> f64 {
                (0..n)
                    .map(|i| f(&nodes[i]) * jacobian.d_log_discount_d_quote[i][k])
                    .sum()
            };
            QuoteSensitivity {
                instrument,
                pillar_date: jacobian.node_dates[k],
                quote: ois.quote_repricing[instrument].rate,
                d_npv: along(|s| s.d_npv),
                d_dirty_price: along(|s| s.d_dirty_price),
            }
        })
        .collect();
    Ok((value, rows))
}
