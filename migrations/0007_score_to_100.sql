-- Rescale review `overall_score` from the legacy 1-10 grid to 0-100 for
-- finer-grained ranking. Existing rows get multiplied by 10 so the
-- dashboard and the score badges stay coherent across the upgrade; new
-- reviews land directly in the 0-100 range thanks to the updated SKILL.md.
--
-- Multi-column UPDATE: PostgreSQL evaluates every right-hand side against
-- the row's pre-update state, so both `overall_score = overall_score * 10`
-- and the jsonb_set payload-rewrite see the original value.

UPDATE reviews
SET overall_score = overall_score * 10,
    payload = jsonb_set(payload, '{overall_score}', to_jsonb((overall_score * 10)::int))
WHERE overall_score IS NOT NULL AND overall_score <= 10;
