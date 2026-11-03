//! Overnight indexed swaps against the QuantLib R package, built with `MakeOIS`
//! (`tools/golden/p11_ois_swap.R`). All inputs are synthetic. Values within a relative 1e-11, and
//! payment dates exactly. Failures must happen in the same cases and for the same reason.

mod common;

use common::{date, day_count, golden_rows};
use riemann::curves::YieldCurve;
use riemann::names;
use riemann::ois_swap::{ois_swap_quote_sensitivity, ois_swap_value, overnight_coupon_amount, overnight_days, Fixings, OisSwap, OisSwapSpec, SwapSide};
use riemann::ois::{build_ois_curve, OisCurveInstrument, OisCurveSpec, OisInterpolation};
use riemann::time::{Period, TimeUnit};

fn curve(nodes: &str, day_count_name: &str) -> YieldCurve {
    let (dates, discounts): (Vec<_>, Vec<f64>) = nodes
        .split(' ')
        .map(|n| {
            let (d, v) = n.split_once(':').unwrap();
            (date(d), v.parse::<f64>().unwrap())
        })
        .unzip();
    YieldCurve::discount_factors(dates, discounts, day_count(day_count_name)).unwrap().with_extrapolation(true)
}

fn close(ours: f64, golden: &str, scale: f64, what: &str, worst: &mut f64, row: &[String]) {
    let theirs: f64 = golden.parse().unwrap();
    let difference = (ours - theirs).abs() / scale;
    *worst = worst.max(difference);
    assert!(difference < 1e-11, "{what}: {ours} against {theirs} in {row:?}");
}

#[test]
fn swaps_against_quantlib() {
    let rows = golden_rows("ois_swaps.csv");
    assert_eq!(rows.len(), 300);
    let (mut solved, mut failed, mut paid, mut worst) = (0, 0, 0, 0.0f64);
    for r in &rows {
        let number = |i: usize| r[i].parse::<f64>().unwrap();
        let index = names::overnight_index(&r[0]).unwrap();
        let dc_name = if matches!(r[0].as_str(), "tonar" | "sonia") { "actual_365_fixed" } else { "actual_360" };
        let swap = OisSwap::new(OisSwapSpec {
            side: if r[1] == "payer" { SwapSide::Payer } else { SwapSide::Receiver },
            nominal: number(2),
            effective_date: date(&r[3]),
            termination_date: date(&r[4]),
            index,
            fixed_rate: number(5),
            fixed_day_count: None,
            spread: number(6),
            fixed_frequency: r[7].parse().unwrap(),
            overnight_frequency: r[8].parse().unwrap(),
            payment_lag: r[9].parse().unwrap(),
        })
        .unwrap();
        let evaluation = date(&r[10]);
        let forecast = curve(&r[12], dc_name);
        let discount = if r[13] == "same" { forecast.clone() } else { curve(&r[13], dc_name) };
        let fixings: Fixings = if r[14] == "none" {
            Fixings::new()
        } else {
            r[14].split(' ').map(|n| { let (d, v) = n.split_once(':').unwrap(); (date(d), v.parse().unwrap()) }).collect()
        };
        let value = ois_swap_value(&swap, &forecast, &discount, evaluation, &fixings);
        if r[15] == "failed" {
            let message = value.expect_err("QuantLib failed here").to_string();
            assert!(message.contains("Missing"), "{message} against {}", r[27]);
            assert!(message.contains("fixing for"), "{message}");
            failed += 1;
            continue;
        }
        let v = value.unwrap_or_else(|e| panic!("{e} in {r:?}"));
        let nominal = number(2);
        close(v.npv, &r[16], nominal, "npv", &mut worst, r);
        close(v.fixed_leg_npv, &r[17], nominal, "fixed leg", &mut worst, r);
        close(v.overnight_leg_npv, &r[18], nominal, "overnight leg", &mut worst, r);
        close(v.fixed_leg_bps, &r[19], nominal, "fixed bps", &mut worst, r);
        close(v.overnight_leg_bps, &r[20], nominal, "overnight bps", &mut worst, r);
        match v.fair_rate {
            Some(rate) if r[21] != "nan" && r[21] != "NaN" => close(rate, &r[21], 1.0, "fair rate", &mut worst, r),
            _ => paid += 1,
        }
        if let (Some(spread), false) = (v.fair_spread, r[22] == "nan" || r[22] == "NaN") {
            close(spread, &r[22], 1.0, "fair spread", &mut worst, r);
        }

        // the legs
        let fixed_dates: Vec<_> = r[23].split(' ').map(date).collect();
        let fixed_amounts: Vec<&str> = r[24].split(' ').collect();
        assert_eq!(swap.fixed_leg.len(), fixed_dates.len(), "{r:?}");
        for (i, c) in swap.fixed_leg.iter().enumerate() {
            assert_eq!(c.payment_date, fixed_dates[i], "fixed payment {i} in {r:?}");
            close(c.amount().unwrap(), fixed_amounts[i], nominal, "fixed amount", &mut worst, r);
        }
        let overnight_dates: Vec<_> = r[25].split(' ').map(date).collect();
        let overnight_amounts: Vec<&str> = r[26].split(' ').collect();
        assert_eq!(swap.overnight_leg.len(), overnight_dates.len(), "{r:?}");
        for (i, c) in swap.overnight_leg.iter().enumerate() {
            assert_eq!(c.payment_date, overnight_dates[i], "overnight payment {i} in {r:?}");
            if overnight_amounts[i] != "NA" {
                let amount = overnight_coupon_amount(&swap, c, &forecast, evaluation, &fixings).unwrap();
                close(amount, overnight_amounts[i], nominal, "overnight amount", &mut worst, r);
            }
        }
        solved += 1;
    }
    println!("{solved} valued, {paid} with no fixed coupon left, {failed} failed alike; largest difference per unit of notional {worst:.3e}");
    assert!(solved > 250 && failed > 0);
}

