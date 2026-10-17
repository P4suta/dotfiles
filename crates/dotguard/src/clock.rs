//! Unix seconds and proleptic Gregorian dates, which `guard_rules` decides and proves.

pub use crate::guard_rules::{
    LAST_DAY_OF_MONTH, LAST_HOUR, LAST_MINUTE, LAST_SECOND, MONTHS_PER_YEAR, SECONDS_PER_DAY,
    SECONDS_PER_HOUR, SECONDS_PER_MINUTE, civil_from_days, days_from_civil,
};
