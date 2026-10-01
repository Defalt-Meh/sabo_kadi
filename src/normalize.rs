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
];

/// Trim a raw cell and return `None` if it is empty or a missing-data placeholder.
/// The returned string is the trimmed original (case and diacritics preserved) —
/// suitable for storing as a `canonical_name` / `raw_*` value.
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
        'ʿ' | 'ʾ' | '\'' | '`' | '’' | '‘' => vec![], // drop hamza/ayn markers
        c if c.is_ascii_alphanumeric() => vec![c.to_ascii_lowercase()],
        c if c.is_alphanumeric() => c.to_lowercase().collect(),
        _ => vec![' '],
    }
}

/// Best-effort integer year for range filtering ONLY.
///
/// Extracts the first run of 3 or 4 ASCII digits from the raw date string. This
/// is deliberately calendar-agnostic: it does not convert Hijri/Rumi to
/// Gregorian and must not be treated as an authoritative year.
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
