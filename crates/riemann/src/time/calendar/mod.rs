//! Holiday calendars (QuantLib-compatible; see `docs/spec_dates_calendars.md`, chapter 3).
//!
//! A calendar is a value. Rule-based holidays come from `rules`; exceptions
//! (extra holidays, removed holidays) are data held in `CalendarAdjustments` and
//! combined with the rules by `HolidayCalendar`. Nothing is shared globally.
//!
//! Every calendar and market of QuantLib C++ v1.42 is here, and each agrees with QuantLib on
//! every valid date. That is agreement with QuantLib, not a check against actual market
//! holidays, and many calendars hold year-specific data for a limited period only
//! (`docs/spec_dates_calendars.md`, section 3.5).

mod easter;
mod rules;

use super::{Date, Weekday};
use crate::Error;
use rules::{americas, asia_pacific, europe, middle_east_africa, special, Day};
use std::collections::BTreeSet;
use std::sync::Arc;

/// Market of a United States calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UsMarket {
    Settlement,
    Nyse,
    GovernmentBond,
    Nerc,
    LiborImpact,
    FederalReserve,
    Sofr,
}

/// Market of a United Kingdom calendar. QuantLib keeps them as separate definitions, although
/// their rules are the same in v1.42.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UkMarket {
    Settlement,
    Exchange,
    Metals,
}

/// Market of an Australia calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AustraliaMarket {
    Settlement,
    Asx,
}

/// Market of an Austria calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AustriaMarket {
    Settlement,
    Exchange,
}

/// Market of a Brazil calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrazilMarket {
    Settlement,
    Exchange,
}

/// Market of a Canada calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CanadaMarket {
    Settlement,
    Tsx,
}

/// Market of a China calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChinaMarket {
    Sse,
    Ib,
}

/// Market of a France calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FranceMarket {
    Settlement,
    Exchange,
}

/// Market of a Germany calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GermanyMarket {
    Settlement,
    FrankfurtStockExchange,
    Xetra,
    Eurex,
    Euwax,
}

/// Market of an Indonesia calendar. The names are successive names of one exchange, and
/// QuantLib uses the same rules for all of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndonesiaMarket {
    Bej,
    Jsx,
    Idx,
}

/// Market of an Israel calendar. Settlement and TASE use the same rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IsraelMarket {
    Settlement,
    Tase,
    Shir,
}

/// Market of an Italy calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItalyMarket {
    Settlement,
    Exchange,
}

/// Market of a New Zealand calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NewZealandMarket {
    Wellington,
    Auckland,
}

/// Market of a Poland calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PolandMarket {
    Settlement,
    Wse,
}

/// Market of a Romania calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RomaniaMarket {
    Public,
    Bvb,
}

/// Market of a Russia calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RussiaMarket {
    Settlement,
    Moex,
}

/// Market of a South Korea calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SouthKoreaMarket {
    Settlement,
    Krx,
}

/// How a joint calendar combines its components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JointRule {
    /// A date is a holiday if it is a holiday for any component.
    JoinHolidays,
    /// A date is a business day if it is a business day for any component.
    JoinBusinessDays,
}

