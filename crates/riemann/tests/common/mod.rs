//! Shared helpers for the golden tests.
#![allow(dead_code)]

use riemann::instruments::FixedRateBondSpec;
use riemann::ois::{OisCurveInstrument, OisCurveSpec, OisInterpolation, OvernightIndex};
use riemann::time::{
    Actual365Kind, ActualActualKind, AustraliaMarket, AustriaMarket, BrazilMarket, BusinessDayConvention, Calendar,
    CanadaMarket, ChinaMarket, DateGenerationRule, DayCount, FranceMarket, GermanyMarket, HolidayCalendar,
    IndonesiaMarket, IsraelMarket, ItalyMarket, JointRule, NewZealandMarket, Period, PolandMarket, RomaniaMarket,
    RussiaMarket, ScheduleSpec, SouthKoreaMarket, Thirty360Kind, TimeUnit, UkMarket, UsMarket, Weekday,
};
use riemann::Date;
use std::path::PathBuf;

/// Data rows of a golden CSV (comments and header skipped), split on commas.
pub fn golden_rows(file: &str) -> Vec<Vec<String>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden").join(file);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}; run the scripts in tools/golden", path.display()));
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').map(|s| s.trim().to_string()).collect())
        .collect()
}

pub fn date(s: &str) -> Date {
    Date::from_serial(s.parse().unwrap()).unwrap()
}

pub fn optional_date(s: &str) -> Option<Date> {
    if s == "0" {
        None
    } else {
        Some(date(s))
    }
}

pub fn convention(name: &str) -> BusinessDayConvention {
    use BusinessDayConvention::*;
    match name {
        "Following" => Following,
        "ModifiedFollowing" => ModifiedFollowing,
        "Preceding" => Preceding,
        "ModifiedPreceding" => ModifiedPreceding,
        "Unadjusted" => Unadjusted,
        other => panic!("unknown convention {other}"),
    }
}

pub fn day_count(name: &str) -> DayCount {
    match name {
        "actual_365_fixed" => DayCount::Actual365Fixed(Actual365Kind::Standard),
        "actual_360" => DayCount::Actual360 { include_last_day: false },
        "thirty_360_bond_basis" => DayCount::Thirty360(Thirty360Kind::BondBasis),
        "act_act_isma" => DayCount::ActualActual(ActualActualKind::Isma),
        "act_act_isda" => DayCount::ActualActual(ActualActualKind::Isda),
        other => panic!("unknown day counter {other}"),
    }
}

fn ois_tenor(label: &str) -> Period {
    let (n, unit) = label.split_at(label.len() - 1);
    let unit = match unit {
        "W" => TimeUnit::Weeks,
        "M" => TimeUnit::Months,
        "Y" => TimeUnit::Years,
        other => panic!("unknown unit {other}"),
    };
    Period::new(n.parse().unwrap(), unit)
}

/// OIS curve inputs from a row of a golden cases file such as `ois_tonar_cases.csv`.
pub fn ois_curve_spec(r: &[String], index: OvernightIndex, interpolation: OisInterpolation) -> OisCurveSpec {
    let instruments = r[8]
        .split(' ')
        .map(|item| {
            let (label, rate) = item.split_once(':').unwrap();
            let rate = rate.parse().unwrap();
            if label == "ON" {
                OisCurveInstrument::OvernightDeposit { rate }
            } else {
                OisCurveInstrument::Ois {
                    tenor: ois_tenor(label),
                    rate,
                    settlement_days: r[5].parse().unwrap(),
                    payment_frequency: r[7].parse().unwrap(),
                    payment_lag: r[6].parse().unwrap(),
                }
            }
        })
        .collect();
    OisCurveSpec {
        evaluation_date: date(&r[1]),
        reference_date: date(&r[2]),
        index,
        curve_day_count: day_count(&r[3]),
        instruments,
        interpolation,
        accuracy: 1.0e-12,
    }
}

