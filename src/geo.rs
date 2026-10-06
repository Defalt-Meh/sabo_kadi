//! Coordinate validation.
//!
//! Coordinates are delivered by a third party (a gazetteer sheet matched
//! against Wikidata) and are **not trusted blindly**. Every candidate point is
//! assessed here; only an [`CoordinateStatus::Accepted`] point is ever copied
//! onto a place. The verdict and the reasons are stored next to the raw values
//! so a researcher can see why a place has no coordinates.

use crate::normalize::{normalize_key, parse_coordinate};

/// Bounding box of the study area: the Ottoman world at its widest — Algiers
/// to Baghdad/Basra/Tabriz, Yemen to Buda/Kamianets/Azov. A point outside it is
/// a wrong match (e.g. `Kudüs` → Kudus, Indonesia; `Peçin` → Pěčín, Czechia).
pub const STUDY_LATITUDE: std::ops::RangeInclusive<f64> = 10.0..=49.5;
pub const STUDY_LONGITUDE: std::ops::RangeInclusive<f64> = -10.0..=60.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateStatus {
    /// Used for the map.
    Accepted,
    /// Plausible but not a settlement-level point; not used until reviewed.
    NeedsReview,
    /// Unusable.
    Rejected,
}

impl CoordinateStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::NeedsReview => "needs_review",
            Self::Rejected => "rejected",
        }
    }
}

/// Reasons attached to a candidate point. The blocking ones decide the status;
/// the informational ones are only stored for review.
pub mod issue {
    // -> rejected
    pub const MISSING: &str = "missing_coordinates";
    pub const UNPARSEABLE: &str = "unparseable_coordinates";
    pub const OUT_OF_RANGE: &str = "out_of_range";
    pub const POSSIBLY_SWAPPED: &str = "possibly_swapped";
    pub const OUTSIDE_STUDY_REGION: &str = "outside_study_region";
    // -> needs_review
    pub const LOW_PRECISION: &str = "low_precision";
    pub const AREA_NOT_SETTLEMENT: &str = "area_not_settlement";
    pub const CONFLICTING_DUPLICATE: &str = "conflicting_duplicate";
    // informational
    pub const NAME_MISMATCH: &str = "name_mismatch";
    pub const SHARED_COORDINATES: &str = "shared_coordinates";
    pub const SHARED_WIKIDATA: &str = "shared_wikidata";
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assessment {
    pub status: CoordinateStatus,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub issues: Vec<&'static str>,
}

impl Assessment {
    /// Add a review-level issue (cannot upgrade a rejection).
    pub fn flag_for_review(&mut self, reason: &'static str) {
        self.push_issue(reason);
        if self.status == CoordinateStatus::Accepted {
            self.status = CoordinateStatus::NeedsReview;
        }
    }

    pub fn push_issue(&mut self, reason: &'static str) {
        if !self.issues.contains(&reason) {
            self.issues.push(reason);
        }
    }
}

pub fn in_study_region(lat: f64, lon: f64) -> bool {
    STUDY_LATITUDE.contains(&lat) && STUDY_LONGITUDE.contains(&lon)
}

/// Great-circle distance in km between two `(lat, lon)` points.
pub fn distance_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la1, lo1) = (a.0.to_radians(), a.1.to_radians());
    let (la2, lo2) = (b.0.to_radians(), b.1.to_radians());
    let h = ((la2 - la1) / 2.0).sin().powi(2)
        + la1.cos() * la2.cos() * ((lo2 - lo1) / 2.0).sin().powi(2);
    2.0 * 6371.0 * h.sqrt().asin()
}

/// Words in a matched entity name that mean "an area, not a town".
const AREA_WORDS: &[&str] = &[
    "governorate",
    "province",
    "eyalet",
    "eyaleti",
    "vilayet",
    "vilayeti",
    "sanjak",
    "sancak",
    "region",
    "plain",
    "mountain",
    "mountains",
    "river",
    "lake",
    "valley",
];