/// The rule set of a calendar. A country without a market value has one market in QuantLib,
/// or none; the name of that market is in the variant's comment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Calendar {
    /// No holidays and no weekends.
    NullCalendar,
    /// Saturdays and Sundays only.
    WeekendsOnly,
    /// Only the given weekend days (QuantLib `BespokeCalendar`); add holidays with
    /// `HolidayCalendar::with_holiday`.
    Bespoke { name: String, weekend: Vec<Weekday> },
    Joint(Vec<Calendar>, JointRule),

    // Americas
    /// Merval.
    Argentina,
    Brazil(BrazilMarket),
    Canada(CanadaMarket),
    /// SSE.
    Chile,
    /// BMV.
    Mexico,
    UnitedStates(UsMarket),

    // Europe
    Target,
    Austria(AustriaMarket),
    /// PSE.
    CzechRepublic,
    Denmark,
    Finland,
    France(FranceMarket),
    Germany(GermanyMarket),
    Hungary,
    /// ICEX.
    Iceland,
    Italy(ItalyMarket),
    Norway,
    Poland(PolandMarket),
    Romania(RomaniaMarket),
    Russia(RussiaMarket),
    /// BSSE.
    Slovakia,
    Sweden,
    Switzerland,
    /// USE.
    Ukraine,
    UnitedKingdom(UkMarket),

    // Middle East and Africa
    Botswana,
    Israel(IsraelMarket),
    /// Tadawul.
    SaudiArabia,
    SouthAfrica,
    Turkey,

    // Asia and the Pacific
    /// QuantLib-compatible Japan calendar (not an implementation of the public holiday law).
    Japan,
    Australia(AustraliaMarket),
    China(ChinaMarket),
    /// HKEx.
    HongKong,
    /// NSE.
    India,
    Indonesia(IndonesiaMarket),
    NewZealand(NewZealandMarket),
    /// SGX.
    Singapore,
    SouthKorea(SouthKoreaMarket),
    /// TSEC.
    Taiwan,
    Thailand,
}

impl Calendar {
    /// Whether the weekday is part of the weekend. For Saudi Arabia this is the current weekend;
    /// its business days also follow the weekend used before June 29th, 2013, as in QuantLib.
    pub fn is_weekend(&self, w: Weekday) -> bool {
        match self {
            Calendar::NullCalendar => false,
            Calendar::Bespoke { weekend, .. } => weekend.contains(&w),
            Calendar::Joint(cals, JointRule::JoinHolidays) => cals.iter().any(|c| c.is_weekend(w)),
            Calendar::Joint(cals, JointRule::JoinBusinessDays) => cals.iter().all(|c| c.is_weekend(w)),
            Calendar::Israel(IsraelMarket::Settlement | IsraelMarket::Tase) | Calendar::SaudiArabia => {
                matches!(w, Weekday::Friday | Weekday::Saturday)
            }
            _ => matches!(w, Weekday::Saturday | Weekday::Sunday),
        }
    }

