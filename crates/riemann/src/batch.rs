//! Many evaluations at once, in parallel (specification, section 3.2).
//!
//! Each unit is a value and each result depends on its own unit only, so the results do not
//! depend on the number of threads, their timing or the order of the units. No language API is
//! called here: an entry point converts its inputs first and its results afterwards.

use crate::cashflow_analysis::{DurationType, YieldConvention};
use crate::instruments::{FixedRateBond, FixedRateBondSpec};
use crate::interest_rate::Compounding;
use crate::ois::{build_ois_curve, OisCurveSpec, QuoteRepricing};
use crate::time::{Date, DayCount};
use crate::Error;
use rayon::prelude::*;

/// Applies `f` to each item and keeps the order of the items. 0 threads means rayon's default,
/// usually one per logical processor.
///
/// Every count runs on worker threads, also 1. The floating-point state of a calling thread can
/// differ from that of a new thread: on Windows, R's main thread has x87 extended precision and
/// a new thread double precision, and the mathematical functions of the GNU build depend on it.
/// Computing on the calling thread for 1 would make the last bits depend on the thread count.
pub fn map_parallel<T, U, F>(items: &[T], threads: usize, f: F) -> Result<Vec<U>, Error>
where
    T: Sync,
    U: Send,
    F: Fn(&T) -> U + Send + Sync,
{
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .map_err(|e| Error::InvalidInput {
            reason: format!("cannot start {threads} threads: {e}"),
        })?;
    Ok(pool.install(|| items.par_iter().map(f).collect()))
}

/// One bond at one yield on one evaluation date.
#[derive(Debug, Clone, PartialEq)]
pub struct BondEvaluation {
    pub spec: FixedRateBondSpec,
    pub rate: f64,
    pub convention: YieldConvention,
    pub evaluation_date: Date,
}

/// The measures of one [`BondEvaluation`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BondValues {
    pub settlement_date: Date,
    pub clean_price: f64,
    pub dirty_price: f64,
    pub accrued_amount: f64,
    pub duration_modified: f64,
    pub convexity: f64,
    pub basis_point_value: f64,
    pub yield_value_basis_point: f64,
}

fn evaluate_bond(unit: &BondEvaluation) -> Result<BondValues, Error> {
    let bond = FixedRateBond::new(unit.spec.clone())?;
    let settlement = bond.settlement_date(unit.evaluation_date)?;
    let y = unit.convention.rate(unit.rate)?;
    Ok(BondValues {
        settlement_date: settlement,
        clean_price: bond.clean_price(&y, settlement)?,
        dirty_price: bond.dirty_price(&y, settlement)?,
        accrued_amount: bond.accrued_amount(settlement)?,
        duration_modified: bond.duration(&y, DurationType::Modified, settlement)?,
        convexity: bond.convexity(&y, settlement)?,
        basis_point_value: bond.basis_point_value(&y, settlement)?,
        yield_value_basis_point: bond.yield_value_basis_point(&y, settlement)?,
    })
}

/// Values every bond. The outer error is only about starting the threads; a unit that fails has
/// its own error, and the others are not affected.
pub fn bond_values_batch(
    units: &[BondEvaluation],
    threads: usize,
) -> Result<Vec<Result<BondValues, Error>>, Error> {
    map_parallel(units, threads, evaluate_bond)
}

/// One curve to build and the dates to read it on.
#[derive(Debug, Clone, PartialEq)]
pub struct CurveEvaluation {
    pub spec: OisCurveSpec,
    pub dates: Vec<Date>,
    /// The day count of the rates read from the curve.
    pub day_count: DayCount,
}

/// One date read from a curve.
#[derive(Debug, Clone, PartialEq)]
pub struct CurvePoint {
    pub date: Date,
    pub discount: Result<f64, Error>,
    pub zero_continuous: Result<f64, Error>,
    pub forward_1d_simple: Result<f64, Error>,
}

/// The result of one [`CurveEvaluation`].
#[derive(Debug, Clone, PartialEq)]
pub struct CurveValues {
    pub quote_repricing: Vec<QuoteRepricing>,
    pub points: Vec<CurvePoint>,
}

fn evaluate_curve(unit: &CurveEvaluation) -> Result<CurveValues, Error> {
    let built = build_ois_curve(&unit.spec)?;
    let curve = &built.curve;
    let points = unit
        .dates
        .iter()
        .map(|&date| CurvePoint {
            date,
            discount: curve.discount(date),
            zero_continuous: curve
                .zero_rate(date, &unit.day_count, Compounding::Continuous, 1)
                .map(|r| r.rate),
            forward_1d_simple: date.add_days(1).and_then(|next| {
                curve
                    .forward_rate(date, next, &unit.day_count, Compounding::Simple, 1)
                    .map(|r| r.rate)
            }),
        })
        .collect();
    Ok(CurveValues {
        quote_repricing: built.quote_repricing,
        points,
    })
}

/// Builds and reads every curve. The outer error is only about starting the threads; a curve
/// that fails has its own error, and the others are not affected.
pub fn curve_values_batch(
    units: &[CurveEvaluation],
    threads: usize,
) -> Result<Vec<Result<CurveValues, Error>>, Error> {
    map_parallel(units, threads, evaluate_curve)
}