/// LagrangeFinance II-3.3.2: QuantLib's curve after its evaluation date moved, with stale node dates
/// (`tools/golden/p11_ois_swap_moved_date.R`). Given those nodes, the swap agrees.
#[test]
fn the_legacy_swap_after_fixings_on_quantlibs_curve() {
    let rows = golden_rows("ois_swap_moved_date.csv");
    let r = &rows[0];
    let c = curve(&r[0], "actual_365_fixed");
    let swap = OisSwap::new(OisSwapSpec {
        side: SwapSide::Payer,
        nominal: 1e7,
        effective_date: riemann::Date::from_ymd(2022, 8, 23).unwrap(),
        termination_date: riemann::Date::from_ymd(2022, 8, 30).unwrap(),
        index: names::overnight_index("tonar").unwrap(),
        fixed_rate: -0.00015,
        fixed_day_count: None,
        spread: 0.0,
        fixed_frequency: 1,
        overnight_frequency: 1,
        payment_lag: 0,
    })
    .unwrap();
    let fixings: Fixings = [
        (riemann::Date::from_ymd(2022, 8, 23).unwrap(), -0.000090),
        (riemann::Date::from_ymd(2022, 8, 24).unwrap(), -0.000085),
    ]
    .into_iter()
    .collect();
    let v = ois_swap_value(&swap, &c, &c, riemann::Date::from_ymd(2022, 8, 25).unwrap(), &fixings).unwrap();
    let mut worst = 0.0f64;
    close(v.npv, &r[1], 1.0, "npv", &mut worst, r);
    close(v.fixed_leg_npv, &r[2], 1.0, "fixed", &mut worst, r);
    close(v.overnight_leg_npv, &r[3], 1.0, "overnight", &mut worst, r);
    close(v.fair_rate.unwrap(), &r[4], 1.0, "fair rate", &mut worst, r);
    println!("the legacy swap after fixings: largest difference {worst:.3e}");
}

fn example_swap(side: SwapSide) -> OisSwap {
    OisSwap::new(OisSwapSpec {
        side,
        nominal: 1e7,
        effective_date: date("46329"),
        termination_date: date("48155"),
        index: names::overnight_index("tonar").unwrap(),
        fixed_rate: 0.012,
        fixed_day_count: None,
        spread: 0.0,
        fixed_frequency: 1,
        overnight_frequency: 1,
        payment_lag: 0,
    })
    .unwrap()
}

