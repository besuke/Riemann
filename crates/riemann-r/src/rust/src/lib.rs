//! R entry point of Riemann. Only conversions are here: the calculations are in the `riemann`
//! crate. Dates are QuantLib serial numbers; `R/dates.R` converts them from and to R dates.
//! Vector functions return their results as named columns, with NA and a message in `error`
//! where a row fails, so that R decides whether to stop.
//!
//! Expected errors of inputs and calculations are returned as values: a list with `failure`
//! holds the message, and `R/common.R` turns it into an R error. They do not use the `Err` path
//! of extendr, which unwinds through a panic. A panic therefore always means a defect here or in
//! the core; the process's panic hook is left alone, so it stays visible and can be diagnosed.

use extendr_api::prelude::*;
use riemann::asset_swap::{self, FutureBasisTerms, OisAssetSwapTerms, YenBasisTerms};
use riemann::batch::{self, BondEvaluation, CurveEvaluation};
use riemann::cashflow_analysis::{YieldConvention, YieldSolver};
use riemann::cds::{self, Cds, CdsCurve, CdsCurveSpec, CdsSpec, CdsValue, HazardCurve, SpreadQuote};
use riemann::hull_white::{self, CallableFlow, GridSettings, HullWhite, Quadrature, SwaptionFlows};
use riemann::instruments::{FixedRateBond, FixedRateBondSpec};
use riemann::interest_rate::Compounding;
use riemann::curves::CurveInput;
use riemann::names;
use riemann::ois::{OisCurve, OisCurveInstrument, OisCurveSpec};
use riemann::options::{self, Greeks, MonteCarloSettings, OptionInputs};
use riemann::risk;
use riemann::sabr::{self, SabrParameters};
use riemann::time::{Date, HolidayCalendar, ScheduleSpec};
use riemann::yen_bond;

type Core<T> = std::result::Result<T, riemann::Error>;

fn failure(e: riemann::Error) -> List {
    list!(failure = e.to_string())
}

/// A list of the results, or the failure of the whole call.
fn outcome(result: Core<List>) -> List {
    result.unwrap_or_else(failure)
}

fn date(serial: i32) -> Core<Date> {
    Date::from_serial(serial as i64)
}

/// 0 stands for no date.
fn optional_date(serial: i32) -> Core<Option<Date>> {
    if serial == 0 {
        Ok(None)
    } else {
        date(serial).map(Some)
    }
}

/// A logical column crosses the border as integers. R has checked it; anything but 0 and 1 (NA
/// included) is a failure here too, and never reads as false.
fn flag(value: i32) -> Core<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(riemann::Error::InvalidInput {
            reason: format!("a logical value must be TRUE or FALSE, not {other}"),
        }),
    }
}

fn na_real() -> f64 {
    <f64 as CanBeNA>::na()
}

/// Splits row results into values, with NA for errors, and messages, empty for values.
fn split_rows(rows: Vec<Core<f64>>) -> (Vec<f64>, Vec<String>) {
    let mut values = Vec::with_capacity(rows.len());
    let mut errors = Vec::with_capacity(rows.len());
    for row in rows {
        match row {
            Ok(v) => {
                values.push(v);
                errors.push(String::new());
            }
            Err(e) => {
                values.push(na_real());
                errors.push(e.to_string());
            }
        }
    }
    (values, errors)
}

fn value_list(rows: Vec<Core<f64>>) -> List {
    let (value, error) = split_rows(rows);
    list!(value = value, error = error)
}

fn yield_convention(day_count: &str, compounding: &str, frequency: i32) -> Core<YieldConvention> {
    Ok(YieldConvention {
        day_count: names::day_count(day_count)?,
        compounding: names::compounding(compounding)?,
        frequency: frequency as u32,
    })
}

// ---- bonds ----------------------------------------------------------------------------------

#[extendr]
#[allow(clippy::too_many_arguments)]
fn fixed_rate_bond(
    settlement_days: i32,
    face_amount: f64,
    effective_date: i32,
    termination_date: i32,
    tenor: &str,
    calendar: &str,
    convention: &str,
    termination_convention: &str,
    rule: &str,
    end_of_month: bool,
    first_date: i32,
    next_to_last_date: i32,
    coupon_rate: f64,
    day_count: &str,
    payment_convention: &str,
    redemption: f64,
    issue_date: i32,
) -> List {
    let build = || -> Core<FixedRateBond> {
        FixedRateBond::new(FixedRateBondSpec {
            settlement_days: settlement_days as u32,
            face_amount,
            schedule: ScheduleSpec {
                effective_date: date(effective_date)?,
                termination_date: date(termination_date)?,
                tenor: names::period(tenor)?,
                calendar: HolidayCalendar::from(names::calendar(calendar)?),
                convention: names::business_day_convention(convention)?,
                termination_convention: names::business_day_convention(termination_convention)?,
                rule: names::date_generation_rule(rule)?,
                end_of_month,
                first_date: optional_date(first_date)?,
                next_to_last_date: optional_date(next_to_last_date)?,
            },
            coupon_rate,
            day_count: names::day_count(day_count)?,
            payment_convention: names::business_day_convention(payment_convention)?,
            redemption,
            issue_date: optional_date(issue_date)?,
            payment_calendar: None,
        })
    };
    outcome(build().map(|bond| list!(value = ExternalPtr::new(bond))))
}

#[extendr]
fn settlement_date(bond: ExternalPtr<FixedRateBond>, evaluation_date: Vec<i32>) -> List {
    let rows = evaluation_date
        .iter()
        .map(|&d| Ok(bond.settlement_date(date(d)?)?.serial() as f64))
        .collect();
    value_list(rows)
}

/// All measures of a bond at each yield and settlement date.
#[extendr]
fn bond_values(
    bond: ExternalPtr<FixedRateBond>,
    rate: Vec<f64>,
    day_count: &str,
    compounding: &str,
    frequency: i32,
    settlement_date: Vec<i32>,
) -> List {
    outcome(bond_values_rows(
        &bond,
        &rate,
        day_count,
        compounding,
        frequency,
        &settlement_date,
    ))
}

fn bond_values_rows(
    bond: &FixedRateBond,
    rate: &[f64],
    day_count: &str,
    compounding: &str,
    frequency: i32,
    settlement_date: &[i32],
) -> Core<List> {
    let convention = yield_convention(day_count, compounding, frequency)?;
    let n = rate.len();
    let mut columns: [Vec<f64>; 9] = Default::default();
    let mut errors = vec![String::new(); n];
    let kinds = [
        names::duration_type("simple")?,
        names::duration_type("modified")?,
        names::duration_type("macaulay")?,
    ];
    // the Macaulay duration exists only for compounded yields; elsewhere its column is NA, which
    // is not a failure of the row. `duration` reports it as an error when it is asked for.
    let has_macaulay = convention.compounding == Compounding::Compounded;
    for i in 0..n {
        let inputs = date(settlement_date[i]).and_then(|s| Ok((s, convention.rate(rate[i])?)));
        let (settlement, y) = match inputs {
            Ok(values) => values,
            Err(e) => {
                columns.iter_mut().for_each(|c| c.push(na_real()));
                errors[i] = e.to_string();
                continue;
            }
        };
        let values: [Core<f64>; 9] = [
            bond.clean_price(&y, settlement),
            bond.dirty_price(&y, settlement),
            bond.accrued_amount(settlement),
            bond.duration(&y, kinds[0], settlement),
            bond.duration(&y, kinds[1], settlement),
            if has_macaulay {
                bond.duration(&y, kinds[2], settlement)
            } else {
                Ok(na_real())
            },
            bond.convexity(&y, settlement),
            bond.basis_point_value(&y, settlement),
            bond.yield_value_basis_point(&y, settlement),
        ];
        for (column, value) in columns.iter_mut().zip(values) {
            match value {
                Ok(v) => column.push(v),
                Err(e) => {
                    column.push(na_real());
                    if errors[i].is_empty() {
                        errors[i] = e.to_string();
                    }
                }
            }
        }
    }
    let [clean_price, dirty_price, accrued_amount, duration_simple, duration_modified, duration_macaulay, convexity, basis_point_value, yield_value_basis_point] =
        columns;
    Ok(list!(
        clean_price = clean_price,
        dirty_price = dirty_price,
        accrued_amount = accrued_amount,
        duration_simple = duration_simple,
        duration_modified = duration_modified,
        duration_macaulay = duration_macaulay,
        convexity = convexity,
        basis_point_value = basis_point_value,
        yield_value_basis_point = yield_value_basis_point,
        error = errors
    ))
}