/// Assess one gazetteer row.
///
/// * `raw_lat` / `raw_lon` — the cells as delivered;
/// * `original` — the historical name the row is for;
/// * `matched` / `country` — what the upstream matcher resolved it to.
pub fn assess(
    raw_lat: Option<&str>,
    raw_lon: Option<&str>,
    original: &str,
    matched: Option<&str>,
    country: Option<&str>,
) -> Assessment {
    let rejected = |issue, lat, lon| Assessment {
        status: CoordinateStatus::Rejected,
        latitude: lat,
        longitude: lon,
        issues: vec![issue],
    };
    // Outside the box: say so, and add a hint when the swapped pair would fit.
    let unusable = |reason, lat: f64, lon: f64| {
        let mut a = rejected(reason, Some(lat), Some(lon));
        if in_study_region(lon, lat) {
            a.push_issue(issue::POSSIBLY_SWAPPED);
        }
        a
    };

    let (Some(raw_lat), Some(raw_lon)) = (raw_lat, raw_lon) else {
        return rejected(issue::MISSING, None, None);
    };
    let (Some(lat), Some(lon)) = (parse_coordinate(raw_lat), parse_coordinate(raw_lon)) else {
        return rejected(issue::UNPARSEABLE, None, None);
    };
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return unusable(issue::OUT_OF_RANGE, lat, lon);
    }
    if !in_study_region(lat, lon) {
        return unusable(issue::OUTSIDE_STUDY_REGION, lat, lon);
    }

    let mut a = Assessment {
        status: CoordinateStatus::Accepted,
        latitude: Some(lat),
        longitude: Some(lon),
        issues: Vec::new(),
    };

    // `32.` / `53` — a country or region centroid, not a town.
    if decimals(raw_lat) == 0 || decimals(raw_lon) == 0 {
        a.flag_for_review(issue::LOW_PRECISION);
    }

    if let Some(matched) = matched {
        if is_area_name(matched, country) {
            a.flag_for_review(issue::AREA_NOT_SETTLEMENT);
        }
        if names_differ(original, matched) {
            a.push_issue(issue::NAME_MISMATCH);
        }
    }

    a
}

