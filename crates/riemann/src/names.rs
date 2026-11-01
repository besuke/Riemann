//! The string names of choices, shared by every language entry point (specification, section 4.2).
//!
//! Names are snake_case. Calendar names are the keys of `tests/golden/calendar_markets.csv`.

use crate::cashflow_analysis::DurationType;
use crate::instruments::PriceType;
use crate::interest_rate::Compounding;
use crate::ois::{estr, sofr, sonia, tonar, OisInterpolation, OvernightIndex};
use crate::time::{
    Actual365Kind, ActualActualKind, AustraliaMarket, AustriaMarket, BrazilMarket,
    BusinessDayConvention, Calendar, CanadaMarket, ChinaMarket, DateGenerationRule, DayCount,
    FranceMarket, GermanyMarket, HolidayCalendar, IndonesiaMarket, IsraelMarket, ItalyMarket,
    NewZealandMarket, Period, PolandMarket, RomaniaMarket, RussiaMarket, SouthKoreaMarket,
    Thirty360Kind, TimeUnit, UkMarket, UsMarket,
};
use crate::Error;

fn unknown<T>(kind: &str, name: &str) -> Result<T, Error> {
    Err(Error::InvalidInput {
        reason: format!("unknown {kind} `{name}`"),
    })
}

/// Every calendar and market of QuantLib v1.42, without the joint and bespoke calendars.
pub fn calendar(name: &str) -> Result<Calendar, Error> {
    use Calendar::*;
    Ok(match name {
        "null_calendar" => NullCalendar,
        "weekends_only" => WeekendsOnly,
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
        other => return unknown("calendar", other),
    })
}

/// A day count. `business_252:<calendar>` names Business/252 on a calendar.
pub fn day_count(name: &str) -> Result<DayCount, Error> {
    if let Some(calendar_name) = name.strip_prefix("business_252:") {
        return Ok(DayCount::Business252(HolidayCalendar::from(calendar(
            calendar_name,
        )?)));
    }
    Ok(match name {
        "actual_360" => DayCount::Actual360 {
            include_last_day: false,
        },
        "actual_360_include_last_day" => DayCount::Actual360 {
            include_last_day: true,
        },
        "actual_364" => DayCount::Actual364,
        "actual_36525" => DayCount::Actual36525 {
            include_last_day: false,
        },
        "actual_36525_include_last_day" => DayCount::Actual36525 {
            include_last_day: true,
        },
        "actual_365_fixed" => DayCount::Actual365Fixed(Actual365Kind::Standard),
        "actual_365_fixed_canadian" => DayCount::Actual365Fixed(Actual365Kind::Canadian),
        "actual_365_fixed_no_leap" => DayCount::Actual365Fixed(Actual365Kind::NoLeap),
        "actual_366" => DayCount::Actual366 {
            include_last_day: false,
        },
        "actual_366_include_last_day" => DayCount::Actual366 {
            include_last_day: true,
        },
        "act_act_isda" => DayCount::ActualActual(ActualActualKind::Isda),
        "act_act_isma" => DayCount::ActualActual(ActualActualKind::Isma),
        "act_act_afb" => DayCount::ActualActual(ActualActualKind::Afb),
        "thirty_360_usa" => DayCount::Thirty360(Thirty360Kind::Usa),
        "thirty_360_bond_basis" => DayCount::Thirty360(Thirty360Kind::BondBasis),
        "thirty_360_european" => DayCount::Thirty360(Thirty360Kind::European),
        "thirty_360_italian" => DayCount::Thirty360(Thirty360Kind::Italian),
        "thirty_360_isda" => DayCount::Thirty360(Thirty360Kind::Isda { termination: None }),
        "thirty_360_nasd" => DayCount::Thirty360(Thirty360Kind::Nasd),
        "thirty_365" => DayCount::Thirty365,
        "one" => DayCount::One,
        "simple" => DayCount::Simple,
        other => return unknown("day count", other),
    })
}

pub fn compounding(name: &str) -> Result<Compounding, Error> {
    Ok(match name {
        "simple" => Compounding::Simple,
        "compounded" => Compounding::Compounded,
        "continuous" => Compounding::Continuous,
        other => return unknown("compounding", other),
    })
}

pub fn business_day_convention(name: &str) -> Result<BusinessDayConvention, Error> {
    use BusinessDayConvention::*;
    Ok(match name {
        "following" => Following,
        "modified_following" => ModifiedFollowing,
        "half_month_modified_following" => HalfMonthModifiedFollowing,
        "preceding" => Preceding,
        "modified_preceding" => ModifiedPreceding,
        "unadjusted" => Unadjusted,
        "nearest" => Nearest,
        other => return unknown("business day convention", other),
    })
}

pub fn date_generation_rule(name: &str) -> Result<DateGenerationRule, Error> {
    use DateGenerationRule::*;
    Ok(match name {
        "backward" => Backward,
        "forward" => Forward,
        "zero" => Zero,
        "third_wednesday" => ThirdWednesday,
        "third_wednesday_inclusive" => ThirdWednesdayInclusive,
        "twentieth" => Twentieth,
        "twentieth_imm" => TwentiethImm,
        "old_cds" => OldCds,
        "cds" => Cds,
        other => return unknown("date generation rule", other),
    })
}

pub fn price_type(name: &str) -> Result<PriceType, Error> {
    Ok(match name {
        "clean" => PriceType::Clean,
        "dirty" => PriceType::Dirty,
        other => return unknown("price type", other),
    })
}

pub fn duration_type(name: &str) -> Result<DurationType, Error> {
    Ok(match name {
        "simple" => DurationType::Simple,
        "modified" => DurationType::Modified,
        "macaulay" => DurationType::Macaulay,
        other => return unknown("duration type", other),
    })
}

/// The overnight indices of the OIS curves.
pub fn overnight_index(name: &str) -> Result<OvernightIndex, Error> {
    Ok(match name {
        "tonar" => tonar(),
        "estr" => estr(),
        "sofr" => sofr(),
        "sonia" => sonia(),
        other => return unknown("overnight index", other),
    })
}

pub fn ois_interpolation(name: &str) -> Result<OisInterpolation, Error> {
    Ok(match name {
        "log_linear" => OisInterpolation::LogLinear,
        "monotonic_log_cubic" => OisInterpolation::MonotonicLogCubic,
        other => return unknown("interpolation", other),
    })
}

/// A period such as `1D`, `2W`, `6M` or `10Y`.
pub fn period(name: &str) -> Result<Period, Error> {
    let split = name.len().saturating_sub(1);
    let (length, unit) = (name.get(..split), name.get(split..));
    let unit = match unit {
        Some("D") => TimeUnit::Days,
        Some("W") => TimeUnit::Weeks,
        Some("M") => TimeUnit::Months,
        Some("Y") => TimeUnit::Years,
        _ => return unknown("period", name),
    };
    match length.and_then(|n| n.parse().ok()) {
        Some(length) => Ok(Period::new(length, unit)),
        None => unknown("period", name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_names_are_errors() {
        assert!(calendar("mars").is_err());
        assert!(day_count("actual_999").is_err());
        assert!(day_count("business_252:mars").is_err());
        assert!(period("6").is_err());
        assert!(period("xM").is_err());
    }

    #[test]
    fn periods_parse() {
        assert_eq!(period("6M").unwrap(), Period::new(6, TimeUnit::Months));
        assert_eq!(period("10Y").unwrap(), Period::new(10, TimeUnit::Years));
    }
}
