//! Riemann: a financial computation core built from values and pure functions.
//!
//! Date conventions follow QuantLib (see `docs/spec_dates_calendars.md`).

#![forbid(unsafe_code)]

pub mod error;
pub mod time;

pub use error::Error;
pub use time::{Date, Month, Period, TimeUnit, Weekday};
