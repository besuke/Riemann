//! Western (Gregorian) Easter, computed with the anonymous Gregorian algorithm
//! (Meeus/Jones/Butcher). QuantLib keeps the same dates as a table for 1901-2199;
//! agreement is checked against QuantLib by the golden calendar tests.

/// Day of the year (January 1st = 1) of Easter Monday.
pub fn easter_monday_day_of_year(year: i32) -> u32 {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31; // 3 = March, 4 = April
    let day = ((h + l - 7 * m + 114) % 31) + 1; // Easter Sunday
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let day_of_year_of_sunday = if month == 3 { 59 + leap as i32 + day } else { 90 + leap as i32 + day };
    (day_of_year_of_sunday + 1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_quantlib_table_samples() {
        // QuantLib's EasterMonday table: 1901 -> 98, 2000 -> 115, 2024 -> 92, 2199 -> 105.
        assert_eq!(easter_monday_day_of_year(1901), 98);
        assert_eq!(easter_monday_day_of_year(2000), 115);
        assert_eq!(easter_monday_day_of_year(2024), 92);
        assert_eq!(easter_monday_day_of_year(2199), 105);
    }
}
