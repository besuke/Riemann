//! Holiday calendars (QuantLib-compatible; see `docs/spec_dates_calendars.md`, chapter 3).
//!
//! A calendar is a value. Rule-based holidays come from `rules`; exceptions
//! (extra holidays, removed holidays) are data held in `CalendarAdjustments` and
//! combined with the rules by `HolidayCalendar`. Nothing is shared globally.

mod easter;
mod rules;

use super::{Date, Weekday};
use std::collections::BTreeSet;
use std::sync::Arc;

/// Market of a United States calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UsMarket {
    Settlement,
    GovernmentBond,
    Sofr,
}

/// Market of a United Kingdom calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UkMarket {
    Settlement,
}

/// How a joint calendar combines its components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JointRule {
    /// A date is a holiday if it is a holiday for any component.
    JoinHolidays,
    /// A date is a business day if it is a business day for any component.
    JoinBusinessDays,
}

/// The rule set of a calendar.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Calendar {
    /// No holidays and no weekends.
    NullCalendar,
    /// Saturdays and Sundays only.
    WeekendsOnly,
    /// QuantLib-compatible Japan calendar (not an implementation of the public holiday law).
    Japan,
    Target,
    UnitedStates(UsMarket),
    UnitedKingdom(UkMarket),
    Joint(Vec<Calendar>, JointRule),
}

impl Calendar {
    /// Whether the weekday is part of the weekend.
    pub fn is_weekend(&self, w: Weekday) -> bool {
        match self {
            Calendar::NullCalendar => false,
            Calendar::Joint(cals, JointRule::JoinHolidays) => cals.iter().any(|c| c.is_weekend(w)),
            Calendar::Joint(cals, JointRule::JoinBusinessDays) => cals.iter().all(|c| c.is_weekend(w)),
            _ => matches!(w, Weekday::Saturday | Weekday::Sunday),
        }
    }

    /// Business day by the calendar's rules only (no exceptions).
    pub fn is_business_day(&self, d: Date) -> bool {
        match self {
            Calendar::NullCalendar => true,
            Calendar::WeekendsOnly => !self.is_weekend(d.weekday()),
            Calendar::Japan => rules::japan(&rules::Day::of(d)),
            Calendar::Target => rules::target(&rules::Day::of(d)),
            Calendar::UnitedStates(UsMarket::Settlement) => rules::us_settlement(&rules::Day::of(d)),
            Calendar::UnitedStates(UsMarket::GovernmentBond) => rules::us_government_bond(&rules::Day::of(d)),
            Calendar::UnitedStates(UsMarket::Sofr) => rules::us_sofr(&rules::Day::of(d)),
            Calendar::UnitedKingdom(UkMarket::Settlement) => rules::uk_settlement(&rules::Day::of(d)),
            Calendar::Joint(cals, JointRule::JoinHolidays) => cals.iter().all(|c| c.is_business_day(d)),
            Calendar::Joint(cals, JointRule::JoinBusinessDays) => cals.iter().any(|c| c.is_business_day(d)),
        }
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
    pub fn is_business_day(&self, d: Date) -> bool {
        if self.adjustments.added.contains(&d) {
            return false;
        }
        if self.adjustments.removed.contains(&d) {
            return true;
        }
        self.calendar.is_business_day(d)
    }

    pub fn is_holiday(&self, d: Date) -> bool {
        !self.is_business_day(d)
    }

    pub fn is_weekend(&self, w: Weekday) -> bool {
        self.calendar.is_weekend(w)
    }

    /// A new calendar with `d` as a holiday (QuantLib `addHoliday`): undoes a removal of a
    /// genuine holiday, and records the date only if the rules make it a business day.
    pub fn with_holiday(&self, d: Date) -> HolidayCalendar {
        let mut adj = (*self.adjustments).clone();
        adj.removed.remove(&d);
        if self.calendar.is_business_day(d) {
            adj.added.insert(d);
        }
        HolidayCalendar::new(self.calendar.clone(), adj)
    }

    /// A new calendar with `d` as a business day (QuantLib `removeHoliday`): undoes an added
    /// holiday, and records the date only if the rules make it a holiday.
    pub fn without_holiday(&self, d: Date) -> HolidayCalendar {
        let mut adj = (*self.adjustments).clone();
        adj.added.remove(&d);
        if !self.calendar.is_business_day(d) {
            adj.removed.insert(d);
        }
        HolidayCalendar::new(self.calendar.clone(), adj)
    }

    /// Holidays between `from` and `to` inclusive (QuantLib `holidayList`).
    pub fn holidays(&self, from: Date, to: Date, include_weekends: bool) -> Vec<Date> {
        (from.serial()..=to.serial().max(from.serial() - 1))
            .filter_map(|s| Date::from_serial(s as i64).ok())
            .filter(|&d| self.is_holiday(d) && (include_weekends || !self.is_weekend(d.weekday())))
            .collect()
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
        let with = jp.with_holiday(d);
        assert!(jp.is_business_day(d));
        assert!(with.is_holiday(d));
    }
}
