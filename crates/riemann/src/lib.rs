//! Riemann: a financial computation core built from values and pure functions.
//!
//! Date conventions follow QuantLib (see `docs/spec_dates_calendars.md`).

#![forbid(unsafe_code)]

pub mod cashflow_analysis;
pub mod cashflows;
pub mod error;
pub mod instruments;
pub mod interest_rate;
pub mod math;
pub mod time;

pub use error::Error;
pub use time::{Date, Month, Period, TimeUnit, Weekday};
