-- Persisted Claude reviews. Every successful call to POST /api/cvs/{id}/reviews
-- inserts a row here so we can show "latest review" on a CV's detail page and
-- (later) build aggregates for the Overview page.
--
-- `payload` holds the full review object (overall_score / verdict / language
-- / report_markdown / improved_yaml) — the dedicated columns at the top are
-- denormalised copies for cheap filtering / aggregation. `yaml_snapshot`
-- captures what was actually reviewed: a follow-up edit on the CV won't
-- silently change the past review's "what we critiqued" record.
--
-- `user_id` is also denormalised (we can derive it from cv_id → cvs.user_id),
-- but keeping it in the row lets us index "all reviews this user ran" without
-- a join, and makes ON DELETE CASCADE behave the way we want regardless of
-- which side gets removed first.

CREATE TABLE IF NOT EXISTS reviews (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    cv_id UUID NOT NULL REFERENCES cvs(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    overall_score SMALLINT NOT NULL,
    verdict TEXT NOT NULL,
    language TEXT NOT NULL,
    payload JSONB NOT NULL,
    yaml_snapshot TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS reviews_cv_id_created_at_idx
    ON reviews (cv_id, created_at DESC);

CREATE INDEX IF NOT EXISTS reviews_user_id_created_at_idx
    ON reviews (user_id, created_at DESC);
