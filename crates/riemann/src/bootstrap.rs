//! One pass of QuantLib's iterative bootstrap for curves with a local interpolation: linear zero
//! rates or log-linear discount factors, one Brent search per node from the shortest pillar.
//!
//! Reproduces QuantLib v1.42 (modified BSD license; see THIRD_PARTY_NOTICES):
//! `ql/termstructures/yield/bootstraptraits.hpp` (Copyright (C) 2005, 2007 StatPro Italia srl;
//! (C) 2011 Ferdinando Ametrano; (C) 2007 Chris Kenyon; (C) 2019 SoftSolutions! S.r.l.), the
//! `ZeroYield` and `Discount` traits; and `ql/termstructures/iterativebootstrap.hpp` (Copyright
//! (C) 2008, 2011, 2015 Ferdinando Ametrano; (C) 2007 Chris Kenyon; (C) 2007 StatPro Italia srl;
//! (C) 2015 Paolo Mazzocchi). A local interpolation needs only one pass (`loopRequired_` is false).

use crate::curves::{LogDiscountInterpolation, YieldCurve};
use crate::interest_rate::Compounding;
use crate::math::solver1d::solve_brent;
use crate::time::{Date, DayCount};
use crate::Error;
use std::cell::Cell;

const AVG_RATE: f64 = 0.05;
const MAX_RATE: f64 = 1.0;
const MAX_FUNCTION_EVALUATIONS: usize = 100;

/// What a bootstrapped curve interpolates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LocalInterpolation {
    /// Continuous zero rates, linear in time (QuantLib `PiecewiseLinearZero`).
    LinearZero,
    /// Discount factors, log-linear in time (QuantLib `PiecewiseLogLinearDiscount`).
    LogLinearDiscount,
}

fn build_curve(interpolation: LocalInterpolation, dates: &[Date], data: &[f64], day_count: &DayCount) -> Result<YieldCurve, Error> {
    match interpolation {
        LocalInterpolation::LinearZero => {
            YieldCurve::zero(dates.to_vec(), data.to_vec(), day_count.clone(), Compounding::Continuous, 0)
        }
        LocalInterpolation::LogLinearDiscount => YieldCurve::interpolated_discounts(
            dates.to_vec(),
            data.to_vec(),
            day_count.clone(),
            LogDiscountInterpolation::LogLinear,
        ),
    }
}

/// Bootstraps a curve from `reference` with a node at each pillar, increasing and after the
/// reference date. `error(k, curve)` is the quote of the instrument of pillar `k` minus its value on
/// a curve whose last node is that pillar. The first node of a zero curve takes the value of the
/// second (QuantLib `ZeroYield::updateGuess`). As in QuantLib, a node keeps the value last passed to
/// the objective.
pub fn bootstrap_local(
    reference: Date,
    day_count: &DayCount,
    interpolation: LocalInterpolation,
    pillars: &[Date],
    accuracy: f64,
    error: impl Fn(usize, &YieldCurve) -> Result<f64, Error>,
) -> Result<YieldCurve, Error> {
    if pillars.is_empty() {
        return Err(Error::InvalidInput { reason: "no instruments given".into() });
    }
    let mut dates = vec![reference];
    dates.extend_from_slice(pillars);
    for w in dates.windows(2) {
        if w[1] <= w[0] {
            return Err(Error::InvalidInput {
                reason: format!("pillars must follow the reference date and increase: {} after {}", w[1], w[0]),
            });
        }
    }
    let times = dates
        .iter()
        .map(|&d| day_count.year_fraction(reference, d, None, None))
        .collect::<Result<Vec<_>, Error>>()?;
    let zero = interpolation == LocalInterpolation::LinearZero;
    let mut data = vec![if zero { AVG_RATE } else { 1.0 }; dates.len()];
    for node in 1..dates.len() {
        let (mut guess, min, max) = if zero {
            let guess = if node == 1 {
                AVG_RATE
            } else if node == 2 {
                data[1]
            } else {
                // the zero rate at the new node on the line through the last two nodes
                let slope = (data[node - 1] - data[node - 2]) / (times[node - 1] - times[node - 2]);
                data[node - 1] + slope * (times[node] - times[node - 1])
            };
            (guess, -MAX_RATE, MAX_RATE)
        } else {
            let dt = times[node] - times[node - 1];
            let guess = if node == 1 {
                1.0 / (1.0 + AVG_RATE * times[1])
            } else {
                let r = -data[node - 1].ln() / times[node - 1];
                (-r * times[node]).exp()
            };
            (guess, data[node - 1] * (-MAX_RATE * dt).exp(), data[node - 1] * (MAX_RATE * dt).exp())
        };
        if guess >= max {
            guess = max - (max - min) / 5.0;
        } else if guess <= min {
            guess = min + (max - min) / 5.0;
        }
        let last_tried = Cell::new(guess);
        let objective = |x: f64| {
            last_tried.set(x);
            let mut y = data[..=node].to_vec();
            y[node] = x;
            if zero && node == 1 {
                y[0] = x;
            }
            let curve = build_curve(interpolation, &dates[..=node], &y, day_count)?;
            error(node - 1, &curve)
        };
        if let Err(e) = solve_brent(objective, accuracy, guess, min, max, MAX_FUNCTION_EVALUATIONS) {
            return Err(Error::NoConvergence { reason: format!("failed at instrument {node}, pillar {}: {e}", dates[node]) });
        }
        data[node] = last_tried.get();
        if zero && node == 1 {
            data[0] = data[1];
        }
    }
    build_curve(interpolation, &dates, &data, day_count)
}
