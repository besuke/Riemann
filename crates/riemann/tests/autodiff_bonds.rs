//! Sensitivities by automatic differentiation against analytic formulas and numeric differences.
//! All inputs are synthetic.

use riemann::cashflow_analysis::DurationType;
use riemann::curves::{CurveInput, LogDiscountInterpolation, YieldCurve};
use riemann::instruments::{FixedRateBond, FixedRateBondSpec};
use riemann::interest_rate::{Compounding, InterestRate};
use riemann::ois::{
    build_ois_curve, estr, sofr, tonar, OisCurveInstrument, OisCurveSpec, OisInterpolation,
};
use riemann::risk::{curve_sensitivity, quote_sensitivity, yield_sensitivity};
use riemann::time::{
    Actual365Kind, BusinessDayConvention, Calendar, Date, DateGenerationRule, DayCount,
    HolidayCalendar, Month, Period, ScheduleSpec, TimeUnit,
};

fn act365() -> DayCount {
    DayCount::Actual365Fixed(Actual365Kind::Standard)
}

fn date(y: i32, m: Month, d: u32) -> Date {
    Date::from_ymd(y, m as u32, d).unwrap()
}

fn bond(coupon: f64, years: i32) -> FixedRateBond {
    let calendar = HolidayCalendar::from(Calendar::Japan);
    FixedRateBond::new(FixedRateBondSpec {
        settlement_days: 2,
        face_amount: 100.0,
        schedule: ScheduleSpec {
            effective_date: date(2025, Month::March, 20),
            termination_date: date(2025 + years, Month::March, 20),
            tenor: Period::new(6, TimeUnit::Months),
            calendar,
            convention: BusinessDayConvention::Unadjusted,
            termination_convention: BusinessDayConvention::Unadjusted,
            rule: DateGenerationRule::Backward,
            end_of_month: false,
            first_date: None,
            next_to_last_date: None,
        },
        coupon_rate: coupon,
        day_count: act365(),
        payment_convention: BusinessDayConvention::Unadjusted,
        redemption: 100.0,
        issue_date: None,
        payment_calendar: None,
    })
    .unwrap()
}

fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance * (1.0 + b.abs())
}

#[test]
fn yield_derivatives_agree_with_duration_and_convexity() {
    let settlement = date(2026, Month::November, 4);
    for (coupon, years) in [(0.0, 5), (0.015, 5), (0.03, 20), (0.08, 10)] {
        let b = bond(coupon, years);
        for rate in [0.001, 0.02, 0.07] {
            for (compounding, frequency) in [
                (Compounding::Compounded, 2),
                (Compounding::Compounded, 1),
                (Compounding::Continuous, 0),
            ] {
                let y = InterestRate::new(rate, act365(), compounding, frequency).unwrap();
                let s = yield_sensitivity(&b, &y, settlement).unwrap();
                let price = b.dirty_price(&y, settlement).unwrap();
                assert_eq!(s.dirty_price, price);
                let modified = b.duration(&y, DurationType::Modified, settlement).unwrap();
                let convexity = b.convexity(&y, settlement).unwrap();
                assert!(close(s.d_price_d_yield, -modified * price, 1e-11), "first");
                assert!(close(s.d2_price_d_yield2, convexity * price, 1e-11), "second");
            }
        }
    }
}

fn curve_inputs() -> Vec<(Date, f64)> {
    vec![
        (date(2026, Month::November, 2), 1.0),
        (date(2027, Month::November, 2), 0.985),
        (date(2029, Month::November, 2), 0.95),
        (date(2031, Month::November, 3), 0.91),
        (date(2036, Month::November, 3), 0.82),
    ]
}

fn discount_curve(
    points: &[(Date, f64)],
    interpolation: LogDiscountInterpolation,
) -> YieldCurve {
    YieldCurve::interpolated_discounts(
        points.iter().map(|p| p.0).collect(),
        points.iter().map(|p| p.1).collect(),
        act365(),
        interpolation,
    )
    .unwrap()
}

#[test]
fn node_sensitivities_agree_with_numeric_differences() {
    let evaluation = date(2026, Month::November, 2);
    let b = bond(0.02, 8);
    let points = curve_inputs();
    for interpolation in [
        LogDiscountInterpolation::LogLinear,
        LogDiscountInterpolation::MonotonicLogCubic,
    ] {
        let curve = discount_curve(&points, interpolation);
        let (value, rows) = curve_sensitivity(&b, &curve, evaluation).unwrap();
        assert_eq!(value, b.value_on_curve(&curve, evaluation).unwrap());
        assert_eq!(rows.len(), points.len() - 1);
        for (i, row) in rows.iter().enumerate() {
            let bumped = |h: f64| {
                let mut p = points.clone();
                p[i + 1].1 *= h.exp();
                b.value_on_curve(&discount_curve(&p, interpolation), evaluation)
                    .unwrap()
            };
            let h = 1e-5;
            let (up, down) = (bumped(h), bumped(-h));
            let npv = (up.npv - down.npv) / (2.0 * h);
            let price = (up.dirty_price - down.dirty_price) / (2.0 * h);
            assert!(close(row.d_npv, npv, 1e-7), "{i}: {} {npv}", row.d_npv);
            assert!(close(row.d_dirty_price, price, 1e-7), "{i}");
            assert!(matches!(row.input, CurveInput::LogDiscount { .. }));
        }
    }
}

