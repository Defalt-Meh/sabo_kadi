// Hijri ↔ Gregorian years (tabular Islamic calendar, civil epoch) and the
// user's choice of how dates are shown. Same arithmetic as src/calendar.rs.
//
// The corpus' numeric years are presumably Hijri; the calendar of each record
// is not yet confirmed (architecture.md, open question 1). Converted years are
// therefore always marked "≈".

const STORAGE_KEY = "kadi-atlas-date-mode";
const HIJRI_EPOCH_JDN = 1948440;

export const DATE_MODES = ["hijri", "gregorian", "both"];

let mode = readMode();

export function getDateMode() {
  return mode;
}

export function setDateMode(next) {
  if (!DATE_MODES.includes(next) || next === mode) return;
  mode = next;

  try {
    localStorage.setItem(STORAGE_KEY, mode);
  } catch {
    // a private window keeps the choice for this visit only
  }

  window.dispatchEvent(new CustomEvent("kadi:datemodechange", { detail: { mode } }));
}

export function hijriNewYearJdn(year) {
  return (year - 1) * 354 + Math.floor((3 + 11 * year) / 30) + HIJRI_EPOCH_JDN;
}

export function hijriYearOfJdn(jdn) {
  let year = Math.floor((30 * (jdn - HIJRI_EPOCH_JDN) + 10646) / 10631);
  while (hijriNewYearJdn(year + 1) <= jdn) year += 1;
  while (hijriNewYearJdn(year) > jdn) year -= 1;
  return year;
}

export function gregorianYearOfJdn(jdn) {
  // Fliegel & Van Flandern, year part only
  let l = jdn + 68569;
  const n = Math.floor((4 * l) / 146097);
  l -= Math.floor((146097 * n + 3) / 4);
  const i = Math.floor((4000 * (l + 1)) / 1461001);
  l = l - Math.floor((1461 * i) / 4) + 31;
  const j = Math.floor((80 * l) / 2447);
  l = Math.floor(j / 11);
  return 100 * (n - 49) + i + l;
}

function jdnFromGregorian(year, month, day) {
  const a = Math.floor((14 - month) / 12);
  const y = year + 4800 - a;
  const m = month + 12 * a - 3;
  return day + Math.floor((153 * m + 2) / 5) + 365 * y + Math.floor(y / 4) - Math.floor(y / 100) + Math.floor(y / 400) - 32045;
}

// [first, last] Gregorian years a Hijri year overlaps.
export function hijriToGregorianSpan(year) {
  return [
    gregorianYearOfJdn(hijriNewYearJdn(year)),
    gregorianYearOfJdn(hijriNewYearJdn(year + 1) - 1)
  ];
}

// [first, last] Hijri years a Gregorian year overlaps.
export function gregorianToHijriSpan(year) {
  return [hijriYearOfJdn(jdnFromGregorian(year, 1, 1)), hijriYearOfJdn(jdnFromGregorian(year, 12, 31))];
}

// "1748" or "1748/49"
export function gregorianLabel(hijriYear) {
  const [first, last] = hijriToGregorianSpan(hijriYear);
  if (first === last) return String(first);
  return Math.floor(first / 100) === Math.floor(last / 100)
    ? `${first}/${String(last % 100).padStart(2, "0")}`
    : `${first}/${last}`;
}

// A record's date for display. `original` is the string as written in the
// source; `year` the numeric (presumed Hijri) year taken from it.
export function formatDate(year, original, display = mode) {
  const text = original ?? (Number.isFinite(year) ? String(year) : null);
  if (!Number.isFinite(year) || display === "hijri") return text;

  const converted = `≈ ${gregorianLabel(year)}`;
  return display === "gregorian" ? converted : `${text} (${converted})`;
}

// A bare year (axis, timeline, range) in the chosen mode.
export function formatYear(year, display = mode) {
  if (!Number.isFinite(year)) return "—";
  if (display === "hijri") return String(year);

  const gregorian = hijriToGregorianSpan(year)[0];
  return display === "gregorian" ? `≈ ${gregorian}` : `${year} (≈ ${gregorian})`;
}

// The Gregorian span of a Hijri range, e.g. "≈ 1658–1858".
export function gregorianRangeLabel(from, to) {
  if (!Number.isFinite(from) || !Number.isFinite(to)) return "";
  return `≈ ${hijriToGregorianSpan(from)[0]}–${hijriToGregorianSpan(to)[1]}`;
}

function readMode() {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (DATE_MODES.includes(stored)) return stored;
  } catch {
    // storage unavailable
  }
  return "hijri";
}
