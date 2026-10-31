//! Dates and periods (QuantLib-compatible; see `docs/spec_dates_calendars.md`, chapter 2).

mod business_day;
pub mod calendar;
mod date;
pub mod day_count;
mod period;
pub mod schedule;

pub use business_day::BusinessDayConvention;
pub use day_count::{Actual365Kind, ActualActualKind, DayCount, Thirty360Kind};
pub use schedule::{generate as generate_schedule, DateGenerationRule, Schedule, ScheduleSpec};
pub use calendar::{
    AustraliaMarket, AustriaMarket, BrazilMarket, Calendar, CalendarAdjustments, CanadaMarket, ChinaMarket, FranceMarket,
    GermanyMarket, HolidayCalendar, IndonesiaMarket, IsraelMarket, ItalyMarket, JointRule, NewZealandMarket, PolandMarket,
    RomaniaMarket, RussiaMarket, SouthKoreaMarket, UkMarket, UsMarket,
};
pub use date::{is_leap, Date, Month, Weekday};
pub use period::{Period, TimeUnit};
