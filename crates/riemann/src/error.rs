//! Errors are values that name the input which caused them.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A serial number outside the QuantLib range [367, 109574].
    SerialOutOfRange { serial: i64 },
    /// A day, month and year that do not form a valid date in range.
    InvalidDate { year: i32, month: u32, day: u32 },
    /// A year outside the range supported for the given operation.
    YearOutOfRange { year: i32 },
    /// Inconsistent schedule inputs (the same conditions QuantLib rejects).
    InvalidSchedule { reason: String },
    /// A feature specified but not implemented yet.
    Unsupported { feature: String },
    /// A reference period that a day count convention cannot use.
    InvalidReferencePeriod { reason: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::SerialOutOfRange { serial } => write!(
                f,
                "date serial number {serial} outside allowed range [367-109574], i.e. [1901-01-01, 2199-12-31]"
            ),
            Error::InvalidDate { year, month, day } => {
                write!(f, "invalid date {year:04}-{month:02}-{day:02} (allowed years 1901-2199)")
            }
            Error::YearOutOfRange { year } => {
                write!(f, "year {year} out of bounds; it must be in [1901, 2199]")
            }
            Error::InvalidSchedule { reason } => write!(f, "invalid schedule: {reason}"),
            Error::Unsupported { feature } => write!(f, "not implemented yet: {feature}"),
            Error::InvalidReferencePeriod { reason } => write!(f, "invalid reference period: {reason}"),
        }
    }
}

impl std::error::Error for Error {}