/// The calendar of a key of `calendar_markets.csv`.
pub fn calendar_by_key(key: &str) -> Calendar {
    use Calendar::*;
    let three = || vec![Japan, Germany(GermanyMarket::Xetra), Brazil(BrazilMarket::Exchange)];
    let japan_moex = || vec![Japan, Russia(RussiaMarket::Moex)];
    match key {
        "null_calendar" => NullCalendar,
        "weekends_only" => WeekendsOnly,
        "bespoke_wednesday_thursday" => Bespoke {
            name: "Riemann test".to_string(),
            weekend: vec![Weekday::Wednesday, Weekday::Thursday],
        },
        "joint_three_holidays" => Joint(three(), JointRule::JoinHolidays),
        "joint_three_business_days" => Joint(three(), JointRule::JoinBusinessDays),
        "joint_japan_moex_holidays" => Joint(japan_moex(), JointRule::JoinHolidays),
        "joint_japan_moex_business_days" => Joint(japan_moex(), JointRule::JoinBusinessDays),

        "argentina_merval" => Argentina,
        "brazil_settlement" => Brazil(BrazilMarket::Settlement),
        "brazil_exchange" => Brazil(BrazilMarket::Exchange),
        "canada_settlement" => Canada(CanadaMarket::Settlement),
        "canada_tsx" => Canada(CanadaMarket::Tsx),
        "chile_sse" => Chile,
        "mexico_bmv" => Mexico,
        "us_settlement" => UnitedStates(UsMarket::Settlement),
        "us_nyse" => UnitedStates(UsMarket::Nyse),
        "us_government_bond" => UnitedStates(UsMarket::GovernmentBond),
        "us_nerc" => UnitedStates(UsMarket::Nerc),
        "us_libor_impact" => UnitedStates(UsMarket::LiborImpact),
        "us_federal_reserve" => UnitedStates(UsMarket::FederalReserve),
        "us_sofr" => UnitedStates(UsMarket::Sofr),

        "target" => Target,
        "austria_settlement" => Austria(AustriaMarket::Settlement),
        "austria_exchange" => Austria(AustriaMarket::Exchange),
        "czech_republic_pse" => CzechRepublic,
        "denmark" => Denmark,
        "finland" => Finland,
        "france_settlement" => France(FranceMarket::Settlement),
        "france_exchange" => France(FranceMarket::Exchange),
        "germany_settlement" => Germany(GermanyMarket::Settlement),
        "germany_frankfurt_stock_exchange" => Germany(GermanyMarket::FrankfurtStockExchange),
        "germany_xetra" => Germany(GermanyMarket::Xetra),
        "germany_eurex" => Germany(GermanyMarket::Eurex),
        "germany_euwax" => Germany(GermanyMarket::Euwax),
        "hungary" => Hungary,
        "iceland_icex" => Iceland,
        "italy_settlement" => Italy(ItalyMarket::Settlement),
        "italy_exchange" => Italy(ItalyMarket::Exchange),
        "norway" => Norway,
        "poland_settlement" => Poland(PolandMarket::Settlement),
        "poland_wse" => Poland(PolandMarket::Wse),
        "romania_public" => Romania(RomaniaMarket::Public),
        "romania_bvb" => Romania(RomaniaMarket::Bvb),
        "russia_settlement" => Russia(RussiaMarket::Settlement),
        "russia_moex" => Russia(RussiaMarket::Moex),
        "slovakia_bsse" => Slovakia,
        "sweden" => Sweden,
        "switzerland" => Switzerland,
        "ukraine_use" => Ukraine,
        "uk_settlement" => UnitedKingdom(UkMarket::Settlement),
        "uk_exchange" => UnitedKingdom(UkMarket::Exchange),
        "uk_metals" => UnitedKingdom(UkMarket::Metals),

        "botswana" => Botswana,
        "israel_settlement" => Israel(IsraelMarket::Settlement),
        "israel_tase" => Israel(IsraelMarket::Tase),
        "israel_shir" => Israel(IsraelMarket::Shir),
        "saudi_arabia_tadawul" => SaudiArabia,
        "south_africa" => SouthAfrica,
        "turkey" => Turkey,

        "japan" => Japan,
        "australia_settlement" => Australia(AustraliaMarket::Settlement),
        "australia_asx" => Australia(AustraliaMarket::Asx),
        "china_sse" => China(ChinaMarket::Sse),
        "china_ib" => China(ChinaMarket::Ib),
        "hong_kong_hkex" => HongKong,
        "india_nse" => India,
        "indonesia_bej" => Indonesia(IndonesiaMarket::Bej),
        "indonesia_jsx" => Indonesia(IndonesiaMarket::Jsx),
        "indonesia_idx" => Indonesia(IndonesiaMarket::Idx),
        "new_zealand_wellington" => NewZealand(NewZealandMarket::Wellington),
        "new_zealand_auckland" => NewZealand(NewZealandMarket::Auckland),
        "singapore_sgx" => Singapore,
        "south_korea_settlement" => SouthKorea(SouthKoreaMarket::Settlement),
        "south_korea_krx" => SouthKorea(SouthKoreaMarket::Krx),
        "taiwan_tsec" => Taiwan,
        "thailand" => Thailand,
        other => panic!("no Riemann calendar for the QuantLib key {other}"),
    }
}

/// Fixed-rate bond inputs from a row of `bond_cases.csv`.
pub fn bond_spec(r: &[String]) -> FixedRateBondSpec {
    let calendar = HolidayCalendar::from(match r[5].as_str() {
        "japan" => Calendar::Japan,
        "target" => Calendar::Target,
        "us_sofr" => Calendar::UnitedStates(UsMarket::Sofr),
        "null_calendar" => Calendar::NullCalendar,
        other => panic!("unknown calendar {other}"),
    });
    let unit = match r[4].as_str() {
        "Months" => TimeUnit::Months,
        "Years" => TimeUnit::Years,
        other => panic!("unknown unit {other}"),
    };
    FixedRateBondSpec {
        settlement_days: r[16].parse().unwrap(),
        face_amount: r[17].parse().unwrap(),
        schedule: ScheduleSpec {
            effective_date: date(&r[1]),
            termination_date: date(&r[2]),
            tenor: Period::new(r[3].parse().unwrap(), unit),
            calendar,
            convention: convention(&r[6]),
            termination_convention: convention(&r[7]),
            rule: if r[8] == "Backward" { DateGenerationRule::Backward } else { DateGenerationRule::Forward },
            end_of_month: r[9] == "1",
            first_date: optional_date(&r[10]),
            next_to_last_date: optional_date(&r[11]),
        },
        coupon_rate: r[12].parse().unwrap(),
        day_count: day_count(&r[13]),
        payment_convention: convention(&r[14]),
        redemption: r[15].parse().unwrap(),
        issue_date: optional_date(&r[18]),
        payment_calendar: None,
    }
}

/// Counts exact and inexact numeric matches.
#[derive(Default)]
pub struct Stats {
    pub exact: usize,
    pub inexact: usize,
    pub max_rel: f64,
    pub max_abs: f64,
}

impl Stats {
    /// True if `value` equals the golden number or is within `rel` relative difference.
    pub fn close(&mut self, golden: &str, value: f64, rel: f64) -> bool {
        let q: f64 = golden.parse().unwrap();
        if q == value {
            self.exact += 1;
            return true;
        }
        let abs = (q - value).abs();
        let r = abs / q.abs().max(1e-300);
        self.inexact += 1;
        self.max_rel = self.max_rel.max(r);
        self.max_abs = self.max_abs.max(abs);
        r <= rel
    }
}
