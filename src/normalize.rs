//! Text normalization helpers shared by the importer.
//!
//! IMPORTANT: normalization here is a *provisional grouping* device. Two strings
//! that normalize to the same value are grouped under one `persons` / `places`
//! row, but that is not a claim of confirmed historical identity. The raw values
//! are always preserved in `source_records.raw` and the `raw_*` columns.

/// Values that historians use to mean "no data" in a cell. When a cell contains
/// only one of these, the *normalized* column is set to NULL — the raw cell is
/// still preserved.
const MISSING_PLACEHOLDERS: &[&str] = &[
    "-",
    "--",
    "---",
    "—",
    "–",
    ".",
    "..",
    "...",
    "?",
    "??",
    "n/a",
    "na",
    "yok",
    "bilinmiyor",
    "belirsiz",
    "null",
    "none",
    "boş",
    "bos",
    // cells where the upstream extraction failed
    "error",
    "#error",
];

/// Ayn / hamza marks as they appear in the transliterations. The same mark is
/// typed with many different code points (`maʻişet`, `ma‘işet`, `ma`işet`,
/// `maՙişet`, ...); all of them are dropped when building keys.
const TRANSLITERATION_MARKS: &[char] =
    &['ʿ', 'ʾ', 'ʻ', 'ʼ', '‘', '’', '‛', '\'', '`', '´', 'ՙ', 'ꞌ'];

/// Plausible range for `year_numeric`. The corpus is dated in Hijri (or Rumi)
/// years; anything outside this window is a transcription/extraction error
/// (e.g. a whole document tagged `1556`). Such years stay in `year_original`
/// but are not used for filtering.
pub const PLAUSIBLE_YEARS: std::ops::RangeInclusive<i32> = 600..=1350;

/// Trim a raw cell and return `None` if it is empty or a missing-data placeholder.
/// The returned string is the trimmed original (case and diacritics preserved) —
/// suitable for storing as a `canonical_name` / `raw_*` value.
///
/// ```
/// use kadi_atlas::normalize::clean_opt;
///
/// assert_eq!(clean_opt("  Üsküdar "), Some("Üsküdar".to_string()));
/// assert_eq!(clean_opt("yok"), None);
/// assert_eq!(clean_opt("   "), None);
/// ```
pub fn clean_opt(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lowered = trimmed.to_lowercase();
    if MISSING_PLACEHOLDERS.contains(&lowered.as_str()) {
        return None;
    }
    Some(trimmed.to_string())
}

