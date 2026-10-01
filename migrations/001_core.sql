-- Kadı Atlas — core schema
--
-- Design notes for historians / maintainers:
--  * Every imported spreadsheet row is preserved verbatim in `source_records.raw`
--    (JSONB). Nothing below is a lossy replacement for that raw record.
--  * Name normalization is a *provisional grouping* aid only. Two rows that share
--    a `normalized_name` are grouped under one `persons` / `places` row, but this
--    is NOT a claim of confirmed identity. See `persons.resolution_status`.
--  * The `old_* -> new_*` movement in `appointments` is stored with the raw
--    strings intact (`raw_old_*`, `raw_new_*`). The resolved *_id columns are a
--    convenience layer and may be revised without touching the raw evidence.
--  * Calendars are never guessed. `calendar` defaults to 'unknown'.

CREATE EXTENSION IF NOT EXISTS postgis;
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Allowed calendar systems. 'unknown' is the mandatory default until a historian
-- confirms otherwise.
CREATE DOMAIN calendar_system AS TEXT
    NOT NULL
    DEFAULT 'unknown'
    CONSTRAINT calendar_system_allowed
        CHECK (VALUE IN ('unknown', 'hijri', 'rumi', 'julian', 'gregorian'));

-- Generic updated_at maintenance.
CREATE OR REPLACE FUNCTION set_updated_at() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END;
$$;