/// One duration measure at each yield and settlement date.
#[extendr]
#[allow(clippy::too_many_arguments)]
fn duration(
    bond: ExternalPtr<FixedRateBond>,
    rate: Vec<f64>,
    day_count: &str,
    compounding: &str,
    frequency: i32,
    settlement_date: Vec<i32>,
    duration_type: &str,
) -> List {
    let rows = || -> Core<List> {
        let convention = yield_convention(day_count, compounding, frequency)?;
        let kind = names::duration_type(duration_type)?;
        let rows = rate
            .iter()
            .zip(&settlement_date)
            .map(|(&r, &s)| bond.duration(&convention.rate(r)?, kind, date(s)?))
            .collect();
        Ok(value_list(rows))
    };
    outcome(rows())
}

#[extendr]
#[allow(clippy::too_many_arguments)]
fn yield_from_price(
    bond: ExternalPtr<FixedRateBond>,
    price: Vec<f64>,
    price_type: &str,
    day_count: &str,
    compounding: &str,
    frequency: i32,
    settlement_date: Vec<i32>,
    accuracy: f64,
    max_iterations: i32,
    guess: f64,
) -> List {
    let rows = || -> Core<List> {
        let convention = yield_convention(day_count, compounding, frequency)?;
        let kind = names::price_type(price_type)?;
        let solver = YieldSolver {
            accuracy,
            max_iterations: max_iterations as usize,
            guess,
        };
        let rows = price
            .iter()
            .zip(&settlement_date)
            .map(|(&p, &s)| bond.yield_from_price(p, kind, &convention, date(s)?, solver))
            .collect();
        Ok(value_list(rows))
    };
    outcome(rows())
}

#[extendr]
fn value_on_curve(
    bond: ExternalPtr<FixedRateBond>,
    curve: ExternalPtr<OisCurve>,
    evaluation_date: Vec<i32>,
) -> List {
    let n = evaluation_date.len();
    let (mut npv, mut settlement_value) = (Vec::with_capacity(n), Vec::with_capacity(n));
    let (mut dirty_price, mut clean_price) = (Vec::with_capacity(n), Vec::with_capacity(n));
    let mut errors = Vec::with_capacity(n);
    for &d in &evaluation_date {
        match date(d).and_then(|d| bond.value_on_curve(&curve.curve, d)) {
            Ok(v) => {
                npv.push(v.npv);
                settlement_value.push(v.settlement_value);
                dirty_price.push(v.dirty_price);
                clean_price.push(v.clean_price);
                errors.push(String::new());
            }
            Err(e) => {
                npv.push(na_real());
                settlement_value.push(na_real());
                dirty_price.push(na_real());
                clean_price.push(na_real());
                errors.push(e.to_string());
            }
        }
    }
    list!(
        npv = npv,
        settlement_value = settlement_value,
        dirty_price = dirty_price,
        clean_price = clean_price,
        error = errors
    )
}

// ---- Japanese bond calculations --------------------------------------------------------------

/// NA digits stand for no truncation.
fn optional_digits(digits: i32) -> Option<u32> {
    if digits.is_na() {
        None
    } else {
        Some(digits as u32)
    }
}

#[extendr]
fn remaining_years(start: Vec<i32>, end: Vec<i32>) -> List {
    let rows = start
        .iter()
        .zip(&end)
        .map(|(&a, &b)| yen_bond::remaining_years(date(a)?, date(b)?))
        .collect();
    value_list(rows)
}

#[extendr]
fn price_from_simple_rate(
    t: Vec<f64>,
    coupon_rate: Vec<f64>,
    simple_rate: Vec<f64>,
    redemption: Vec<f64>,
    digits: i32,
) -> List {
    let rows = (0..t.len())
        .map(|i| {
            yen_bond::price_from_simple_rate(
                t[i],
                coupon_rate[i],
                simple_rate[i],
                redemption[i],
                optional_digits(digits),
            )
        })
        .collect();
    value_list(rows)
}

#[extendr]
fn simple_rate_from_price(
    t: Vec<f64>,
    coupon_rate: Vec<f64>,
    price: Vec<f64>,
    redemption: Vec<f64>,
    digits: i32,
    max_iterations: i32,
) -> List {
    let rows = (0..t.len())
        .map(|i| {
            yen_bond::simple_rate_from_price(
                t[i],
                coupon_rate[i],
                price[i],
                redemption[i],
                digits as u32,
                max_iterations as usize,
            )
        })
        .collect();
    value_list(rows)
}

fn frequencies(coupon: i32, compound: i32) -> yen_bond::CompoundFrequencies {
    yen_bond::CompoundFrequencies {
        coupon: coupon as u32,
        compound: compound as u32,
    }
}

#[extendr]
#[allow(clippy::too_many_arguments)]
fn price_from_compound_yield(
    t: Vec<f64>,
    coupon_rate: Vec<f64>,
    compound_yield: Vec<f64>,
    redemption: Vec<f64>,
    coupon_frequency: i32,
    compound_frequency: i32,
    digits: i32,
) -> List {
    let rows = (0..t.len())
        .map(|i| {
            yen_bond::price_from_compound_yield(
                t[i],
                coupon_rate[i],
                compound_yield[i],
                redemption[i],
                frequencies(coupon_frequency, compound_frequency),
                optional_digits(digits),
            )
        })
        .collect();
    value_list(rows)
}

#[extendr]
#[allow(clippy::too_many_arguments)]
fn compound_yield_from_price(
    t: Vec<f64>,
    coupon_rate: Vec<f64>,
    price: Vec<f64>,
    redemption: Vec<f64>,
    coupon_frequency: i32,
    compound_frequency: i32,
    digits: i32,
    max_iterations: i32,
) -> List {
    let rows = (0..t.len())
        .map(|i| {
            yen_bond::compound_yield_from_price(
                t[i],
                coupon_rate[i],
                price[i],
                redemption[i],
                frequencies(coupon_frequency, compound_frequency),
                digits as u32,
                max_iterations as usize,
            )
        })
        .collect();
    value_list(rows)
}

