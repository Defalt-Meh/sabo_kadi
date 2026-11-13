-- Kadı Atlas — index clean-up after measuring the real corpus (25.8k rows).
--
--  * `source_records_source_text_trgm` was never used by any query (source
--    search runs on the tsvector) yet was the largest index (~41 MB, a sixth of
--    the database). Dropping it keeps the free hosting tier comfortable as the
--    corpus grows.
--  * Name searches filter on `normalized_name LIKE '%…%'` / `LIKE '…%'`, so the
--    trigram indexes belong on `normalized_name`, not on `canonical_name`.

DROP INDEX IF EXISTS source_records_source_text_trgm;

DROP INDEX IF EXISTS persons_canonical_name_trgm;
DROP INDEX IF EXISTS places_canonical_name_trgm;

CREATE INDEX IF NOT EXISTS persons_normalized_name_trgm
    ON persons USING gin (normalized_name gin_trgm_ops);
CREATE INDEX IF NOT EXISTS places_normalized_name_trgm
    ON places USING gin (normalized_name gin_trgm_ops);
