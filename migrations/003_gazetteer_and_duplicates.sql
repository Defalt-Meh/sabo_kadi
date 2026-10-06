-- Kadı Atlas — gazetteer, coordinate provenance and duplicate transcriptions.
--
--  * Coordinates arrive in a separate gazetteer sheet (one row per place name)
--    whose values are NOT trusted blindly. Every delivered row is kept in
--    `gazetteer_entries` together with the importer's verdict; only `accepted`
--    coordinates are copied onto `places`.
--  * The same register entry is sometimes transcribed in two documents. Such
--    rows stay in `appointment_records` (raw evidence is never deleted) but are
--    marked `duplicate_of`, and the `appointments` view the API reads from hides
--    them so counts and flows are not inflated.

-- ---------------------------------------------------------------------------
-- 1. Where a place's coordinates came from.
--    source_row : latitude/longitude columns on an appointment row
--    gazetteer  : an accepted `gazetteer_entries` row
-- ---------------------------------------------------------------------------
ALTER TABLE places ADD COLUMN coordinate_source TEXT
    CONSTRAINT places_coordinate_source_allowed
        CHECK (coordinate_source IN ('source_row', 'gazetteer'));

UPDATE places SET coordinate_source = 'source_row' WHERE latitude IS NOT NULL;

ALTER TABLE places ADD CONSTRAINT places_coordinate_source_paired
    CHECK ((coordinate_source IS NULL) = (latitude IS NULL));

-- ---------------------------------------------------------------------------
-- 2. gazetteer_entries: the delivered place → coordinate table, kept verbatim
--    with a validation verdict. Linked to `places` by `normalized_name`.
-- ---------------------------------------------------------------------------
CREATE TABLE gazetteer_entries (
    id                BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    normalized_name   TEXT NOT NULL,
    original_name     TEXT NOT NULL,
    matched_name      TEXT,
    -- as delivered; only format-valid ids are ever copied onto `places`
    wikidata_qid      TEXT,
    wikipedia_url     TEXT,
    country           TEXT,
    raw_latitude      TEXT,
    raw_longitude     TEXT,
    -- parsed values (NULL when unparseable); may be set even when rejected
    latitude          DOUBLE PRECISION,
    longitude         DOUBLE PRECISION,
    -- accepted     : copied onto the matching place
    -- needs_review : plausible but too coarse / not a settlement; not used
    -- rejected     : unusable (unparseable, outside the study region, ...)
    coordinate_status TEXT NOT NULL
        CHECK (coordinate_status IN ('accepted', 'needs_review', 'rejected')),
    issues            TEXT[] NOT NULL DEFAULT '{}',
    source_file       TEXT NOT NULL,
    source_sheet      TEXT NOT NULL,
    source_row        INTEGER NOT NULL,
    raw               JSONB NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX gazetteer_entries_normalized_name_key ON gazetteer_entries (normalized_name);
CREATE INDEX gazetteer_entries_status_idx ON gazetteer_entries (coordinate_status);

CREATE TRIGGER gazetteer_entries_set_updated_at
    BEFORE UPDATE ON gazetteer_entries
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ---------------------------------------------------------------------------
-- 3. Duplicate transcriptions.
-- ---------------------------------------------------------------------------
-- md5 of the normalized source text; equal fingerprints = same register entry.
ALTER TABLE source_records ADD COLUMN text_fingerprint TEXT;
CREATE INDEX source_records_text_fingerprint_idx
    ON source_records (text_fingerprint) WHERE text_fingerprint IS NOT NULL;

ALTER TABLE appointments RENAME TO appointment_records;
ALTER TABLE appointment_records
    ADD COLUMN duplicate_of BIGINT REFERENCES appointment_records (id) ON DELETE SET NULL;
CREATE INDEX appointment_records_duplicate_of_idx
    ON appointment_records (duplicate_of) WHERE duplicate_of IS NOT NULL;

-- Everything that reads appointments for counting, flows and journeys goes
-- through this view. The importer writes to `appointment_records`.
CREATE VIEW appointments AS
    SELECT * FROM appointment_records WHERE duplicate_of IS NULL;