-- ---------------------------------------------------------------------------
-- source_records: the original spreadsheet row, kept losslessly.
-- ---------------------------------------------------------------------------
CREATE TABLE source_records (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    doc_id      TEXT NOT NULL,
    varak_no    TEXT,
    certificate TEXT,
    source_text TEXT,
    raw         JSONB NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Source identity: the importer is idempotent on doc_id.
CREATE UNIQUE INDEX source_records_doc_id_key ON source_records (doc_id);

CREATE TRIGGER source_records_set_updated_at
    BEFORE UPDATE ON source_records
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ---------------------------------------------------------------------------
-- persons: provisional grouping of name mentions.
-- ---------------------------------------------------------------------------
CREATE TABLE persons (
    id                BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    canonical_name    TEXT NOT NULL,
    normalized_name   TEXT NOT NULL,
    wikidata_qid      TEXT,
    -- provisional  : auto-grouped by normalized_name, unreviewed
    -- under_review : a researcher is currently assessing this grouping
    -- confirmed    : a researcher confirmed this is a single real person
    -- split_needed : grouping is known to conflate several people
    resolution_status TEXT NOT NULL DEFAULT 'provisional'
        CHECK (resolution_status IN ('provisional', 'under_review', 'confirmed', 'split_needed')),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT persons_wikidata_qid_format
        CHECK (wikidata_qid IS NULL OR wikidata_qid ~ '^Q[1-9][0-9]*$')
);

CREATE UNIQUE INDEX persons_normalized_name_key ON persons (normalized_name);
CREATE INDEX persons_canonical_name_trgm ON persons USING gin (canonical_name gin_trgm_ops);
CREATE INDEX persons_wikidata_qid_idx ON persons (wikidata_qid) WHERE wikidata_qid IS NOT NULL;

CREATE TRIGGER persons_set_updated_at
    BEFORE UPDATE ON persons
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ---------------------------------------------------------------------------
-- places: provisional grouping of place mentions, optional coordinates.
-- ---------------------------------------------------------------------------
CREATE TABLE places (
    id              BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    canonical_name  TEXT NOT NULL,
    normalized_name TEXT NOT NULL,
    latitude        DOUBLE PRECISION,
    longitude       DOUBLE PRECISION,
    geom            geometry(Point, 4326),
    wikidata_qid    TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT places_latitude_range
        CHECK (latitude IS NULL OR latitude BETWEEN -90 AND 90),
    CONSTRAINT places_longitude_range
        CHECK (longitude IS NULL OR longitude BETWEEN -180 AND 180),
    -- coordinates are all-or-nothing
    CONSTRAINT places_coordinates_paired
        CHECK ((latitude IS NULL) = (longitude IS NULL)),
    CONSTRAINT places_wikidata_qid_format
        CHECK (wikidata_qid IS NULL OR wikidata_qid ~ '^Q[1-9][0-9]*$')
);

CREATE UNIQUE INDEX places_normalized_name_key ON places (normalized_name);
CREATE INDEX places_geom_gist ON places USING gist (geom);
CREATE INDEX places_canonical_name_trgm ON places USING gin (canonical_name gin_trgm_ops);
CREATE INDEX places_wikidata_qid_idx ON places (wikidata_qid) WHERE wikidata_qid IS NOT NULL;

-- Keep the PostGIS point in sync with the plain lat/lon columns so callers only
-- ever have to write latitude/longitude.
CREATE OR REPLACE FUNCTION places_sync_geom() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.latitude IS NOT NULL AND NEW.longitude IS NOT NULL THEN
        NEW.geom := ST_SetSRID(ST_MakePoint(NEW.longitude, NEW.latitude), 4326);
    ELSE
        NEW.geom := NULL;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER places_sync_geom_trg
    BEFORE INSERT OR UPDATE OF latitude, longitude ON places
    FOR EACH ROW EXECUTE FUNCTION places_sync_geom();

CREATE TRIGGER places_set_updated_at
    BEFORE UPDATE ON places
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ---------------------------------------------------------------------------
-- place_names: historical / variant names for a place.
-- Populated minimally by the importer (one attested name per source); designed
-- to hold researcher-curated name history with validity ranges later.
-- ---------------------------------------------------------------------------
CREATE TABLE place_names (
    id               BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    place_id         BIGINT NOT NULL REFERENCES places (id) ON DELETE CASCADE,
    name             TEXT NOT NULL,
    normalized_name  TEXT NOT NULL,
    -- year bounds are expressed in `calendar`; NULL means open-ended / unknown.
    valid_from       INTEGER,
    valid_to         INTEGER,
    calendar         calendar_system,
    language         TEXT,
    source_reference TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT place_names_year_order
        CHECK (valid_from IS NULL OR valid_to IS NULL OR valid_from <= valid_to)
);

CREATE INDEX place_names_place_id_idx ON place_names (place_id);
CREATE INDEX place_names_normalized_name_idx ON place_names (normalized_name);
CREATE INDEX place_names_normalized_name_trgm ON place_names USING gin (name gin_trgm_ops);

-- Idempotency guard for the importer's minimal "attested name" rows.
CREATE UNIQUE INDEX place_names_attested_key
    ON place_names (place_id, normalized_name, calendar)
    WHERE valid_from IS NULL AND valid_to IS NULL;

-- ---------------------------------------------------------------------------
-- appointments: one historical event record per spreadsheet row.
-- ---------------------------------------------------------------------------
CREATE TABLE appointments (
    id                   BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    source_record_id     BIGINT NOT NULL REFERENCES source_records (id) ON DELETE CASCADE,

    -- Resolved (provisional) links.
    old_person_id        BIGINT REFERENCES persons (id),
    new_person_id        BIGINT REFERENCES persons (id),
    origin_place_id      BIGINT REFERENCES places (id),
    destination_place_id BIGINT REFERENCES places (id),

    -- Raw evidence, never overwritten by resolution.
    raw_old_kadi         TEXT,
    raw_new_kadi         TEXT,
    raw_old_place        TEXT,
    raw_new_place        TEXT,

    -- Date handling: keep the original string, never guess the calendar.
    year_original        TEXT,
    -- Best-effort integer pulled from `year_original` (first 3-4 digit run) for
    -- range filtering ONLY. It is calendar-agnostic and NOT an authoritative
    -- Gregorian year. See architecture.md.
    year_numeric         INTEGER,
    calendar             calendar_system,

    degree               TEXT,
    position_type        TEXT,
    period               TEXT,
    salary               TEXT,
    old_salary           TEXT,
    asitane              TEXT,
    infisal              TEXT,
    region_raw           TEXT,

    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One appointment event per source row -> re-import upserts instead of duplicating.
CREATE UNIQUE INDEX appointments_source_record_id_key ON appointments (source_record_id);

CREATE INDEX appointments_old_person_idx      ON appointments (old_person_id)        WHERE old_person_id IS NOT NULL;
CREATE INDEX appointments_new_person_idx      ON appointments (new_person_id)        WHERE new_person_id IS NOT NULL;
CREATE INDEX appointments_origin_place_idx    ON appointments (origin_place_id)      WHERE origin_place_id IS NOT NULL;
CREATE INDEX appointments_dest_place_idx      ON appointments (destination_place_id) WHERE destination_place_id IS NOT NULL;
CREATE INDEX appointments_year_numeric_idx    ON appointments (year_numeric)         WHERE year_numeric IS NOT NULL;
CREATE INDEX appointments_flow_idx           ON appointments (origin_place_id, destination_place_id)
    WHERE origin_place_id IS NOT NULL AND destination_place_id IS NOT NULL;

CREATE TRIGGER appointments_set_updated_at
    BEFORE UPDATE ON appointments
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