    /// Business day by the calendar's rules only (no exceptions). Fails where QuantLib fails:
    /// Russia's MOEX before 2012, and Israel's calendars on the first and last days of the valid
    /// date range, whose rules look at dates outside the range.
    pub fn is_business_day(&self, d: Date) -> Result<bool, Error> {
        use Calendar::*;
        let x = Day::of(d);
        Ok(match self {
            NullCalendar => true,
            WeekendsOnly | Bespoke { .. } => !self.is_weekend(d.weekday()),
            Joint(cals, JointRule::JoinHolidays) => {
                for c in cals {
                    if !c.is_business_day(d)? {
                        return Ok(false);
                    }
                }
                true
            }
            Joint(cals, JointRule::JoinBusinessDays) => {
                for c in cals {
                    if c.is_business_day(d)? {
                        return Ok(true);
                    }
                }
                false
            }

            Argentina => americas::argentina_merval(&x),
            Brazil(BrazilMarket::Settlement) => americas::brazil_settlement(&x),
            Brazil(BrazilMarket::Exchange) => americas::brazil_exchange(&x),
            Canada(CanadaMarket::Settlement) => americas::canada_settlement(&x),
            Canada(CanadaMarket::Tsx) => americas::canada_tsx(&x),
            Chile => americas::chile_sse(&x),
            Mexico => americas::mexico_bmv(&x),
            UnitedStates(UsMarket::Settlement) => rules::us_settlement(&x),
            UnitedStates(UsMarket::Nyse) => rules::us_nyse(&x),
            UnitedStates(UsMarket::GovernmentBond) => rules::us_government_bond(&x),
            UnitedStates(UsMarket::Nerc) => rules::us_nerc(&x),
            UnitedStates(UsMarket::LiborImpact) => rules::us_libor_impact(&x),
            UnitedStates(UsMarket::FederalReserve) => rules::us_federal_reserve(&x),
            UnitedStates(UsMarket::Sofr) => rules::us_sofr(&x),

            Target => rules::target(&x),
            Austria(AustriaMarket::Settlement) => europe::austria_settlement(&x),
            Austria(AustriaMarket::Exchange) => europe::austria_exchange(&x),
            CzechRepublic => europe::czech_republic_pse(&x),
            Denmark => europe::denmark(&x),
            Finland => europe::finland(&x),
            France(FranceMarket::Settlement) => europe::france_settlement(&x),
            France(FranceMarket::Exchange) => europe::france_exchange(&x),
            Germany(GermanyMarket::Settlement) => europe::germany_settlement(&x),
            Germany(GermanyMarket::FrankfurtStockExchange) => europe::germany_frankfurt_stock_exchange(&x),
            Germany(GermanyMarket::Xetra) => europe::germany_xetra(&x),
            Germany(GermanyMarket::Eurex) => europe::germany_eurex(&x),
            Germany(GermanyMarket::Euwax) => europe::germany_euwax(&x),
            Hungary => europe::hungary(&x),
            Iceland => europe::iceland_icex(&x),
            Italy(ItalyMarket::Settlement) => europe::italy_settlement(&x),
            Italy(ItalyMarket::Exchange) => europe::italy_exchange(&x),
            Norway => europe::norway(&x),
            Poland(PolandMarket::Settlement) => europe::poland_settlement(&x),
            Poland(PolandMarket::Wse) => europe::poland_wse(&x),
            Romania(RomaniaMarket::Public) => europe::romania_public(&x),
            Romania(RomaniaMarket::Bvb) => europe::romania_bvb(&x),
            Russia(RussiaMarket::Settlement) => special::russia_settlement(&x),
            Russia(RussiaMarket::Moex) => special::russia_moex(&x)?,
            Slovakia => europe::slovakia_bsse(&x),
            Sweden => europe::sweden(&x),
            Switzerland => europe::switzerland(&x),
            Ukraine => europe::ukraine_use(&x),
            UnitedKingdom(UkMarket::Settlement | UkMarket::Exchange | UkMarket::Metals) => rules::uk_settlement(&x),

            Botswana => middle_east_africa::botswana(&x),
            Israel(IsraelMarket::Settlement | IsraelMarket::Tase) => special::israel_tel_aviv(d)?,
            Israel(IsraelMarket::Shir) => special::israel_shir(d)?,
            SaudiArabia => special::saudi_arabia_tadawul(d),
            SouthAfrica => middle_east_africa::south_africa(&x),
            Turkey => middle_east_africa::turkey(&x),

            Japan => rules::japan(&x),
            Australia(AustraliaMarket::Settlement) => asia_pacific::australia_settlement(&x),
            Australia(AustraliaMarket::Asx) => asia_pacific::australia_asx(&x),
            China(ChinaMarket::Sse) => asia_pacific::china_sse(&x),
            China(ChinaMarket::Ib) => special::china_ib(d),
            HongKong => asia_pacific::hong_kong_hkex(&x),
            India => asia_pacific::india_nse(&x),
            Indonesia(IndonesiaMarket::Bej | IndonesiaMarket::Jsx | IndonesiaMarket::Idx) => {
                asia_pacific::indonesia_bej(&x)
            }
            NewZealand(NewZealandMarket::Wellington) => asia_pacific::new_zealand_wellington(&x),
            NewZealand(NewZealandMarket::Auckland) => asia_pacific::new_zealand_auckland(&x),
            Singapore => asia_pacific::singapore_sgx(&x),
            SouthKorea(SouthKoreaMarket::Settlement) => asia_pacific::south_korea_settlement(&x),
            SouthKorea(SouthKoreaMarket::Krx) => asia_pacific::south_korea_krx(&x),
            Taiwan => asia_pacific::taiwan_tsec(&x),
            Thailand => asia_pacific::thailand_set(&x),
        })
    }

