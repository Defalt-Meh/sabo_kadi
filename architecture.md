# Kadı Atlas — architecture & data model

## Goals

Let researchers query kadı appointments through time: build person and place
profiles, show origin → destination movements on a map, support historical place
names, search the source texts, and leave room for Wikidata entity matching
later. Corpus size: ~50–70k rows, delivered as XLSX.

## Shape of the system

```
XLSX ──(import_xlsx CLI, one transaction)──▶ PostgreSQL + PostGIS ──▶ Axum read-only HTTP API ──▶ web/
```

- **No ORM.** SQLx with the runtime query API (`query`, `query_as`,
  `QueryBuilder`) — no compile-time DB connection required, all user input is
  bound, never string-concatenated.
- **Write path is offline only.** The HTTP service has no write/admin routes.
- **Layers:** request tracing, security headers, request timeout, panic capture,
  optional single-origin CORS. Graceful shutdown on SIGTERM/Ctrl-C.

## Guiding principle: raw evidence is never destroyed

Historical data is uncertain. Every transformation the importer does is
*additive*:

- the entire spreadsheet row is kept verbatim in `source_records.raw` (JSONB);
- `appointments` keeps `raw_old_kadi / raw_new_kadi / raw_old_place /
  raw_new_place` and `year_original` exactly as written;
- the resolved foreign keys (`old_person_id`, `origin_place_id`, …) are a
  *convenience layer* that can be recomputed or corrected without touching the
  evidence.

## Tables

### `source_records`
One row per spreadsheet row. `doc_id` is the unique source identity the importer
is idempotent on. `raw JSONB` holds the original row. `source_text` is indexed
for full-text search (migration `002`, `to_tsvector('simple', …)` + GIN, plus a
`pg_trgm` GIN index for "contains" search).

### `persons`
`canonical_name` (first-seen spelling), `normalized_name` (folded key),
`wikidata_qid` (nullable, format-checked `^Q[1-9][0-9]*$`), `resolution_status`
∈ `provisional | under_review | confirmed | split_needed`.

**Normalization is provisional grouping, not identity.** `normalized_name` is
unique, so all mentions that fold to the same key share one `persons` row — but
that is explicitly *not* a claim that they are the same historical person. That
is what `resolution_status` (default `provisional`) is for. Splitting/merging is
a future researcher workflow.

Normalization (`src/normalize.rs`): lower-case, Turkish diacritics folded to
ASCII (`ç→c, ğ→g, ı/İ→i, ö→o, ş→s, ü→u`, plus circumflex vowels), hamza/ayn
marks dropped, everything else collapsed to single spaces.

### `places`
`canonical_name`, `normalized_name` (unique → provisional grouping, same caveat),
optional `latitude` / `longitude` (`DOUBLE PRECISION`, range-checked, all-or-
nothing via a CHECK), a generated PostGIS `geometry(Point, 4326)` `geom` kept in
sync by a `BEFORE INSERT/UPDATE` trigger, and a GiST index on `geom`.
`wikidata_qid` nullable + format-checked. Coordinates are **optional** and can be
added later; the importer fills them only when currently NULL and never
overwrites an existing pair silently.

### `place_names`
Historical / variant names: `place_id`, `name`, `normalized_name`, `valid_from`,
`valid_to` (integer years, open-ended when NULL), `calendar`, `language`,
`source_reference`. The importer inserts a minimal "attested name" row per place
(`calendar = 'unknown'`, no validity range, deduped by a partial unique index).
The validity-range columns are for researcher-curated name history added later.

### `appointments`
One event row per spreadsheet row (`source_record_id` unique → re-import
upserts). Columns:

- resolved links: `old_person_id`, `new_person_id`, `origin_place_id`,
  `destination_place_id` (all nullable);
- raw evidence: `raw_old_kadi`, `raw_new_kadi`, `raw_old_place`, `raw_new_place`;
- date: `year_original` (raw string), `calendar` (**always starts `unknown`**),
  `year_numeric` (see below);
- descriptive: `degree`, `position_type`, `period`, `salary`, `old_salary`,
  `asitane`, `infisal`, `region_raw` (kept as text — formats vary and are not
  yet specified).

Indexes: partial btree indexes on each FK, on `year_numeric`, and a composite
`(origin_place_id, destination_place_id)` for flow aggregation.