/// Produce a normalized key for provisional grouping:
/// lower-cased, Turkish diacritics folded to ASCII, punctuation collapsed to
/// spaces, whitespace collapsed. Returns `None` for empty / placeholder input.
///
/// ```
/// use kadi_atlas::normalize::normalize_key;
///
/// assert_eq!(normalize_key("Şeyh-ül İslâm").as_deref(), Some("seyh ul islam"));
/// assert_eq!(normalize_key("?"), None);
/// ```
pub fn normalize_key(raw: &str) -> Option<String> {
    let cleaned = clean_opt(raw)?;

    let mut out = String::with_capacity(cleaned.len());
    let mut prev_space = true; // trims leading space
    for ch in cleaned.chars() {
        let mapped = fold_char(ch);
        for m in mapped {
            if m == ' ' {
                if !prev_space {
                    out.push(' ');
                    prev_space = true;
                }
            } else {
                out.push(m);
                prev_space = false;
            }
        }
    }
    let trimmed = out.trim_end();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Map one character to its folded form(s). Anything that is not a letter or
/// digit becomes a space.
fn fold_char(ch: char) -> Vec<char> {
    match ch {
        'ç' | 'Ç' => vec!['c'],
        'ğ' | 'Ğ' => vec!['g'],
        'ı' | 'I' => vec!['i'],
        'İ' | 'i' => vec!['i'],
        'ö' | 'Ö' => vec!['o'],
        'ş' | 'Ş' => vec!['s'],
        'ü' | 'Ü' => vec!['u'],
        'â' | 'Â' | 'à' | 'á' | 'ā' => vec!['a'],
        'î' | 'Î' | 'ï' | 'í' => vec!['i'],
        'û' | 'Û' | 'ú' | 'ū' => vec!['u'],
        'ê' | 'Ê' | 'é' | 'è' => vec!['e'],
        'ô' | 'Ô' | 'ó' => vec!['o'],
        c if TRANSLITERATION_MARKS.contains(&c) => vec![], // drop hamza/ayn markers
        c if c.is_ascii_alphanumeric() => vec![c.to_ascii_lowercase()],
        c if c.is_alphanumeric() => c.to_lowercase().collect(),
        _ => vec![' '],
    }
}

/// Fold a short vocabulary value (`position_type`, `degree`) so spelling
/// variants group together: lower-cased (Turkish-aware), ayn/hamza marks
/// dropped, a trailing izafet (`-i`, `-ı`, `-ü`, `-u`) removed. Diacritics are
/// kept, so the value stays readable. Returns `None` for blank / placeholder.
///
/// ```
/// use kadi_atlas::normalize::fold_term;
///
/// assert_eq!(fold_term("Maʻişet-i").as_deref(), Some("maişet"));
/// assert_eq!(fold_term("te’bid").as_deref(), Some("tebid"));
/// assert_eq!(fold_term("-"), None);
/// ```
pub fn fold_term(raw: &str) -> Option<String> {
    let cleaned = clean_opt(raw)?;
    let mut out = String::with_capacity(cleaned.len());
    for ch in cleaned.chars() {
        match ch {
            'I' => out.push('ı'),
            'İ' => out.push('i'),
            c if TRANSLITERATION_MARKS.contains(&c) => {}
            c => out.extend(c.to_lowercase()),
        }
    }
    let mut folded = out.trim();
    for suffix in ["-i", "-ı", "-ü", "-u"] {
        if let Some(stripped) = folded.strip_suffix(suffix) {
            folded = stripped.trim_end();
            break;
        }
    }
    clean_opt(folded)
}

/// Best-effort integer year for range filtering ONLY.
///
/// Extracts the first run of 3 or 4 ASCII digits from the raw date string. This
/// is deliberately calendar-agnostic: it does not convert Hijri/Rumi to
/// Gregorian and must not be treated as an authoritative year.
///
/// ```
/// use kadi_atlas::normalize::extract_year_numeric;
///
/// assert_eq!(extract_year_numeric("gurre-i Receb 1150"), Some(1150));
/// assert_eq!(extract_year_numeric("evahir-i Şaban"), None);
/// ```
pub fn extract_year_numeric(raw: &str) -> Option<i32> {
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let len = i - start;
            if (3..=4).contains(&len) {
                return raw[start..i].parse::<i32>().ok();
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Parse a coordinate cell into a finite `f64`, accepting comma decimal
/// separators. Returns `None` for blank / non-numeric / non-finite input.
///
/// ```
/// use kadi_atlas::normalize::parse_coordinate;
///
/// assert_eq!(parse_coordinate(" 39,9334 "), Some(39.9334));
/// assert_eq!(parse_coordinate("NaN"), None);
/// ```
pub fn parse_coordinate(raw: &str) -> Option<f64> {
    let cleaned = clean_opt(raw)?;
    let normalized = cleaned.replace(',', ".");
    let value: f64 = normalized.trim().parse().ok()?;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_become_none() {
        for p in ["", "  ", "-", "—", "?", "N/A", "yok", "  -  "] {
            assert_eq!(clean_opt(p), None, "expected None for {p:?}");
        }
        assert_eq!(clean_opt("  İstanbul "), Some("İstanbul".to_string()));
    }

    #[test]
    fn normalize_folds_turkish() {
        assert_eq!(normalize_key("İstanbul").as_deref(), Some("istanbul"));
        assert_eq!(
            normalize_key("Üsküdar Kâzâsı").as_deref(),
            Some("uskudar kazasi")
        );
        assert_eq!(
            normalize_key("el-Hâc  Ahmed  Efendi").as_deref(),
            Some("el hac ahmed efendi")
        );
        assert_eq!(normalize_key("-"), None);
        assert_eq!(normalize_key("ERROR"), None);
        // every ayn/hamza spelling folds to the same key
        for v in [
            "Maʼarretüʼn-Nuʻmân",
            "Ma‘arretü’n-Nu`mân",
            "Maarretün-Numan",
        ] {
            assert_eq!(normalize_key(v).as_deref(), Some("maarretun numan"), "{v}");
        }
    }

    #[test]
    fn vocabulary_terms_fold_variants() {
        for v in [
            "maişet",
            "maʻişet",
            "ma‘işet",
            "ma`işet",
            "maʿişet",
            "maՙişet",
            "maʻişet-i",
        ] {
            assert_eq!(fold_term(v).as_deref(), Some("maişet"), "{v}");
        }
        for v in ["tebid", "te’bid", "te'bid", "te‘bid", "te‛bid", "teꞌbid"] {
            assert_eq!(fold_term(v).as_deref(), Some("tebid"), "{v}");
        }
        assert_eq!(fold_term("Arpalık").as_deref(), Some("arpalık"));
        assert_eq!(fold_term("ilhak-ı").as_deref(), Some("ilhak"));
        assert_eq!(fold_term("ba-samine").as_deref(), Some("ba-samine"));
        assert_eq!(fold_term(" - "), None);
    }

    #[test]
    fn year_extraction_is_lenient() {
        assert_eq!(extract_year_numeric("1123"), Some(1123));
        assert_eq!(extract_year_numeric("Ramazan 1099"), Some(1099));
        assert_eq!(extract_year_numeric("H. 1234 / M. 1819"), Some(1234));
        assert_eq!(extract_year_numeric("evasıt-ı Muharrem"), None);
        assert_eq!(extract_year_numeric("12"), None);
    }

    #[test]
    fn coordinate_parsing() {
        assert_eq!(parse_coordinate("41.0082"), Some(41.0082));
        assert_eq!(parse_coordinate("28,9784"), Some(28.9784));
        assert_eq!(parse_coordinate("-"), None);
        assert_eq!(parse_coordinate("abc"), None);
    }
}
