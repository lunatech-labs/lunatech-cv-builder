-- Introduces user ownership of CVs. The user `id` is the Keycloak `sub`
-- claim (UUID); for unauthenticated dev mode we use the nil UUID. The dev
-- user is always present so the dev path works whether or not anyone has
-- ever logged in via Keycloak.

CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY,
    email TEXT,
    name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO users (id, email, name)
VALUES ('00000000-0000-0000-0000-000000000000', 'dev@local', 'Dev User')
ON CONFLICT (id) DO NOTHING;

ALTER TABLE cvs
    ADD COLUMN IF NOT EXISTS user_id UUID REFERENCES users(id) ON DELETE CASCADE;

-- Backfill existing rows to the dev user so they remain visible in dev mode
-- (and harmlessly invisible to authenticated users in prod).
UPDATE cvs SET user_id = '00000000-0000-0000-0000-000000000000' WHERE user_id IS NULL;

ALTER TABLE cvs ALTER COLUMN user_id SET NOT NULL;

CREATE INDEX IF NOT EXISTS cvs_user_id_updated_at_idx ON cvs (user_id, updated_at DESC);