The `old_place → new_place` direction is stored, but **not** baked in as an
irreversible truth: the raw strings sit next to the ids, and the movement
semantics live only in the query layer (`/flows`, person journeys).

### `calendar_system` domain
`unknown | hijri | rumi | julian | gregorian`, default `unknown`, used by
`appointments` and `place_names`. The importer never sets anything but `unknown`.

## `year_numeric` — deliberate, documented compromise

The API needs `year_from` / `year_to` filtering and flow windows, but the source
`tarih` column is free text in an unknown calendar. `year_numeric` is a
**best-effort integer**: the first run of 3–4 digits found in `year_original`
(`"Ramazan 1125"` → `1125`, `"H. 1234 / M. 1819"` → `1234`, `"evahir-i
Muharrem"` → `NULL`). It is:

- **calendar-agnostic** — no Hijri→Gregorian conversion is performed;
- **not authoritative** — it must not be shown as "the year";
- **filter-only** — `/meta` and the field docs say so explicitly.

This is the main thing historians should review (below).

## API notes

- Consistent envelope: lists return `{ items, total, limit, offset }`; errors
  return `{ error: { code, message } }`. `sqlx::Error` is mapped so `RowNotFound`
  → 404, pool exhaustion → 503, everything else → logged + generic 500.
- `/flows` groups by the resolved origin/destination place pair, applies the
  year window to `year_numeric`, `HAVING count(*) >= min_count`, and by default
  only returns pairs where **both places are geocoded** (`require_coordinates`,
  because the primary consumer is a map). Coordinates are a property of the
  place, not the row.
- `/persons/{id}` returns the person's appointments as a `journey` in
  itinerary order, each step tagged `appointed` / `departed` / `both` with
  origin & destination coordinates, a 1-based `sequence` and
  `follows_previous` (the step starts where the previous one ended). Year alone
  cannot order same-year records (1224: B → C and C → D), so `year_numeric`
  stays the primary key but, within the earliest pending year, the step that
  departs from the person's current place wins; undated steps are pulled in
  where they continue the chain, otherwise appended. Places are linked by
  resolved id, else by normalized raw name. A record is read as the
  *incoming* kadı moving `old_place → new_place` and replacing the outgoing
  one there, so for a `departed` step the person's place is `new_place` (they
  left it) and the frontend does not draw that record as their journey. Each
  step also carries `old_person` / `new_person` (predecessor / successor) and
  `source_record_id`. This interpretation is open question 5.
- `/sources?query=` uses `websearch_to_tsquery('simple', …)` + `ts_rank` +
  `ts_headline`. `'simple'` (no stemming) is chosen for a mixed
  Ottoman-Turkish / transliteration corpus; revisit if a better dictionary is
  agreed.

## Open questions for historians

1. **Calendars.** Almost every `tarih` is presumably Hijri (or Rumi for later
   material). Confirm per-source, and decide whether/how to compute a real
   Gregorian year. Until then `calendar = unknown` and `year_numeric` is just a
   filtering aid.
2. **`year_numeric` extraction.** Is "first 3–4 digit run" acceptable? Some
   cells may carry two years (Hijri + Miladi) or ranges — which should win?
3. **Person identity.** How aggressive should name folding be? Titles/patronyms
   (`el-Hâc`, `es-Seyyid`, `Efendi`, `Zâde`) currently stay in the key, so
   "Ahmed Efendi" and "es-Seyyid Ahmed Efendi" are *different* provisional
   persons. Is that the desired default, or should honorifics be stripped?
4. **Place identity & scope.** Should `kaza`, `nahiye`, `eyalet` be one `places`
   table (current) or separated by administrative level? How to treat
   "Yenişehir-i Fener" vs "Yenişehir"?
5. **Movement semantics.** Is every row a genuine relocation of the *new* kadı
   from `old_place` to `new_place`, or do some rows just record a vacancy /
   reappointment / salary change? This affects what `/flows` actually means.
6. **`asitane` / `infisal`.** Expected values and meaning (boolean? category?
   date?) so they can be typed instead of kept as raw text.
7. **`salary` / `old_salary` / `period`.** Units and format (akçe? guruş?
   "X months")? Currently free text.
8. **`degree` / `position type`.** Is there a closed vocabulary to validate
   against?
9. **`certificate`.** What does this column contain and how should it be
   queryable?
10. **Duplicates without `doc_id`.** If the source can ship rows with no stable
    id, what is the natural key (e.g. `varak_no` + parties + year)?
