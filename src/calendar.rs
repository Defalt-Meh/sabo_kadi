//! Hijri (Islamic, tabular / civil epoch) ↔ Gregorian year conversion.
//!
//! The corpus' `year_numeric` is presumably a Hijri year (see architecture.md,
//! open question 1). A Hijri year is ~354 days long, so it usually overlaps two
//! Gregorian years: 1161 H runs from 2 Jan 1748 to 20 Dec 1748, 1162 H from
//! 21 Dec 1748 to 10 Dec 1749. The functions here return that span.
//!
//! The *tabular* calendar is used (arithmetic, epoch 16 July 622 Julian). Real
//! month starts depended on moon sighting and can differ by a day or two; at
//! year resolution this only matters right at a year boundary. Dates are
//! returned in the proleptic Gregorian calendar (the corpus is post-1582).
//!
//! The same arithmetic lives in `web/js/calendar.js`; keep them in sync.

/// Julian Day Number of 1 Muharram 1 H (civil epoch).
const HIJRI_EPOCH_JDN: i64 = 1_948_440;

/// Julian Day Number of 1 Muharram of Hijri year `year`.
pub fn hijri_new_year_jdn(year: i32) -> i64 {
    let y = i64::from(year);
    (y - 1) * 354 + (3 + 11 * y).div_euclid(30) + HIJRI_EPOCH_JDN
}

/// Hijri year containing the day `jdn`.
pub fn hijri_year_of_jdn(jdn: i64) -> i32 {
    let year = (30 * (jdn - HIJRI_EPOCH_JDN) + 10_646).div_euclid(10_631);
    // the closed formula can be one year off right at a boundary
    let mut year = year as i32;
    while hijri_new_year_jdn(year + 1) <= jdn {
        year += 1;
    }
    while hijri_new_year_jdn(year) > jdn {
        year -= 1;
    }
    year
}

/// Gregorian `(year, month, day)` for a Julian Day Number.
pub fn gregorian_from_jdn(jdn: i64) -> (i32, u32, u32) {
    // Fliegel & Van Flandern
    let l = jdn + 68_569;
    let n = (4 * l).div_euclid(146_097);
    let l = l - (146_097 * n + 3).div_euclid(4);
    let i = (4_000 * (l + 1)).div_euclid(1_461_001);
    let l = l - (1_461 * i).div_euclid(4) + 31;
    let j = (80 * l).div_euclid(2_447);
    let day = l - (2_447 * j).div_euclid(80);
    let l = j.div_euclid(11);
    let month = j + 2 - 12 * l;
    let year = 100 * (n - 49) + i + l;
    (year as i32, month as u32, day as u32)
}

/// Julian Day Number of a Gregorian date.
pub fn jdn_from_gregorian(year: i32, month: u32, day: u32) -> i64 {
    let (y, m, d) = (i64::from(year), i64::from(month), i64::from(day));
    let a = (14 - m).div_euclid(12);
    let y = y + 4_800 - a;
    let m = m + 12 * a - 3;
    d + (153 * m + 2).div_euclid(5) + 365 * y + y.div_euclid(4) - y.div_euclid(100)
        + y.div_euclid(400)
        - 32_045
}

/// The Gregorian years a Hijri year overlaps: `(first, last)`, equal when the
/// whole Hijri year falls inside one Gregorian year.
pub fn hijri_to_gregorian_span(year: i32) -> (i32, i32) {
    let first = gregorian_from_jdn(hijri_new_year_jdn(year)).0;
    let last = gregorian_from_jdn(hijri_new_year_jdn(year + 1) - 1).0;
    (first, last)
}

/// Short label for the Gregorian span: `1748` or `1748/49` (or `1799/1800`).
pub fn gregorian_label(hijri_year: i32) -> String {
    let (first, last) = hijri_to_gregorian_span(hijri_year);
    if first == last {
        first.to_string()
    } else if first / 100 == last / 100 {
        format!("{first}/{:02}", last % 100)
    } else {
        format!("{first}/{last}")
    }
}

/// The Hijri years overlapping a Gregorian year: `(first, last)`.
pub fn gregorian_to_hijri_span(year: i32) -> (i32, i32) {
    (
        hijri_year_of_jdn(jdn_from_gregorian(year, 1, 1)),
        hijri_year_of_jdn(jdn_from_gregorian(year, 12, 31)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_new_years() {
        // tabular 1 Muharram dates, cross-checked against published tables
        assert_eq!(gregorian_from_jdn(hijri_new_year_jdn(1)), (622, 7, 19));
        assert_eq!(gregorian_from_jdn(hijri_new_year_jdn(1300)), (1882, 11, 12));
        assert_eq!(gregorian_from_jdn(hijri_new_year_jdn(1400)), (1979, 11, 21));
        assert_eq!(gregorian_from_jdn(hijri_new_year_jdn(1161)), (1748, 1, 2));
    }

    #[test]
    fn jdn_round_trip() {
        for jdn in [2_299_161, 2_356_000, 2_400_000, 2_451_545] {
            let (y, m, d) = gregorian_from_jdn(jdn);
            assert_eq!(jdn_from_gregorian(y, m, d), jdn);
        }
        assert_eq!(jdn_from_gregorian(2000, 1, 1), 2_451_545);
    }

    #[test]
    fn hijri_year_lookup_matches_new_years() {
        for year in [1, 900, 1069, 1161, 1162, 1274, 1400] {
            let start = hijri_new_year_jdn(year);
            assert_eq!(hijri_year_of_jdn(start), year);
            assert_eq!(hijri_year_of_jdn(start - 1), year - 1);
        }
    }

    #[test]
    fn spans_and_labels() {
        assert_eq!(hijri_to_gregorian_span(1161), (1748, 1748));
        assert_eq!(hijri_to_gregorian_span(1162), (1748, 1749));
        assert_eq!(gregorian_label(1161), "1748");
        assert_eq!(gregorian_label(1162), "1748/49");
        // 1 Jan 1748 is still the last day of 1160
        assert_eq!(gregorian_to_hijri_span(1748), (1160, 1162));
        assert_eq!(gregorian_to_hijri_span(1749), (1162, 1163));
    }
}
