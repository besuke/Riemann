//! Interest rate risk in the banking book (IRRBB): the standardised interest rate shock scenarios of the
//! Basel Framework, SRP31.90–31.93 (version in force from 1 January 2026, published 16 July 2024).
//!
//! Written for rmql; no source was taken from QuantLib.
//!
//! - **Shock sizes** (SRP31.90, Table 2): a parallel, a short and a long shock per currency, in basis
//!   points. [`get_irrbb_shock_sizes`] returns the table values for the currencies listed there.
//! - **Shapes** (SRP31.91), for a tenor `t` in years and `x = 4` (the decay of the short shock):
//!   parallel `±P`; short `±S·e^(−t/x)`; long `±L·(1 − e^(−t/x))`; steepener
//!   `−0.65·|short(t)| + 0.9·|long(t)|`; flattener `+0.8·|short(t)| − 0.6·|long(t)|`. The worked example
//!   of SRP31.92 for the yen (`S = L = 100` bp, `t = 3.5`) gives +25.4 bp for the steepener and −1.6 bp
//!   for the flattener.
//! - **Floor** (SRP31.93): national supervisors may set floors on the post-shock rates, not greater than
//!   zero. The floor is an argument. A rate already below the floor is not raised by it: the post-shock
//!   rate is `max(base + shock, min(base, floor))`.
//! - **Shocked curve**: the base curve's zero rates (Actual/365 Fixed, continuously compounded) at the
//!   given dates, plus the shock at each date's tenor, floored, as a zero curve (linear in the zero
//!   rates). With no scenario the same construction gives the base curve on the same dates, so that a
//!   difference in value comes from the shock alone.

use crate::curves::YieldCurve;
use crate::interest_rate::Compounding;
use crate::time::{Actual365Kind, Date, DayCount};
use crate::Error;

fn invalid<T>(reason: String) -> Result<T, Error> {
    Err(Error::InvalidInput { reason })
}

/// The six scenarios of SRP31.90.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrrbbScenario {
    ParallelUp,
    ParallelDown,
    Steepener,
    Flattener,
    ShortUp,
    ShortDown,
}

impl IrrbbScenario {
    pub fn parse(name: &str) -> Result<IrrbbScenario, Error> {
        Ok(match name {
            "parallel_up" => IrrbbScenario::ParallelUp,
            "parallel_down" => IrrbbScenario::ParallelDown,
            "steepener" => IrrbbScenario::Steepener,
            "flattener" => IrrbbScenario::Flattener,
            "short_up" => IrrbbScenario::ShortUp,
            "short_down" => IrrbbScenario::ShortDown,
            other => return invalid(format!("unknown IRRBB scenario: {other}")),
        })
    }
}

/// Shock sizes of one currency, as decimals (0.01 = 100 bp).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IrrbbShockSizes {
    pub parallel: f64,
    pub short: f64,
    pub long: f64,
}

/// SRP31.90 Table 2 (in force from 1 January 2026), in basis points: parallel, short, long. Only the
/// currencies whose values were read from the published table are listed.
const SHOCK_TABLE_BP: [(&str, [f64; 3]); 8] = [
    ("JPY", [100.0, 100.0, 100.0]),
    ("USD", [200.0, 300.0, 225.0]),
    ("EUR", [225.0, 350.0, 200.0]),
    ("GBP", [275.0, 425.0, 250.0]),
    ("CHF", [175.0, 250.0, 200.0]),
    ("CAD", [200.0, 275.0, 175.0]),
    ("CNY", [225.0, 300.0, 150.0]),
    ("ZAR", [325.0, 500.0, 300.0]),
];

pub fn get_irrbb_shock_sizes(currency: &str) -> Result<IrrbbShockSizes, Error> {
    match SHOCK_TABLE_BP.iter().find(|(c, _)| *c == currency) {
        Some((_, [p, s, l])) => Ok(IrrbbShockSizes { parallel: p / 1e4, short: s / 1e4, long: l / 1e4 }),
        None => invalid(format!("no IRRBB shock sizes for {currency}; give them explicitly")),
    }
}