fn risk_list(rows: Vec<Core<yen_bond::YenBondRisk>>) -> List {
    let pick = |f: fn(&yen_bond::YenBondRisk) -> f64| -> Vec<f64> {
        rows.iter()
            .map(|r| r.as_ref().map_or(na_real(), f))
            .collect()
    };
    let errors: Vec<String> = rows
        .iter()
        .map(|r| r.as_ref().err().map_or(String::new(), |e| e.to_string()))
        .collect();
    list!(
        price = pick(|r| r.price),
        accrued_interest = pick(|r| r.accrued_interest),
        dirty_price = pick(|r| r.dirty_price),
        dp_dy_pct = pick(|r| r.dp_dy_pct),
        d2p_dy2_pct = pick(|r| r.d2p_dy2_pct),
        bpv_yen = pick(|r| r.bpv_yen),
        bpv_sen = pick(|r| r.bpv_sen),
        modified_duration = pick(|r| r.modified_duration),
        convexity = pick(|r| r.convexity),
        error = errors
    )
}

#[extendr]
fn risk_from_simple_rate(
    t: Vec<f64>,
    coupon_rate: Vec<f64>,
    simple_rate: Vec<f64>,
    redemption: Vec<f64>,
    accrued_interest: Vec<f64>,
) -> List {
    risk_list(
        (0..t.len())
            .map(|i| {
                yen_bond::risk_from_simple_rate(
                    t[i],
                    coupon_rate[i],
                    simple_rate[i],
                    redemption[i],
                    accrued_interest[i],
                )
            })
            .collect(),
    )
}

#[extendr]
#[allow(clippy::too_many_arguments)]
fn risk_from_compound_yield(
    t: Vec<f64>,
    coupon_rate: Vec<f64>,
    compound_yield: Vec<f64>,
    redemption: Vec<f64>,
    accrued_interest: Vec<f64>,
    coupon_frequency: i32,
    compound_frequency: i32,
) -> List {
    risk_list(
        (0..t.len())
            .map(|i| {
                yen_bond::risk_from_compound_yield(
                    t[i],
                    coupon_rate[i],
                    compound_yield[i],
                    redemption[i],
                    frequencies(coupon_frequency, compound_frequency),
                    accrued_interest[i],
                )
            })
            .collect(),
    )
}

// ---- OIS curves ------------------------------------------------------------------------------

/// A tenor of `ON` is an overnight deposit, which has no settlement days, frequency or lag.
#[extendr]
#[allow(clippy::too_many_arguments)]
fn build_ois_curve(
    evaluation_date: i32,
    reference_date: i32,
    index: &str,
    curve_day_count: &str,
    tenor: Vec<String>,
    rate: Vec<f64>,
    settlement_days: Vec<i32>,
    payment_frequency: Vec<i32>,
    payment_lag: Vec<i32>,
    interpolation: &str,
    accuracy: f64,
) -> List {
    let build = || -> Core<OisCurve> {
        let instruments = (0..tenor.len())
            .map(|i| {
                Ok(if tenor[i] == "ON" {
                    OisCurveInstrument::OvernightDeposit { rate: rate[i] }
                } else {
                    OisCurveInstrument::Ois {
                        tenor: names::period(&tenor[i])?,
                        rate: rate[i],
                        settlement_days: settlement_days[i] as u32,
                        payment_frequency: payment_frequency[i] as u32,
                        payment_lag: payment_lag[i] as u32,
                    }
                })
            })
            .collect::<Core<Vec<_>>>()?;
        riemann::ois::build_ois_curve(&OisCurveSpec {
            evaluation_date: date(evaluation_date)?,
            reference_date: date(reference_date)?,
            index: names::overnight_index(index)?,
            curve_day_count: names::day_count(curve_day_count)?,
            instruments,
            interpolation: names::ois_interpolation(interpolation)?,
            accuracy,
        })
    };
    outcome(build().map(|curve| list!(value = ExternalPtr::new(curve))))
}

/// The same curve with extrapolation allowed or not; the original value does not change.
#[extendr]
fn with_extrapolation(curve: ExternalPtr<OisCurve>, extrapolate: bool) -> ExternalPtr<OisCurve> {
    let mut changed: OisCurve = (*curve).clone();
    changed.curve = changed.curve.with_extrapolation(extrapolate);
    ExternalPtr::new(changed)
}

#[extendr]
fn discount(curve: ExternalPtr<OisCurve>, date_serial: Vec<i32>) -> List {
    let rows = date_serial
        .iter()
        .map(|&d| curve.curve.discount(date(d)?))
        .collect();
    value_list(rows)
}

#[extendr]
fn zero_rate(
    curve: ExternalPtr<OisCurve>,
    date_serial: Vec<i32>,
    day_count: &str,
    compounding: &str,
    frequency: i32,
) -> List {
    let rows = || -> Core<List> {
        let dc = names::day_count(day_count)?;
        let comp = names::compounding(compounding)?;
        let rows = date_serial
            .iter()
            .map(|&d| {
                Ok(curve
                    .curve
                    .zero_rate(date(d)?, &dc, comp, frequency as u32)?
                    .rate)
            })
            .collect();
        Ok(value_list(rows))
    };
    outcome(rows())
}

#[extendr]
fn forward_rate(
    curve: ExternalPtr<OisCurve>,
    start_date: Vec<i32>,
    end_date: Vec<i32>,
    day_count: &str,
    compounding: &str,
    frequency: i32,
) -> List {
    let rows = || -> Core<List> {
        let dc = names::day_count(day_count)?;
        let comp = names::compounding(compounding)?;
        let rows = start_date
            .iter()
            .zip(&end_date)
            .map(|(&a, &b)| {
                Ok(curve
                    .curve
                    .forward_rate(date(a)?, date(b)?, &dc, comp, frequency as u32)?
                    .rate)
            })
            .collect();
        Ok(value_list(rows))
    };
    outcome(rows())
}

#[extendr]
fn quote_repricing(curve: ExternalPtr<OisCurve>) -> List {
    let q = &curve.quote_repricing;
    let serials = |f: fn(&riemann::ois::QuoteRepricing) -> Date| -> Vec<i32> {
        q.iter().map(|r| f(r).serial()).collect()
    };
    list!(
        rate = q.iter().map(|r| r.rate).collect::<Vec<f64>>(),
        start_date = serials(|r| r.start_date),
        maturity_date = serials(|r| r.maturity_date),
        pillar_date = serials(|r| r.pillar_date),
        latest_relevant_date = serials(|r| r.latest_relevant_date),
        used_in_bootstrap = q.iter().map(|r| r.used_in_bootstrap).collect::<Vec<bool>>(),
        implied_rate = q
            .iter()
            .map(|r| r.implied_rate.unwrap_or_else(na_real))
            .collect::<Vec<f64>>()
    )
}

#[extendr]
fn discount_nodes(curve: ExternalPtr<OisCurve>) -> List {
    let (dates, values) = curve
        .curve
        .discount_nodes()
        .expect("an OIS curve holds discount factors");
    list!(
        date = dates.iter().map(|d| d.serial()).collect::<Vec<i32>>(),
        discount = values.to_vec()
    )
}

// ---- sensitivities ---------------------------------------------------------------------------