#[test]
fn flat_zero_and_spreaded_curves_agree_with_numeric_differences() {
    let evaluation = date(2026, Month::November, 2);
    let reference = evaluation;
    let b = bond(0.02, 8);
    let d = |n: &[f64]| {
        let zero = YieldCurve::zero(
            vec![
                reference,
                date(2029, Month::November, 2),
                date(2036, Month::November, 3),
            ],
            vec![n[0], n[1], n[2]],
            act365(),
            Compounding::Continuous,
            0,
        )
        .unwrap();
        YieldCurve::zero_spreaded(zero, n[3], Compounding::Continuous, 0).unwrap()
    };
    let point = [0.01, 0.015, 0.02, 0.002];
    let (_, rows) = curve_sensitivity(&b, &d(&point), evaluation).unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[3].input, CurveInput::ZeroSpread);
    for (i, row) in rows.iter().enumerate() {
        let value = |h: f64| {
            let mut p = point;
            p[i] += h;
            b.value_on_curve(&d(&p), evaluation).unwrap().npv
        };
        let numeric = (value(1e-6) - value(-1e-6)) / 2e-6;
        assert!(close(row.d_npv, numeric, 1e-6), "{i}: {} {numeric}", row.d_npv);
    }

    let flat = |r: f64| {
        YieldCurve::flat(reference, InterestRate::new(r, act365(), Compounding::Compounded, 2).unwrap())
    };
    let (_, rows) = curve_sensitivity(&b, &flat(0.012), evaluation).unwrap();
    let numeric = (b.value_on_curve(&flat(0.012 + 1e-6), evaluation).unwrap().npv
        - b.value_on_curve(&flat(0.012 - 1e-6), evaluation).unwrap().npv)
        / 2e-6;
    assert!(close(rows[0].d_npv, numeric, 1e-7));
    assert_eq!(rows[0].input, CurveInput::FlatRate);
}

fn ois_spec(
    index: riemann::ois::OvernightIndex,
    quotes: &[(i32, f64)],
    interpolation: OisInterpolation,
) -> OisCurveSpec {
    let evaluation = date(2026, Month::November, 2);
    let mut instruments = vec![OisCurveInstrument::OvernightDeposit { rate: quotes[0].1 }];
    for &(years, rate) in &quotes[1..] {
        instruments.push(OisCurveInstrument::Ois {
            tenor: Period::new(years, TimeUnit::Years),
            rate,
            settlement_days: 2,
            payment_frequency: 1,
            payment_lag: 1,
        });
    }
    OisCurveSpec {
        evaluation_date: evaluation,
        reference_date: evaluation,
        index,
        curve_day_count: act365(),
        instruments,
        interpolation,
        accuracy: 1e-12,
    }
}

#[test]
fn quote_sensitivities_agree_with_rebuilding_the_curve() {
    let quotes = [(0, 0.002), (1, 0.004), (2, 0.006), (5, 0.011), (10, 0.016), (20, 0.02)];
    let b = bond(0.02, 8);
    let evaluation = date(2026, Month::November, 2);
    for interpolation in [OisInterpolation::LogLinear, OisInterpolation::MonotonicLogCubic] {
        for index in [tonar(), estr(), sofr()] {
            let spec = ois_spec(index.clone(), &quotes, interpolation);
            let ois = build_ois_curve(&spec).unwrap();
            let (value, rows) = quote_sensitivity(&b, &ois, evaluation).unwrap();
            assert_eq!(value, b.value_on_curve(&ois.curve, evaluation).unwrap());
            assert_eq!(rows.len(), quotes.len());
            for row in &rows {
                let npv_with = |h: f64| {
                    let mut q = quotes;
                    q[row.instrument].1 += h;
                    let ois = build_ois_curve(&ois_spec(index.clone(), &q, interpolation)).unwrap();
                    b.value_on_curve(&ois.curve, evaluation).unwrap()
                };
                let h = 1e-6;
                let (up, down) = (npv_with(h), npv_with(-h));
                let numeric = (up.npv - down.npv) / (2.0 * h);
                let price = (up.dirty_price - down.dirty_price) / (2.0 * h);
                // the rebuilt curves solve to 1e-12 on discount factors, so the difference
                // quotient carries noise of about 1e-6 per 100 of notional
                assert!(close(row.d_npv, numeric, 2e-5), "{interpolation:?}: {} {numeric}", row.d_npv);
                assert!(close(row.d_dirty_price, price, 2e-5));
            }
        }
    }
}

#[test]
fn the_jacobian_reproduces_a_shift_of_all_quotes() {
    // log-linear: the derivative of a node with respect to the quotes of later nodes is 0
    let quotes = [(0, 0.002), (1, 0.004), (2, 0.006), (5, 0.011)];
    let spec = ois_spec(tonar(), &quotes, OisInterpolation::LogLinear);
    let jacobian = build_ois_curve(&spec).unwrap().quote_jacobian().unwrap();
    for i in 0..jacobian.d_log_discount_d_quote.len() {
        for k in i + 1..jacobian.d_log_discount_d_quote.len() {
            assert!(jacobian.d_log_discount_d_quote[i][k].abs() < 1e-12);
        }
        assert!(jacobian.d_log_discount_d_quote[i][i] < 0.0);
    }
}