/// Number of digits after the decimal separator in a raw coordinate cell.
fn decimals(raw: &str) -> usize {
    let t = raw.trim();
    match t.find(['.', ',']) {
        Some(i) => t[i + 1..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .count(),
        None => 0,
    }
}

fn is_area_name(matched: &str, country: Option<&str>) -> bool {
    let Some(key) = normalize_key(matched) else {
        return false;
    };
    if key.split(' ').any(|w| AREA_WORDS.contains(&w)) {
        return true;
    }
    // the "place" resolved to a whole country (`İran` -> Iran)
    country
        .into_iter()
        .flat_map(|c| c.split('|'))
        .filter_map(normalize_key)
        .any(|c| c == key)
}

/// Loose name comparison: same key, one containing the other, or a bigram
/// Dice similarity of at least 0.5 count as "the same name". Renames
/// (`Larende` → Karaman) are flagged for a human to confirm, not rejected.
fn names_differ(original: &str, matched: &str) -> bool {
    let (Some(a), Some(b)) = (normalize_key(original), normalize_key(matched)) else {
        return false;
    };
    if a == b || a.contains(&b) || b.contains(&a) {
        return false;
    }
    dice(&a, &b) < 0.5
}

fn dice(a: &str, b: &str) -> f64 {
    let grams = |s: &str| -> Vec<(char, char)> {
        let cs: Vec<char> = s.chars().filter(|c| *c != ' ').collect();
        cs.windows(2).map(|w| (w[0], w[1])).collect()
    };
    let ga = grams(a);
    let mut gb = grams(b);
    if ga.is_empty() || gb.is_empty() {
        return 0.0;
    }
    let total = ga.len() + gb.len();
    let mut shared = 0;
    for g in &ga {
        if let Some(pos) = gb.iter().position(|x| x == g) {
            gb.swap_remove(pos);
            shared += 1;
        }
    }
    2.0 * shared as f64 / total as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(a: &Assessment) -> &'static str {
        a.status.as_str()
    }

    #[test]
    fn good_points_are_accepted() {
        let a = assess(
            Some("36.2025"),
            Some("36.1605"),
            "Antakya",
            Some("Antakya"),
            Some("Turkey"),
        );
        assert_eq!(st(&a), "accepted");
        assert!(a.issues.is_empty());
        assert_eq!((a.latitude, a.longitude), (Some(36.2025), Some(36.1605)));

        // renamed town: accepted, but the mismatch is recorded
        let a = assess(
            Some("37.1833"),
            Some("33.2166"),
            "Larende",
            Some("Karaman"),
            Some("Turkey"),
        );
        assert_eq!(st(&a), "accepted");
        assert_eq!(a.issues, vec![issue::NAME_MISMATCH]);

        // transliteration variants are not a mismatch
        let a = assess(
            Some("38.2"),
            Some("44.7666"),
            "Selmas",
            Some("Salmas"),
            Some("Iran"),
        );
        assert!(a.issues.is_empty(), "{:?}", a.issues);
    }

    #[test]
    fn wrong_matches_outside_the_study_region_are_rejected() {
        // Kudüs matched to Kudus, Indonesia
        let a = assess(
            Some("-6.8"),
            Some("11.0866"),
            "Kudüs",
            Some("Kudus"),
            Some("Indonesia"),
        );
        assert_eq!(st(&a), "rejected");
        assert_eq!(a.issues[0], issue::OUTSIDE_STUDY_REGION);

        // Peçin matched to Pěčín, Czech Republic
        let a = assess(
            Some("50.1538"),
            Some("16.4247"),
            "Peçin",
            Some("Pěčín"),
            None,
        );
        assert_eq!(st(&a), "rejected");
    }

    #[test]
    fn swapped_and_broken_values_are_rejected() {
        let a = assess(Some("36.1605"), Some("8.5"), "X", None, None);
        assert_eq!(st(&a), "accepted"); // Tunisia-ish, still inside the box

        let a = assess(Some("53.0"), Some("32.0"), "X", None, None);
        assert_eq!(
            a.issues,
            vec![issue::OUTSIDE_STUDY_REGION, issue::POSSIBLY_SWAPPED]
        );
        assert_eq!(st(&a), "rejected");

        let a = assess(Some("abc"), Some("32"), "X", None, None);
        assert_eq!(a.issues, vec![issue::UNPARSEABLE]);

        let a = assess(None, Some("32"), "X", None, None);
        assert_eq!(a.issues, vec![issue::MISSING]);

        let a = assess(Some("0"), Some("0"), "X", None, None);
        assert_eq!(st(&a), "rejected");

        let a = assess(Some("95"), Some("38"), "X", None, None);
        assert_eq!(a.issues, vec![issue::OUT_OF_RANGE]);
    }

    #[test]
    fn coarse_or_area_points_need_review() {
        // country centroid
        let a = assess(Some("32."), Some("53."), "İran", Some("Iran"), Some("Iran"));
        assert_eq!(st(&a), "needs_review");
        assert!(a.issues.contains(&issue::LOW_PRECISION));
        assert!(a.issues.contains(&issue::AREA_NOT_SETTLEMENT));

        // governorate centroid
        let a = assess(
            Some("34.35"),
            Some("38.31"),
            "Humus",
            Some("Homs Governorate"),
            Some("Syria"),
        );
        assert_eq!(st(&a), "needs_review");
        assert!(a.issues.contains(&issue::AREA_NOT_SETTLEMENT));

        let a = assess(
            Some("38.7808"),
            Some("30.3222"),
            "Sincanlı",
            Some("Plain of Sincanlı"),
            None,
        );
        assert_eq!(st(&a), "needs_review");

        // comma decimals are fine
        let a = assess(
            Some("39,9334"),
            Some("32,8597"),
            "Ankara",
            Some("Ankara"),
            None,
        );
        assert_eq!(st(&a), "accepted");
    }
}