#[extendr]
fn yield_sensitivity(
    bond: ExternalPtr<FixedRateBond>,
    rate: Vec<f64>,
    day_count: &str,
    compounding: &str,
    frequency: i32,
    settlement_date: Vec<i32>,
) -> List {
    let rows = || -> Core<List> {
        let convention = yield_convention(day_count, compounding, frequency)?;
        let n = rate.len();
        let (mut price, mut first, mut second) =
            (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
        let mut errors = Vec::with_capacity(n);
        for (&r, &s) in rate.iter().zip(&settlement_date) {
            let row = convention
                .rate(r)
                .and_then(|y| risk::yield_sensitivity(&bond, &y, date(s)?));
            match row {
                Ok(v) => {
                    price.push(v.dirty_price);
                    first.push(v.d_price_d_yield);
                    second.push(v.d2_price_d_yield2);
                    errors.push(String::new());
                }
                Err(e) => {
                    price.push(na_real());
                    first.push(na_real());
                    second.push(na_real());
                    errors.push(e.to_string());
                }
            }
        }
        Ok(list!(
            dirty_price = price,
            d_price_d_yield = first,
            d2_price_d_yield2 = second,
            error = errors
        ))
    };
    outcome(rows())
}

#[extendr]
fn curve_node_sensitivity(
    bond: ExternalPtr<FixedRateBond>,
    curve: ExternalPtr<OisCurve>,
    evaluation_date: i32,
) -> List {
    let rows = || -> Core<List> {
        let (value, rows) = risk::curve_sensitivity(&bond, &curve.curve, date(evaluation_date)?)?;
        let (mut dates, mut times) = (Vec::new(), Vec::new());
        let (mut d_npv, mut d_price) = (Vec::new(), Vec::new());
        for row in rows {
            if let CurveInput::LogDiscount { date, time } = row.input {
                dates.push(date.serial() as f64);
                times.push(time);
                d_npv.push(row.d_npv);
                d_price.push(row.d_dirty_price);
            }
        }
        Ok(list!(
            npv = value.npv,
            dirty_price = value.dirty_price,
            date = dates,
            time = times,
            d_npv = d_npv,
            d_dirty_price = d_price
        ))
    };
    outcome(rows())
}

#[extendr]
fn quote_sensitivity(
    bond: ExternalPtr<FixedRateBond>,
    curve: ExternalPtr<OisCurve>,
    evaluation_date: i32,
) -> List {
    let rows = || -> Core<List> {
        let (value, rows) = risk::quote_sensitivity(&bond, &curve, date(evaluation_date)?)?;
        Ok(list!(
            npv = value.npv,
            dirty_price = value.dirty_price,
            instrument = rows.iter().map(|r| r.instrument as i32 + 1).collect::<Vec<i32>>(),
            pillar_date = rows.iter().map(|r| r.pillar_date.serial() as f64).collect::<Vec<f64>>(),
            rate = rows.iter().map(|r| r.quote).collect::<Vec<f64>>(),
            d_npv = rows.iter().map(|r| r.d_npv).collect::<Vec<f64>>(),
            d_dirty_price = rows.iter().map(|r| r.d_dirty_price).collect::<Vec<f64>>()
        ))
    };
    outcome(rows())
}

// ---- asset swaps and futures basis ----------------------------------------------------------

#[extendr]
fn ois_asset_swap(
    bond: ExternalPtr<FixedRateBond>,
    curve: ExternalPtr<OisCurve>,
    clean_price: Vec<f64>,
    spread: Vec<f64>,
    payment_frequency: i32,
    payment_lag: i32,
    evaluation_date: i32,
) -> List {
    let (columns, error) = table(clean_price.len(), |i| {
        let v = asset_swap::ois_asset_swap(
            &bond,
            &curve.curve,
            &curve.spec.index,
            &OisAssetSwapTerms {
                clean_price: clean_price[i],
                payment_frequency: payment_frequency as u32,
                payment_lag: payment_lag.max(0) as u32,
                spread: spread[i],
            },
            date(evaluation_date)?,
        )?;
        Ok([
            v.accrued,
            v.dirty_price,
            v.fair_spread,
            v.spread_annuity,
            v.npv,
            v.bond_leg_pv,
            v.overnight_leg_pv,
            v.principal_pv,
            v.upfront_pv,
            v.fair_clean_price,
            v.effective_date.serial() as f64,
            v.maturity_date.serial() as f64,
        ])
    });
    list!(
        accrued = columns[0].clone(),
        dirty_price = columns[1].clone(),
        fair_spread = columns[2].clone(),
        spread_annuity = columns[3].clone(),
        npv = columns[4].clone(),
        bond_leg_pv = columns[5].clone(),
        overnight_leg_pv = columns[6].clone(),
        principal_pv = columns[7].clone(),
        upfront_pv = columns[8].clone(),
        fair_clean_price = columns[9].clone(),
        effective_date = columns[10].clone(),
        maturity_date = columns[11].clone(),
        error = error
    )
}

#[extendr]
fn asset_swap_quote_sensitivity(
    bond: ExternalPtr<FixedRateBond>,
    curve: ExternalPtr<OisCurve>,
    clean_price: f64,
    payment_frequency: i32,
    payment_lag: i32,
    evaluation_date: i32,
) -> List {
    let run = || -> Core<List> {
        let rows = asset_swap::asset_swap_quote_sensitivity(
            &bond,
            &curve,
            &OisAssetSwapTerms {
                clean_price,
                payment_frequency: payment_frequency as u32,
                payment_lag: payment_lag.max(0) as u32,
                spread: 0.0,
            },
            date(evaluation_date)?,
        )?;
        Ok(list!(
            instrument = rows.iter().map(|r| r.instrument as i32 + 1).collect::<Vec<i32>>(),
            pillar_date = rows.iter().map(|r| r.pillar_date.serial() as f64).collect::<Vec<f64>>(),
            rate = rows.iter().map(|r| r.quote).collect::<Vec<f64>>(),
            d_fair_spread = rows.iter().map(|r| r.d_fair_spread).collect::<Vec<f64>>()
        ))
    };
    outcome(run())
}

#[extendr]
#[allow(clippy::too_many_arguments)]
fn yen_asset_swap(
    bond: ExternalPtr<FixedRateBond>,
    foreign_curve: ExternalPtr<OisCurve>,
    yen_curve: ExternalPtr<OisCurve>,
    clean_price: Vec<f64>,
    fx_spot: Vec<f64>,
    basis: Vec<f64>,
    spread: Vec<f64>,
    foreign_payment_frequency: i32,
    yen_payment_frequency: i32,
    foreign_payment_lag: i32,
    yen_payment_lag: i32,
    evaluation_date: i32,
) -> List {
    let (columns, error) = table(clean_price.len(), |i| {
        let terms = |frequency: i32, lag: i32| OisAssetSwapTerms {
            clean_price: clean_price[i],
            payment_frequency: frequency as u32,
            payment_lag: lag.max(0) as u32,
            spread: 0.0,
        };
        let v = asset_swap::yen_asset_swap(
            &bond,
            &foreign_curve.curve,
            &foreign_curve.spec.index,
            &terms(foreign_payment_frequency, foreign_payment_lag),
            &yen_curve.curve,
            &yen_curve.spec.index,
            &terms(yen_payment_frequency, yen_payment_lag),
            &YenBasisTerms { fx_spot: fx_spot[i], basis: basis[i], spread: spread[i] },
            date(evaluation_date)?,
        )?;
        Ok([
            v.native_fair_spread,
            v.yen_fair_spread,
            v.simple_sum,
            v.npv,
            v.hedged_bond_pv,
            v.yen_base_leg_pv,
            v.upfront_pv,
            v.spread_annuity,
            v.calibrated_discount_spread,
        ])
    });
    list!(
        native_fair_spread = columns[0].clone(),
        yen_fair_spread = columns[1].clone(),
        simple_sum = columns[2].clone(),
        npv = columns[3].clone(),
        hedged_bond_pv = columns[4].clone(),
        yen_base_leg_pv = columns[5].clone(),
        upfront_pv = columns[6].clone(),
        spread_annuity = columns[7].clone(),
        calibrated_discount_spread = columns[8].clone(),
        error = error
    )
}

#[extendr]
fn future_basis(
    bond: ExternalPtr<FixedRateBond>,
    clean_price: f64,
    futures_price: f64,
    conversion_factor: f64,
    funding_rate: f64,
    valuation_date: i32,
    delivery_date: i32,
) -> List {
    let run = || -> Core<List> {
        let v = asset_swap::future_basis(
            &bond,
            &FutureBasisTerms {
                clean_price,
                futures_price,
                conversion_factor,
                funding_rate,
                valuation_date: date(valuation_date)?,
                delivery_date: date(delivery_date)?,
            },
        )?;
        Ok(list!(
            accrued_start = v.accrued_start,
            accrued_end = v.accrued_end,
            coupon_income = v.coupon_income,
            futures_invoice = v.futures_invoice,
            gross_basis = v.gross_basis,
            funding_cost = v.funding_cost,
            carry = v.carry,
            net_basis = v.net_basis,
            implied_repo = v.implied_repo
        ))
    };
    outcome(run())
}

// ---- Hull-White and SABR ---------------------------------------------------------------------

/// `spacing` above 0 selects the trapezoid rule; otherwise `nodes` of Gauss-Hermite.
fn grid_settings(points: i32, limit: f64, nodes: i32, spacing: f64) -> GridSettings {
    GridSettings {
        points: points.max(0) as usize,
        limit,
        quadrature: if spacing > 0.0 {
            Quadrature::Trapezoid(spacing)
        } else {
            Quadrature::GaussHermite(nodes.max(0) as usize)
        },
    }
}

#[extendr]
#[allow(clippy::too_many_arguments)]
fn hull_white_bond_option(
    option_type: Vec<String>,
    strike: Vec<f64>,
    maturity: Vec<f64>,
    bond_maturity: Vec<f64>,
    mean_reversion: f64,
    volatility: f64,
    curve: ExternalPtr<OisCurve>,
) -> List {
    let model = HullWhite { mean_reversion, volatility, curve: &curve.curve };
    let rows = (0..strike.len())
        .map(|i| {
            model.discount_bond_option(
                names::option_type(&option_type[i])?,
                strike[i],
                maturity[i],
                maturity[i],
                bond_maturity[i],
            )
        })
        .collect();
    value_list(rows)
}

#[extendr]
#[allow(clippy::too_many_arguments)]
fn hull_white_swaption(
    payer: bool,
    maturity: f64,
    payment_times: Vec<f64>,
    coupons: Vec<f64>,
    nominal: f64,
    mean_reversion: f64,
    volatility: f64,
    curve: ExternalPtr<OisCurve>,
) -> List {
    let flows = SwaptionFlows { payer, maturity, value_time: maturity, payment_times, coupons, nominal };
    outcome(
        hull_white::jamshidian_swaption_sensitivity(mean_reversion, volatility, &curve.curve, &flows).map(|s| {
            list!(value = s.value, d_mean_reversion = s.d_mean_reversion, d_volatility = s.d_volatility)
        }),
    )
}

#[extendr]
#[allow(clippy::too_many_arguments)]
fn hull_white_bermudan_swaption(
    payer: bool,
    maturity: f64,
    payment_times: Vec<f64>,
    coupons: Vec<f64>,
    nominal: f64,
    exercise_times: Vec<f64>,
    mean_reversion: f64,
    volatility: f64,
    curve: ExternalPtr<OisCurve>,
    points: i32,
    limit: f64,
    nodes: i32,
    spacing: f64,
) -> List {
    let flows = SwaptionFlows { payer, maturity, value_time: maturity, payment_times, coupons, nominal };
    let settings = grid_settings(points, limit, nodes, spacing);
    outcome(
        hull_white::bermudan_swaption(&flows, &exercise_times, mean_reversion, volatility, &curve.curve, &settings)
            .map(|value| list!(value = value)),
    )
}

#[extendr]
#[allow(clippy::too_many_arguments)]
fn hull_white_callable_bond(
    time: Vec<f64>,
    coupon: Vec<f64>,
    principal: Vec<f64>,
    call_price: Vec<f64>,
    mean_reversion: f64,
    volatility: f64,
    curve: ExternalPtr<OisCurve>,
    points: i32,
    limit: f64,
    nodes: i32,
    spacing: f64,
) -> List {
    let flows: Vec<CallableFlow> = (0..time.len())
        .map(|i| CallableFlow {
            time: time[i],
            coupon: coupon[i],
            principal: principal[i],
            // R has turned NA into a value of NaN here, which means no call on this date
            call_price: if call_price[i].is_nan() { None } else { Some(call_price[i]) },
        })
        .collect();
    let settings = grid_settings(points, limit, nodes, spacing);
    outcome(
        hull_white::callable_bond(&flows, mean_reversion, volatility, &curve.curve, &settings).map(|v| {
            list!(bullet = v.bullet, callable = v.callable, issuer_call_value = v.issuer_call_value)
        }),
    )
}

#[extendr]
#[allow(clippy::too_many_arguments)]
fn sabr_volatility(
    kind: &str,
    strike: Vec<f64>,
    forward: Vec<f64>,
    expiry: Vec<f64>,
    alpha: Vec<f64>,
    beta: Vec<f64>,
    nu: Vec<f64>,
    rho: Vec<f64>,
) -> List {
    let rows = || -> Core<List> {
        let kind = names::sabr_volatility_type(kind)?;
        let rows = (0..strike.len())
            .map(|i| {
                sabr::sabr_volatility(
                    kind,
                    strike[i],
                    forward[i],
                    expiry[i],
                    &SabrParameters { alpha: alpha[i], beta: beta[i], nu: nu[i], rho: rho[i] },
                )
            })
            .collect();
        Ok(value_list(rows))
    };
    outcome(rows())
}

#[extendr]
fn calibrate_sabr(kind: &str, strike: Vec<f64>, volatility: Vec<f64>, forward: f64, expiry: f64, beta: f64) -> List {
    let run = || -> Core<List> {
        let c = sabr::calibrate_sabr(names::sabr_volatility_type(kind)?, &strike, &volatility, forward, expiry, beta)?;
        Ok(list!(
            alpha = c.parameters.alpha,
            beta = c.parameters.beta,
            nu = c.parameters.nu,
            rho = c.parameters.rho,
            root_mean_square_error = c.root_mean_square_error,
            iterations = c.iterations as i32
        ))
    };
    outcome(run())
}

// ---- credit default swaps ----------------------------------------------------------------------

fn cds_value_list(v: CdsValue) -> List {
    list!(
        npv = v.npv,
        coupon_leg_npv = v.coupon_leg_npv,
        default_leg_npv = v.default_leg_npv,
        upfront_npv = v.upfront_npv,
        accrual_rebate_npv = v.accrual_rebate_npv,
        fair_spread = v.fair_spread,
        fair_upfront = v.fair_upfront.unwrap_or_else(na_real)
    )
}

#[extendr]
#[allow(clippy::too_many_arguments)]
fn credit_default_swap(
    side: &str,
    notional: f64,
    running_spread: f64,
    effective_date: i32,
    termination_date: i32,
    tenor: &str,
    calendar: &str,
    convention: &str,
    termination_convention: &str,
    rule: &str,
    payment_convention: &str,
    day_count: &str,
    upfront: f64,
    rebates_accrual: bool,
) -> List {
    let build = || -> Core<Cds> {
        Cds::new(CdsSpec {
            side: names::protection_side(side)?,
            notional,
            running_spread,
            schedule: ScheduleSpec {
                effective_date: date(effective_date)?,
                termination_date: date(termination_date)?,
                tenor: names::period(tenor)?,
                calendar: HolidayCalendar::from(names::calendar(calendar)?),
                convention: names::business_day_convention(convention)?,
                termination_convention: names::business_day_convention(termination_convention)?,
                rule: names::date_generation_rule(rule)?,
                end_of_month: false,
                first_date: None,
                next_to_last_date: None,
            },
            payment_convention: names::business_day_convention(payment_convention)?,
            day_count: names::day_count(day_count)?,
            last_period_day_count: None,
            upfront: Some(upfront),
            protection_start: None,
            trade_date: None,
            cash_settlement_days: 3,
            rebates_accrual,
        })
    };
    outcome(build().map(|c| list!(value = ExternalPtr::new(c))))
}

#[extendr]
fn hazard_curve(reference_date: i32, day_count: &str, dates: Vec<i32>, hazards: Vec<f64>) -> List {
    let build = || -> Core<HazardCurve> {
        let reference = date(reference_date)?;
        let dates = dates.iter().map(|&d| date(d)).collect::<Core<Vec<Date>>>()?;
        HazardCurve::new(reference, names::day_count(day_count)?, dates, hazards.clone())
    };
    outcome(build().map(|c| list!(value = ExternalPtr::new(c))))
}

#[extendr]
fn survival_probability(curve: ExternalPtr<HazardCurve>, date_serial: Vec<i32>) -> List {
    let rows = date_serial.iter().map(|&d| curve.survival(date(d)?)).collect();
    value_list(rows)
}

#[extendr]
fn cds_value(
    cds: ExternalPtr<Cds>,
    hazard: ExternalPtr<HazardCurve>,
    recovery: f64,
    curve: ExternalPtr<OisCurve>,
    evaluation_date: i32,
    engine: &str,
) -> List {
    let run = || -> Core<List> {
        let v = cds::cds_value(names::cds_engine(engine)?, &cds, &hazard, recovery, &curve.curve, date(evaluation_date)?)?;
        Ok(cds_value_list(v))
    };
    outcome(run())
}

#[extendr]
#[allow(clippy::too_many_arguments)]
fn bootstrap_hazard_curve(
    evaluation_date: i32,
    maturity: Vec<i32>,
    spread: Vec<f64>,
    recovery: f64,
    curve: ExternalPtr<OisCurve>,
    engine: &str,
    settlement_days: i32,
    calendar: &str,
    tenor: &str,
    payment_convention: &str,
    rule: &str,
    day_count: &str,
    curve_day_count: &str,
    accuracy: f64,
) -> List {
    let build = || -> Core<CdsCurve> {
        let spec = CdsCurveSpec {
            evaluation_date: date(evaluation_date)?,
            settlement_days: settlement_days.max(0) as u32,
            calendar: HolidayCalendar::from(names::calendar(calendar)?),
            tenor: names::period(tenor)?,
            payment_convention: names::business_day_convention(payment_convention)?,
            rule: names::date_generation_rule(rule)?,
            day_count: names::day_count(day_count)?,
            last_period_day_count: None,
            rebates_accrual: true,
            recovery,
            engine: names::cds_engine(engine)?,
            curve_day_count: names::day_count(curve_day_count)?,
            accuracy,
            quotes: maturity
                .iter()
                .zip(&spread)
                .map(|(&m, &s)| Ok(SpreadQuote { maturity: date(m)?, spread: s }))
                .collect::<Core<Vec<_>>>()?,
        };
        cds::bootstrap_hazard(&spec, &curve.curve)
    };
    outcome(build().map(|c| {
        list!(
            node_date = c.hazard.dates().iter().map(|d| d.serial() as f64).collect::<Vec<f64>>(),
            hazard = c.hazard.hazards().to_vec(),
            hazard_curve = ExternalPtr::new(c.hazard.clone()),
            value = ExternalPtr::new(c)
        )
    }))
}

#[extendr]
fn cds_quote_sensitivity(
    cds: ExternalPtr<Cds>,
    credit: ExternalPtr<CdsCurve>,
    curve: ExternalPtr<OisCurve>,
    evaluation_date: i32,
    engine: &str,
) -> List {
    let run = || -> Core<List> {
        let s = cds::cds_sensitivity(names::cds_engine(engine)?, &cds, &credit, &curve.curve, date(evaluation_date)?)?;
        Ok(list!(
            npv = s.value.npv,
            d_npv_d_recovery = s.d_npv_d_recovery,
            quote = s.quotes.iter().map(|q| q.quote as i32 + 1).collect::<Vec<i32>>(),
            maturity = s.quotes.iter().map(|q| q.maturity.serial() as f64).collect::<Vec<f64>>(),
            spread = s.quotes.iter().map(|q| q.spread).collect::<Vec<f64>>(),
            d_npv = s.quotes.iter().map(|q| q.d_npv).collect::<Vec<f64>>()
        ))
    };
    outcome(run())
}

#[extendr]
fn cds_maturity(trade_date: i32, tenor: &str, rule: &str) -> List {
    let run = || -> Core<f64> {
        let d = riemann::time::cds_maturity(date(trade_date)?, names::period(tenor)?, names::date_generation_rule(rule)?)?;
        Ok(d.serial() as f64)
    };
    value_list(vec![run()])
}

// ---- options --------------------------------------------------------------------------------

fn option_inputs(values: [f64; 6]) -> OptionInputs {
    let [spot, strike, rate, dividend_yield, volatility, time] = values;
    OptionInputs { spot, strike, rate, dividend_yield, volatility, time }
}

/// Columns of `f(row)`, with NA and a message where a row fails.
fn table<const N: usize>(n: usize, f: impl Fn(usize) -> Core<[f64; N]>) -> (Vec<Vec<f64>>, Vec<String>) {
    let mut columns = vec![Vec::with_capacity(n); N];
    let mut errors = Vec::with_capacity(n);
    for i in 0..n {
        match f(i) {
            Ok(row) => {
                columns.iter_mut().zip(row).for_each(|(c, v)| c.push(v));
                errors.push(String::new());
            }
            Err(e) => {
                columns.iter_mut().for_each(|c| c.push(na_real()));
                errors.push(e.to_string());
            }
        }
    }
    (columns, errors)
}

fn greeks_row(g: Greeks) -> [f64; 8] {
    [g.value, g.delta, g.gamma, g.vega, g.rho, g.dividend_rho, g.theta, g.strike_sensitivity]
}

fn greeks_list(n: usize, f: impl Fn(usize) -> Core<Greeks>) -> List {
    let (columns, error) = table(n, |i| f(i).map(greeks_row));
    list!(
        value = columns[0].clone(),
        delta = columns[1].clone(),
        gamma = columns[2].clone(),
        vega = columns[3].clone(),
        rho = columns[4].clone(),
        dividend_rho = columns[5].clone(),
        theta = columns[6].clone(),
        strike_sensitivity = columns[7].clone(),
        error = error
    )
}

#[extendr]
#[allow(clippy::too_many_arguments)]
fn european_option(
    option_type: Vec<String>,
    spot: Vec<f64>,
    strike: Vec<f64>,
    rate: Vec<f64>,
    dividend_yield: Vec<f64>,
    volatility: Vec<f64>,
    time: Vec<f64>,
) -> List {
    greeks_list(spot.len(), |i| {
        options::european_greeks(
            names::option_type(&option_type[i])?,
            &option_inputs([spot[i], strike[i], rate[i], dividend_yield[i], volatility[i], time[i]]),
        )
    })
}

#[extendr]
#[allow(clippy::too_many_arguments)]
fn barrier_option(
    option_type: Vec<String>,
    barrier_type: Vec<String>,
    spot: Vec<f64>,
    strike: Vec<f64>,
    barrier: Vec<f64>,
    rebate: Vec<f64>,
    rate: Vec<f64>,
    dividend_yield: Vec<f64>,
    volatility: Vec<f64>,
    time: Vec<f64>,
) -> List {
    greeks_list(spot.len(), |i| {
        options::barrier_greeks(
            names::option_type(&option_type[i])?,
            names::barrier_type(&barrier_type[i])?,
            barrier[i],
            rebate[i],
            &option_inputs([spot[i], strike[i], rate[i], dividend_yield[i], volatility[i], time[i]]),
        )
    })
}

#[extendr]
#[allow(clippy::too_many_arguments)]
fn binomial_option(
    option_type: Vec<String>,
    exercise: Vec<String>,
    spot: Vec<f64>,
    strike: Vec<f64>,
    rate: Vec<f64>,
    dividend_yield: Vec<f64>,
    volatility: Vec<f64>,
    time: Vec<f64>,
    steps: Vec<i32>,
) -> List {
    greeks_list(spot.len(), |i| {
        options::binomial_greeks(
            names::option_type(&option_type[i])?,
            names::exercise(&exercise[i])?,
            &option_inputs([spot[i], strike[i], rate[i], dividend_yield[i], volatility[i], time[i]]),
            steps[i].max(0) as usize,
        )
    })
}

#[extendr]
fn forward_option(
    model: Vec<String>,
    option_type: Vec<String>,
    forward: Vec<f64>,
    strike: Vec<f64>,
    volatility: Vec<f64>,
    time: Vec<f64>,
    discount: Vec<f64>,
) -> List {
    let (columns, error) = table(forward.len(), |i| {
        let g = options::forward_greeks(
            names::forward_model(&model[i])?,
            names::option_type(&option_type[i])?,
            &options::ForwardInputs {
                forward: forward[i],
                strike: strike[i],
                volatility: volatility[i],
                time: time[i],
                discount: discount[i],
            },
        )?;
        Ok([g.value, g.delta_forward, g.gamma_forward, g.vega, g.d_time, g.d_discount, g.strike_sensitivity])
    });
    list!(
        value = columns[0].clone(),
        delta_forward = columns[1].clone(),
        gamma_forward = columns[2].clone(),
        vega = columns[3].clone(),
        d_time = columns[4].clone(),
        d_discount = columns[5].clone(),
        strike_sensitivity = columns[6].clone(),
        error = error
    )
}

#[extendr]
#[allow(clippy::too_many_arguments)]
fn implied_volatility(
    option_type: Vec<String>,
    price: Vec<f64>,
    spot: Vec<f64>,
    strike: Vec<f64>,
    rate: Vec<f64>,
    dividend_yield: Vec<f64>,
    time: Vec<f64>,
    accuracy: f64,
    max_evaluations: i32,
) -> List {
    let rows = (0..spot.len())
        .map(|i| {
            options::implied_volatility(
                names::option_type(&option_type[i])?,
                price[i],
                &option_inputs([spot[i], strike[i], rate[i], dividend_yield[i], 0.2, time[i]]),
                accuracy,
                max_evaluations.max(0) as usize,
            )
        })
        .collect();
    value_list(rows)
}

#[extendr]
#[allow(clippy::too_many_arguments)]
fn monte_carlo_european(
    option_type: &str,
    spot: f64,
    strike: f64,
    rate: f64,
    dividend_yield: f64,
    volatility: f64,
    time: f64,
    paths: f64,
    seed: f64,
    antithetic: bool,
    smoothing: f64,
    threads: i32,
) -> List {
    let run = || -> Core<List> {
        let settings = MonteCarloSettings {
            paths: paths as u64,
            seed: seed as u64,
            antithetic,
            smoothing: if smoothing > 0.0 { Some(smoothing) } else { None },
            threads: threads.max(0) as usize,
        };
        let r = options::monte_carlo_european(
            names::option_type(option_type)?,
            &option_inputs([spot, strike, rate, dividend_yield, volatility, time]),
            &settings,
        )?;
        Ok(list!(
            value = r.value,
            standard_error = r.standard_error,
            delta = r.delta,
            gamma = r.gamma.unwrap_or_else(na_real),
            vega = r.vega,
            rho = r.rho,
            dividend_rho = r.dividend_rho,
            theta = r.theta
        ))
    };
    outcome(run())
}

// ---- calendars -------------------------------------------------------------------------------

/// 1 for a business day, 0 for a holiday, NA where the calendar fails.
#[extendr]
fn is_business_day(calendar: &str, date_serial: Vec<i32>) -> List {
    outcome(names::calendar(calendar).map(|cal| {
        let rows = date_serial
            .iter()
            .map(|&d| Ok(f64::from(u8::from(cal.is_business_day(date(d)?)?))))
            .collect();
        value_list(rows)
    }))
}

#[extendr]
fn holidays(calendar: &str, from: i32, to: i32, include_weekends: bool) -> List {
    let rows = || -> Core<List> {
        let cal = HolidayCalendar::from(names::calendar(calendar)?);
        let list = cal.holidays(date(from)?, date(to)?, include_weekends)?;
        Ok(list!(
            value = list.iter().map(|d| d.serial()).collect::<Vec<i32>>()
        ))
    };
    outcome(rows())
}

// ---- parallel batches --------------------------------------------------------------------
//
// The inputs are converted here on R's main thread, and the results are converted back here after
// the threads have finished. `riemann::batch` receives plain Rust values only; the core crate does
// not depend on extendr, so the threads cannot call the R API.

#[extendr]
#[allow(clippy::too_many_arguments)]
fn bond_values_batch(
    settlement_days: Vec<i32>,
    face_amount: Vec<f64>,
    effective_date: Vec<i32>,
    termination_date: Vec<i32>,
    tenor: Vec<String>,
    calendar: Vec<String>,
    convention: Vec<String>,
    termination_convention: Vec<String>,
    rule: Vec<String>,
    end_of_month: Vec<i32>,
    coupon_rate: Vec<f64>,
    day_count: Vec<String>,
    payment_convention: Vec<String>,
    redemption: Vec<f64>,
    issue_date: Vec<i32>,
    rate: Vec<f64>,
    yield_day_count: Vec<String>,
    compounding: Vec<String>,
    frequency: Vec<i32>,
    evaluation_date: Vec<i32>,
    threads: i32,
) -> List {
    let run = || -> Core<List> {
        let units = (0..rate.len())
            .map(|i| {
                Ok(BondEvaluation {
                    spec: FixedRateBondSpec {
                        settlement_days: settlement_days[i] as u32,
                        face_amount: face_amount[i],
                        schedule: ScheduleSpec {
                            effective_date: date(effective_date[i])?,
                            termination_date: date(termination_date[i])?,
                            tenor: names::period(&tenor[i])?,
                            calendar: HolidayCalendar::from(names::calendar(&calendar[i])?),
                            convention: names::business_day_convention(&convention[i])?,
                            termination_convention: names::business_day_convention(
                                &termination_convention[i],
                            )?,
                            rule: names::date_generation_rule(&rule[i])?,
                            end_of_month: flag(end_of_month[i])?,
                            first_date: None,
                            next_to_last_date: None,
                        },
                        coupon_rate: coupon_rate[i],
                        day_count: names::day_count(&day_count[i])?,
                        payment_convention: names::business_day_convention(&payment_convention[i])?,
                        redemption: redemption[i],
                        issue_date: Some(date(issue_date[i])?),
                        payment_calendar: None,
                    },
                    rate: rate[i],
                    convention: yield_convention(
                        &yield_day_count[i],
                        &compounding[i],
                        frequency[i],
                    )?,
                    evaluation_date: date(evaluation_date[i])?,
                })
            })
            .collect::<Core<Vec<_>>>()?;
        let rows = batch::bond_values_batch(&units, threads as usize)?;
        let pick = |f: fn(&batch::BondValues) -> f64| -> Vec<f64> {
            rows.iter()
                .map(|r| r.as_ref().map_or(na_real(), f))
                .collect()
        };
        let errors: Vec<String> = rows
            .iter()
            .map(|r| r.as_ref().err().map_or(String::new(), |e| e.to_string()))
            .collect();
        Ok(list!(
            settlement_date = pick(|r| r.settlement_date.serial() as f64),
            clean_price = pick(|r| r.clean_price),
            dirty_price = pick(|r| r.dirty_price),
            accrued_amount = pick(|r| r.accrued_amount),
            duration_modified = pick(|r| r.duration_modified),
            convexity = pick(|r| r.convexity),
            basis_point_value = pick(|r| r.basis_point_value),
            yield_value_basis_point = pick(|r| r.yield_value_basis_point),
            error = errors
        ))
    };
    outcome(run())
}

fn first_error(values: [&Core<f64>; 3]) -> String {
    values
        .iter()
        .find_map(|v| v.as_ref().err().map(|e| e.to_string()))
        .unwrap_or_default()
}

/// Instruments and dates are columns with the 1-based number of their curve.
#[extendr]
#[allow(clippy::too_many_arguments)]
fn curve_values_batch(
    evaluation_date: Vec<i32>,
    reference_date: Vec<i32>,
    index: Vec<String>,
    curve_day_count: Vec<String>,
    interpolation: Vec<String>,
    accuracy: Vec<f64>,
    instrument_curve: Vec<i32>,
    tenor: Vec<String>,
    rate: Vec<f64>,
    settlement_days: Vec<i32>,
    payment_frequency: Vec<i32>,
    payment_lag: Vec<i32>,
    date_curve: Vec<i32>,
    date_serial: Vec<i32>,
    threads: i32,
) -> List {
    let run = || -> Core<List> {
        let mut units = Vec::with_capacity(evaluation_date.len());
        for c in 0..evaluation_date.len() {
            let number = c as i32 + 1;
            let instruments = (0..tenor.len())
                .filter(|&i| instrument_curve[i] == number)
                .map(|i| {
                    Ok(if tenor[i] == "ON" {
                        OisCurveInstrument::OvernightDeposit { rate: rate[i] }
                    } else {
                        OisCurveInstrument::Ois {
                            tenor: names::period(&tenor[i])?,
                            rate: rate[i],
                            settlement_days: settlement_days[i] as u32,
                            payment_frequency: payment_frequency[i] as u32,
                            payment_lag: payment_lag[i] as u32,
                        }
                    })
                })
                .collect::<Core<Vec<_>>>()?;
            let dates = (0..date_serial.len())
                .filter(|&i| date_curve[i] == number)
                .map(|i| date(date_serial[i]))
                .collect::<Core<Vec<_>>>()?;
            units.push(CurveEvaluation {
                spec: OisCurveSpec {
                    evaluation_date: date(evaluation_date[c])?,
                    reference_date: date(reference_date[c])?,
                    index: names::overnight_index(&index[c])?,
                    curve_day_count: names::day_count(&curve_day_count[c])?,
                    instruments,
                    interpolation: names::ois_interpolation(&interpolation[c])?,
                    accuracy: accuracy[c],
                },
                dates,
                day_count: names::day_count(&curve_day_count[c])?,
            });
        }
        let results = batch::curve_values_batch(&units, threads as usize)?;

        let (mut curve, mut serial, mut errors) = (Vec::new(), Vec::new(), Vec::new());
        let (mut discount, mut zero, mut forward, mut worst) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for (c, result) in results.iter().enumerate() {
            match result {
                Ok(values) => {
                    let max_error = values
                        .quote_repricing
                        .iter()
                        .filter_map(|q| q.implied_rate.map(|v| (v - q.rate).abs()))
                        .fold(0.0_f64, f64::max);
                    for p in &values.points {
                        curve.push(c as i32 + 1);
                        serial.push(p.date.serial());
                        discount.push(p.discount.as_ref().map_or(na_real(), |v| *v));
                        zero.push(p.zero_continuous.as_ref().map_or(na_real(), |v| *v));
                        forward.push(p.forward_1d_simple.as_ref().map_or(na_real(), |v| *v));
                        worst.push(max_error);
                        errors.push(first_error([
                            &p.discount,
                            &p.zero_continuous,
                            &p.forward_1d_simple,
                        ]));
                    }
                }
                Err(e) => {
                    curve.push(c as i32 + 1);
                    serial.push(0);
                    discount.push(na_real());
                    zero.push(na_real());
                    forward.push(na_real());
                    worst.push(na_real());
                    errors.push(e.to_string());
                }
            }
        }
        Ok(list!(
            curve = curve,
            date = serial,
            discount = discount,
            zero_continuous = zero,
            forward_1d_simple = forward,
            max_reproduction_error = worst,
            error = errors
        ))
    };
    outcome(run())
}

/// A deliberate panic, for the test that a defect stays visible and does not end the R session.
#[extendr]
fn panic_for_test() {
    panic!("riemann deliberate panic for the test");
}

// The module has the name of the R package, because R looks for its routines by that name.
extendr_module! {
    mod RiemannFinMath;
    fn fixed_rate_bond;
    fn settlement_date;
    fn bond_values;
    fn duration;
    fn yield_from_price;
    fn value_on_curve;
    fn yield_sensitivity;
    fn curve_node_sensitivity;
    fn quote_sensitivity;
    fn ois_asset_swap;
    fn asset_swap_quote_sensitivity;
    fn yen_asset_swap;
    fn future_basis;
    fn hull_white_bond_option;
    fn hull_white_swaption;
    fn hull_white_bermudan_swaption;
    fn hull_white_callable_bond;
    fn sabr_volatility;
    fn calibrate_sabr;
    fn credit_default_swap;
    fn hazard_curve;
    fn survival_probability;
    fn cds_value;
    fn bootstrap_hazard_curve;
    fn cds_quote_sensitivity;
    fn cds_maturity;
    fn european_option;
    fn barrier_option;
    fn binomial_option;
    fn forward_option;
    fn implied_volatility;
    fn monte_carlo_european;
    fn remaining_years;
    fn price_from_simple_rate;
    fn simple_rate_from_price;
    fn price_from_compound_yield;
    fn compound_yield_from_price;
    fn risk_from_simple_rate;
    fn risk_from_compound_yield;
    fn build_ois_curve;
    fn with_extrapolation;
    fn discount;
    fn zero_rate;
    fn forward_rate;
    fn quote_repricing;
    fn discount_nodes;
    fn is_business_day;
    fn holidays;
    fn bond_values_batch;
    fn curve_values_batch;
    fn panic_for_test;
}