/// The shock of a scenario at a tenor of `t` years, as a decimal.
pub fn compute_irrbb_shock(t: f64, scenario: IrrbbScenario, sizes: &IrrbbShockSizes, x: f64) -> Result<f64, Error> {
    if !t.is_finite() || t < 0.0 {
        return invalid(format!("the tenor must be finite and not negative ({t})"));
    }
    if !(x.is_finite() && x > 0.0) {
        return invalid(format!("the decay parameter x must be positive ({x})"));
    }
    let short = sizes.short * (-t / x).exp();
    let long = sizes.long * (1.0 - (-t / x).exp());
    Ok(match scenario {
        IrrbbScenario::ParallelUp => sizes.parallel,
        IrrbbScenario::ParallelDown => -sizes.parallel,
        IrrbbScenario::Steepener => -0.65 * short.abs() + 0.9 * long.abs(),
        IrrbbScenario::Flattener => 0.8 * short.abs() - 0.6 * long.abs(),
        IrrbbScenario::ShortUp => short,
        IrrbbScenario::ShortDown => -short,
    })
}

/// One point of a shocked curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IrrbbCurvePoint {
    pub date: Date,
    pub time: f64,
    pub base_zero: f64,
    pub shock: f64,
    /// `base_zero + shock`, floored.
    pub shocked_zero: f64,
    pub floored: bool,
}

/// The base zero rates at `dates`, the shock and the floored post-shock rates. `dates` must start at the
/// curve's reference date and increase.
pub fn compute_irrbb_curve_points(
    base: &YieldCurve,
    scenario: Option<IrrbbScenario>,
    sizes: &IrrbbShockSizes,
    x: f64,
    floor: Option<f64>,
    dates: &[Date],
) -> Result<Vec<IrrbbCurvePoint>, Error> {
    if dates.len() < 2 {
        return invalid("at least two dates are needed".into());
    }
    if dates[0] != base.reference_date() {
        return invalid(format!("the first date must be the curve's reference date ({})", base.reference_date()));
    }
    if dates.windows(2).any(|w| w[1] <= w[0]) {
        return invalid("the dates must increase".into());
    }
    if let Some(f) = floor {
        if !(f.is_finite() && f <= 0.0) {
            return invalid(format!("the floor must not be greater than zero (SRP31.93), not {f}"));
        }
    }
    let dc = DayCount::Actual365Fixed(Actual365Kind::Standard);
    dates
        .iter()
        .map(|&d| {
            let time = dc.year_fraction(dates[0], d, None, None)?;
            let base_zero = base.zero_rate(d, &dc, Compounding::Continuous, 1)?.rate;
            let shock = match scenario {
                Some(s) => compute_irrbb_shock(time, s, sizes, x)?,
                None => 0.0,
            };
            let raw = base_zero + shock;
            let shocked_zero = match floor {
                Some(f) => raw.max(base_zero.min(f)),
                None => raw,
            };
            Ok(IrrbbCurvePoint { date: d, time, base_zero, shock, shocked_zero, floored: shocked_zero != raw })
        })
        .collect()
}

