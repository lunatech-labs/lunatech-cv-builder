-- Reverts 0002: review persistence is now session-only (in browser memory),
-- so the latest_review / latest_review_at columns added in 0002 are unused.
ALTER TABLE cvs
    DROP COLUMN IF EXISTS latest_review,
    DROP COLUMN IF EXISTS latest_review_at;