    /// The name QuantLib gives the calendar (`Calendar::name`).
    pub fn name(&self) -> String {
        use Calendar::*;
        let name = match self {
            NullCalendar => "Null",
            WeekendsOnly => "weekends only",
            Bespoke { name, .. } => return name.clone(),
            Joint(cals, rule) => {
                let names: Vec<String> = cals.iter().map(Calendar::name).collect();
                let rule = match rule {
                    JointRule::JoinHolidays => "JoinHolidays",
                    JointRule::JoinBusinessDays => "JoinBusinessDays",
                };
                return format!("{rule}({})", names.join(", "));
            }
            Argentina => "Buenos Aires stock exchange",
            Brazil(BrazilMarket::Settlement) => "Brazil",
            Brazil(BrazilMarket::Exchange) => "BOVESPA",
            Canada(CanadaMarket::Settlement) => "Canada",
            Canada(CanadaMarket::Tsx) => "TSX",
            Chile => "Santiago Stock Exchange",
            Mexico => "Mexican stock exchange",
            UnitedStates(UsMarket::Settlement) => "US settlement",
            UnitedStates(UsMarket::Nyse) => "New York stock exchange",
            UnitedStates(UsMarket::GovernmentBond) => "US government bond market",
            UnitedStates(UsMarket::Nerc) => "North American Energy Reliability Council",
            UnitedStates(UsMarket::LiborImpact) => "US with Libor impact",
            UnitedStates(UsMarket::FederalReserve) => "Federal Reserve Bankwire System",
            UnitedStates(UsMarket::Sofr) => "SOFR fixing calendar",
            Target => "TARGET",
            Austria(AustriaMarket::Settlement) => "Austrian settlement",
            Austria(AustriaMarket::Exchange) => "Vienna stock exchange",
            CzechRepublic => "Prague stock exchange",
            Denmark => "Denmark",
            Finland => "Finland",
            France(FranceMarket::Settlement) => "French settlement",
            France(FranceMarket::Exchange) => "Paris stock exchange",
            Germany(GermanyMarket::Settlement) => "German settlement",
            Germany(GermanyMarket::FrankfurtStockExchange) => "Frankfurt stock exchange",
            Germany(GermanyMarket::Xetra) => "Xetra",
            Germany(GermanyMarket::Eurex) => "Eurex",
            Germany(GermanyMarket::Euwax) => "Euwax",
            Hungary => "Hungary",
            Iceland => "Iceland stock exchange",
            Italy(ItalyMarket::Settlement) => "Italian settlement",
            Italy(ItalyMarket::Exchange) => "Milan stock exchange",
            Norway => "Norway",
            Poland(PolandMarket::Settlement) => "Poland Settlement",
            Poland(PolandMarket::Wse) => "Warsaw stock exchange",
            Romania(RomaniaMarket::Public) => "Romania",
            Romania(RomaniaMarket::Bvb) => "Bucharest stock exchange",
            Russia(RussiaMarket::Settlement) => "Russian settlement",
            Russia(RussiaMarket::Moex) => "Moscow exchange",
            Slovakia => "Bratislava stock exchange",
            Sweden => "Sweden",
            Switzerland => "Switzerland",
            Ukraine => "Ukrainian stock exchange",
            UnitedKingdom(UkMarket::Settlement) => "UK settlement",
            UnitedKingdom(UkMarket::Exchange) => "London stock exchange",
            UnitedKingdom(UkMarket::Metals) => "London metals exchange",
            Botswana => "Botswana",
            Israel(IsraelMarket::Settlement | IsraelMarket::Tase) => "Tel Aviv stock exchange",
            Israel(IsraelMarket::Shir) => "SHIR fixing calendar",
            SaudiArabia => "Tadawul",
            SouthAfrica => "South Africa",
            Turkey => "Turkey",
            Japan => "Japan",
            Australia(AustraliaMarket::Settlement) => "Australia settlement",
            Australia(AustraliaMarket::Asx) => "Australia exchange",
            China(ChinaMarket::Sse) => "Shanghai stock exchange",
            China(ChinaMarket::Ib) => "China inter bank market",
            HongKong => "Hong Kong stock exchange",
            India => "National Stock Exchange of India",
            Indonesia(_) => "Jakarta stock exchange",
            NewZealand(NewZealandMarket::Wellington) => "New Zealand (Wellington)",
            NewZealand(NewZealandMarket::Auckland) => "New Zealand (Auckland)",
            Singapore => "Singapore exchange",
            SouthKorea(SouthKoreaMarket::Settlement) => "South-Korean settlement",
            SouthKorea(SouthKoreaMarket::Krx) => "South-Korea exchange",
            Taiwan => "Taiwan stock exchange",
            Thailand => "Thailand stock exchange",
        };
        name.to_string()
    }
}

