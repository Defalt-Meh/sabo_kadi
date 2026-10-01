-- Kadı Atlas — full-text search support for source texts.
--
-- We index with the 'simple' configuration (no language-specific stemming).
-- The corpus mixes Ottoman Turkish transliteration, Arabic-script-derived
-- spellings and modern Turkish; 'simple' gives predictable, reversible tokens
-- and avoids wrong stems. Historians can revisit this choice later.

ALTER TABLE source_records
    ADD COLUMN source_text_tsv tsvector
    GENERATED ALWAYS AS (to_tsvector('simple', coalesce(source_text, ''))) STORED;

CREATE INDEX source_records_tsv_gin ON source_records USING gin (source_text_tsv);

-- Trigram index for fuzzy substring matching on the raw source text
-- (used as a fallback / "contains" search).
CREATE INDEX source_records_source_text_trgm
    ON source_records USING gin (source_text gin_trgm_ops);

-- Helpful for the /sources listing and doc_id lookups that are not the unique key.
CREATE INDEX source_records_varak_no_idx ON source_records (varak_no) WHERE varak_no IS NOT NULL;
