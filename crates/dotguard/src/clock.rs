//! Unix seconds and proleptic Gregorian dates, by Howard Hinnant's `days_from_civil` and `civil_from_days`.
//! The algorithms count years from March, so the leap day ends each year.

pub const SECONDS_PER_MINUTE: i64 = 60;
pub const SECONDS_PER_HOUR: i64 = 60 * SECONDS_PER_MINUTE;
pub const SECONDS_PER_DAY: i64 = 24 * SECONDS_PER_HOUR;

pub const MONTHS_PER_YEAR: i64 = 12;
pub const LAST_DAY_OF_MONTH: i64 = 31;
pub const LAST_HOUR: i64 = 23;
pub const LAST_MINUTE: i64 = 59;
/// A leap second adds one second to a minute.
pub const LAST_SECOND: i64 = 60;

/// The days 1970-01-01 lies after 0000-03-01, the start of era zero.
const EPOCH_SHIFT: i64 = 719_468;
/// The calendar repeats every 400 years of 146 097 days.
const DAYS_PER_ERA: i64 = 146_097;
const YEARS_PER_ERA: i64 = 400;
const DAYS_PER_YEAR: i64 = 365;
/// The days of four years and of a century, which mark where a leap day falls in an era.
const DAYS_PER_FOUR_YEARS: i64 = 1460;
const DAYS_PER_CENTURY: i64 = 36_524;
const YEARS_PER_LEAP: i64 = 4;
const YEARS_PER_CENTURY: i64 = 100;
/// The five months from March through July hold 153 days, which spreads month lengths evenly from March.
const DAYS_PER_FIVE_MONTHS: i64 = 153;
const MONTHS_PER_FIVE: i64 = 5;
/// The day of the year where each month's shift rounds to its first day.
const MONTH_ROUNDING: i64 = 2;
/// March counts as month zero, so January and February belong to the next civil year.
const MARCH: i64 = 3;
const FEBRUARY: i64 = 2;

/// Days since the Unix epoch of the date `year-month-day`.
#[must_use]
pub const fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= FEBRUARY { year - 1 } else { year };
    let era = if year >= 0 {
        year
    } else {
        year - (YEARS_PER_ERA - 1)
    } / YEARS_PER_ERA;
    let yoe = year - era * YEARS_PER_ERA;
    let mp = (month + MONTHS_PER_YEAR - MARCH) % MONTHS_PER_YEAR;
    let doy = (DAYS_PER_FIVE_MONTHS * mp + MONTH_ROUNDING) / MONTHS_PER_FIVE + day - 1;
    let doe = yoe * DAYS_PER_YEAR + yoe / YEARS_PER_LEAP - yoe / YEARS_PER_CENTURY + doy;
    era * DAYS_PER_ERA + doe - EPOCH_SHIFT
}

/// The date `(year, month, day)` of the day `days` after the Unix epoch.
#[must_use]
pub const fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + EPOCH_SHIFT;
    let era = if z >= 0 { z } else { z - (DAYS_PER_ERA - 1) } / DAYS_PER_ERA;
    let doe = z - era * DAYS_PER_ERA;
    let yoe = (doe - doe / DAYS_PER_FOUR_YEARS + doe / DAYS_PER_CENTURY - doe / (DAYS_PER_ERA - 1))
        / DAYS_PER_YEAR;
    let year = yoe + era * YEARS_PER_ERA;
    let doy = doe - (DAYS_PER_YEAR * yoe + yoe / YEARS_PER_LEAP - yoe / YEARS_PER_CENTURY);
    let mp = (MONTHS_PER_FIVE * doy + MONTH_ROUNDING) / DAYS_PER_FIVE_MONTHS;
    let day = doy - (DAYS_PER_FIVE_MONTHS * mp + MONTH_ROUNDING) / MONTHS_PER_FIVE + 1;
    let month = (mp + MARCH - 1) % MONTHS_PER_YEAR + 1;
    (if month <= FEBRUARY { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_dates_convert_both_ways() {
        for (days, date) in [
            (0, (1970, 1, 1)),
            (59, (1970, 3, 1)),
            (11_016, (2000, 2, 29)),
            (-1, (1969, 12, 31)),
            (20_733, (2026, 10, 7)),
        ] {
            assert_eq!(civil_from_days(days), date, "{days}");
            assert_eq!(days_from_civil(date.0, date.1, date.2), days, "{date:?}");
        }
    }

    #[test]
    fn every_day_of_four_centuries_round_trips() {
        let mut previous = civil_from_days(-DAYS_PER_ERA - 1);
        for days in -DAYS_PER_ERA..DAYS_PER_ERA {
            let date = civil_from_days(days);
            assert_eq!(days_from_civil(date.0, date.1, date.2), days);
            assert!(date > previous);
            previous = date;
        }
    }
}