/// Exceptions to a calendar's rules: extra holidays and removed holidays.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CalendarAdjustments {
    pub added: BTreeSet<Date>,
    pub removed: BTreeSet<Date>,
}

/// A calendar together with its exceptions. Cheap to clone; never mutated in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HolidayCalendar {
    pub calendar: Calendar,
    pub adjustments: Arc<CalendarAdjustments>,
}

impl From<Calendar> for HolidayCalendar {
    fn from(calendar: Calendar) -> Self {
        HolidayCalendar { calendar, adjustments: Arc::new(CalendarAdjustments::default()) }
    }
}

impl HolidayCalendar {
    pub fn new(calendar: Calendar, adjustments: CalendarAdjustments) -> Self {
        HolidayCalendar { calendar, adjustments: Arc::new(adjustments) }
    }

    /// Same order as QuantLib: an added holiday, then a removed holiday, then the rules.
    pub fn is_business_day(&self, d: Date) -> Result<bool, Error> {
        if self.adjustments.added.contains(&d) {
            return Ok(false);
        }
        if self.adjustments.removed.contains(&d) {
            return Ok(true);
        }
        self.calendar.is_business_day(d)
    }

    pub fn is_holiday(&self, d: Date) -> Result<bool, Error> {
        Ok(!self.is_business_day(d)?)
    }

    pub fn is_weekend(&self, w: Weekday) -> bool {
        self.calendar.is_weekend(w)
    }

    /// A new calendar with `d` as a holiday (QuantLib `addHoliday`): undoes a removal of a
    /// genuine holiday, and records the date only if the rules make it a business day.
    pub fn with_holiday(&self, d: Date) -> Result<HolidayCalendar, Error> {
        let mut adj = (*self.adjustments).clone();
        adj.removed.remove(&d);
        if self.calendar.is_business_day(d)? {
            adj.added.insert(d);
        }
        Ok(HolidayCalendar::new(self.calendar.clone(), adj))
    }

    /// A new calendar with `d` as a business day (QuantLib `removeHoliday`): undoes an added
    /// holiday, and records the date only if the rules make it a holiday.
    pub fn without_holiday(&self, d: Date) -> Result<HolidayCalendar, Error> {
        let mut adj = (*self.adjustments).clone();
        adj.added.remove(&d);
        if !self.calendar.is_business_day(d)? {
            adj.removed.insert(d);
        }
        Ok(HolidayCalendar::new(self.calendar.clone(), adj))
    }

    /// Holidays between `from` and `to` inclusive (QuantLib `holidayList`).
    pub fn holidays(&self, from: Date, to: Date, include_weekends: bool) -> Result<Vec<Date>, Error> {
        let mut list = Vec::new();
        for s in from.serial()..=to.serial().max(from.serial() - 1) {
            let d = Date::from_serial(s as i64)?;
            if self.is_holiday(d)? && (include_weekends || !self.is_weekend(d.weekday())) {
                list.push(d);
            }
        }
        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_types_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Calendar>();
        assert_send_sync::<HolidayCalendar>();
    }

    #[test]
    fn adjustments_do_not_change_the_original_value() {
        let jp = HolidayCalendar::from(Calendar::Japan);
        let d = Date::from_ymd(2024, 3, 5).unwrap();
        let with = jp.with_holiday(d).unwrap();
        assert!(jp.is_business_day(d).unwrap());
        assert!(with.is_holiday(d).unwrap());
    }
}
