//! Riemann: a financial computation core built from values and pure functions.
//!
//! Date conventions follow QuantLib (see `docs/spec_dates_calendars.md`).

#![forbid(unsafe_code)]

pub mod american_approximations;
pub mod asset_swap;
pub mod autodiff;
pub mod batch;
pub mod bootstrap;
pub mod bond_curve;
pub mod cashflow_analysis;
pub mod cds;
pub mod cashflows;
pub mod curves;
pub mod error;
pub mod hull_white;
pub mod instruments;
pub mod lmm;
pub mod longstaff_schwartz;
pub mod mbs;
pub mod interest_rate;
pub mod math;
pub mod names;
pub mod risk;
pub mod sabr;
pub mod ois;
pub mod ois_swap;
pub mod options;
pub mod prdc;
pub mod time;
pub mod xccy;
pub mod yen_bond;

pub use error::Error;
pub use time::{Date, Month, Period, TimeUnit, Weekday};