fn example_curve(quotes: &[f64]) -> riemann::ois::OisCurve {
    let tenors = [(1, TimeUnit::Years), (3, TimeUnit::Years), (5, TimeUnit::Years), (7, TimeUnit::Years)];
    build_ois_curve(&OisCurveSpec {
        evaluation_date: date("46328"),
        reference_date: date("46328"),
        index: names::overnight_index("tonar").unwrap(),
        curve_day_count: day_count("actual_365_fixed"),
        instruments: tenors
            .iter()
            .zip(quotes)
            .map(|(&(n, unit), &rate)| OisCurveInstrument::Ois { tenor: Period::new(n, unit), rate, settlement_days: 0, payment_frequency: 1, payment_lag: 0 })
            .collect(),
        interpolation: OisInterpolation::MonotonicLogCubic,
        accuracy: 1e-12,
    })
    .unwrap()
}

#[test]
fn the_two_sides_are_opposite_and_the_fair_rate_gives_zero() {
    let quotes = [0.008, 0.01, 0.012, 0.014];
    let c = example_curve(&quotes).curve;
    let payer = ois_swap_value(&example_swap(SwapSide::Payer), &c, &c, date("46328"), &Fixings::new()).unwrap();
    let receiver = ois_swap_value(&example_swap(SwapSide::Receiver), &c, &c, date("46328"), &Fixings::new()).unwrap();
    assert!((payer.npv + receiver.npv).abs() < 1e-8);
    assert!((payer.fair_rate.unwrap() - receiver.fair_rate.unwrap()).abs() < 1e-14);
    let mut at_fair = example_swap(SwapSide::Payer).spec;
    at_fair.fixed_rate = payer.fair_rate.unwrap();
    let swap = OisSwap::new(at_fair).unwrap();
    assert!(ois_swap_value(&swap, &c, &c, date("46328"), &Fixings::new()).unwrap().npv.abs() < 1e-7);
}

#[test]
fn quote_sensitivities_agree_with_rebuilding_the_curve() {
    let quotes = [0.008, 0.01, 0.012, 0.014];
    let swap = example_swap(SwapSide::Payer);
    let sensitivity = ois_swap_quote_sensitivity(&swap, &example_curve(&quotes), date("46328"), &Fixings::new()).unwrap();
    assert_eq!(sensitivity.len(), quotes.len());
    let npv = |q: &[f64]| {
        let c = example_curve(q).curve;
        ois_swap_value(&swap, &c, &c, date("46328"), &Fixings::new()).unwrap().npv
    };
    let h = 1e-6;
    for s in &sensitivity {
        let (mut up, mut down) = (quotes, quotes);
        up[s.instrument] += h;
        down[s.instrument] -= h;
        let numeric = (npv(&up) - npv(&down)) / (2.0 * h);
        assert!((s.d_npv - numeric).abs() < 1e-3 * numeric.abs().max(1e3), "{} against {numeric}", s.d_npv);
    }
}

#[test]
fn days_use_fixings_up_to_the_evaluation_date() {
    let c = example_curve(&[0.008, 0.01, 0.012, 0.014]).curve;
    let swap = example_swap(SwapSide::Payer);
    let coupon = &swap.overnight_leg[0];
    let evaluation = coupon.value_dates[2];
    let fixings: Fixings = [(coupon.value_dates[0], 0.05), (coupon.value_dates[1], 0.06)].into_iter().collect();
    let days = overnight_days(coupon, &c, evaluation, &fixings).unwrap();
    assert_eq!(days[0].applied_rate, 0.05);
    assert_eq!(days[1].applied_rate, 0.06);
    assert_eq!(days[2].fixing, None);
    assert_eq!(days[2].applied_rate, days[2].forward_rate);
    assert_eq!(days.len(), coupon.value_dates.len() - 1);
    assert!(days.iter().all(|d| d.next_date > d.fixing_date));
}