/// The shocked curve (or, with no scenario, the base curve rebuilt on the same dates): a zero curve on
/// `dates`, Actual/365 Fixed, continuously compounded, linear in the zero rates.
pub fn create_irrbb_shocked_curve(
    base: &YieldCurve,
    scenario: Option<IrrbbScenario>,
    sizes: &IrrbbShockSizes,
    x: f64,
    floor: Option<f64>,
    dates: &[Date],
) -> Result<YieldCurve, Error> {
    let points = compute_irrbb_curve_points(base, scenario, sizes, x, floor, dates)?;
    YieldCurve::zero(
        points.iter().map(|p| p.date).collect(),
        points.iter().map(|p| p.shocked_zero).collect(),
        DayCount::Actual365Fixed(Actual365Kind::Standard),
        Compounding::Continuous,
        1,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interest_rate::InterestRate;

    fn jpy() -> IrrbbShockSizes {
        get_irrbb_shock_sizes("JPY").unwrap()
    }

    #[test]
    fn yen_parallel_is_100bp_at_every_tenor() {
        for t in [0.0, 0.25, 1.0, 3.5, 10.0, 30.0] {
            assert!((compute_irrbb_shock(t, IrrbbScenario::ParallelUp, &jpy(), 4.0).unwrap() - 0.01).abs() < 1e-15);
            assert!((compute_irrbb_shock(t, IrrbbScenario::ParallelDown, &jpy(), 4.0).unwrap() + 0.01).abs() < 1e-15);
        }
    }

    #[test]
    fn worked_example_of_srp31_92() {
        // steepener +25.4 bp and flattener -1.6 bp at 3.5 years for the yen; short scalar 0.417
        let st = compute_irrbb_shock(3.5, IrrbbScenario::Steepener, &jpy(), 4.0).unwrap() * 1e4;
        let fl = compute_irrbb_shock(3.5, IrrbbScenario::Flattener, &jpy(), 4.0).unwrap() * 1e4;
        let su = compute_irrbb_shock(3.5, IrrbbScenario::ShortUp, &jpy(), 4.0).unwrap() * 1e4;
        assert_eq!((st * 10.0).round() / 10.0, 25.4);
        assert_eq!((fl * 10.0).round() / 10.0, -1.6);
        assert_eq!((su * 10.0).round() / 10.0, 41.7);
        let e = (-3.5f64 / 4.0).exp();
        assert!((st - (-65.0 * e + 90.0 * (1.0 - e))).abs() < 1e-12);
    }

    #[test]
    fn shocked_curve_adds_the_shock_and_respects_the_floor() {
        let d0 = Date::from_ymd(2026, 10, 6).unwrap();
        let base = YieldCurve::flat(d0, InterestRate::new(0.005, DayCount::Actual365Fixed(Actual365Kind::Standard), Compounding::Continuous, 1).unwrap());
        let dates: Vec<Date> = (0..=40).map(|k| d0.add_days(k * 365).unwrap()).collect();
        let up = compute_irrbb_curve_points(&base, Some(IrrbbScenario::ParallelUp), &jpy(), 4.0, None, &dates).unwrap();
        assert!(up.iter().all(|p| (p.shocked_zero - 0.015).abs() < 1e-12));
        // down 100 bp from 0.5% gives -0.5%; a 0% floor stops it at 0, and no floor leaves it
        let down = compute_irrbb_curve_points(&base, Some(IrrbbScenario::ParallelDown), &jpy(), 4.0, Some(0.0), &dates).unwrap();
        assert!(down.iter().all(|p| p.shocked_zero.abs() < 1e-12 && p.floored));
        let free = compute_irrbb_curve_points(&base, Some(IrrbbScenario::ParallelDown), &jpy(), 4.0, None, &dates).unwrap();
        assert!(free.iter().all(|p| (p.shocked_zero + 0.005).abs() < 1e-12));
        // a floor above zero is refused (SRP31.93)
        assert!(compute_irrbb_curve_points(&base, None, &jpy(), 4.0, Some(0.001), &dates).is_err());
        // the curve built from the points reproduces them
        let curve = create_irrbb_shocked_curve(&base, Some(IrrbbScenario::Steepener), &jpy(), 4.0, None, &dates).unwrap();
        let p = compute_irrbb_curve_points(&base, Some(IrrbbScenario::Steepener), &jpy(), 4.0, None, &dates).unwrap();
        let dc = DayCount::Actual365Fixed(Actual365Kind::Standard);
        for q in p.iter().skip(1) {
            let z = curve.zero_rate(q.date, &dc, Compounding::Continuous, 1).unwrap().rate;
            assert!((z - q.shocked_zero).abs() < 1e-12);
        }
    }
}
