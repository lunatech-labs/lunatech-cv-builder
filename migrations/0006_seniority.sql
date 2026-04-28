-- Cached seniority score for each CV. Computed at create/update from the
-- YAML and re-computable on demand from the same source — these columns are
-- the latest snapshot for fast filtering and display, the canonical truth
-- still lives in the YAML itself.
--
-- `seniority` carries the full per-dimension breakdown (years / leadership /
-- scope / external_signals / title_bonus); the `_score` and `_level` are
-- pulled out for cheap COUNT / ORDER BY in dashboard queries.

ALTER TABLE cvs
    ADD COLUMN IF NOT EXISTS seniority JSONB,
    ADD COLUMN IF NOT EXISTS seniority_score SMALLINT,
    ADD COLUMN IF NOT EXISTS seniority_level TEXT;

CREATE INDEX IF NOT EXISTS cvs_seniority_score_idx
    ON cvs (seniority_score DESC NULLS LAST);
