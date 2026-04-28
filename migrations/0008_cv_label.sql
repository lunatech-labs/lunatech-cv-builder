-- Optional CV label — typically the client a tailored version is being
-- prepared for ("Disney", "BNP Paribas", etc.). Extracted from the YAML's
-- `label:` field on every save; NULL when the field is absent.

ALTER TABLE cvs
    ADD COLUMN IF NOT EXISTS label TEXT;
