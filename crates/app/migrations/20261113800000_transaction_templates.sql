-- Phase 8: saved, partial transactions the editor fills itself from
-- (ARCHITECTURE.md 5.4). `splits` is a JSON array of partial splits.
CREATE TABLE transaction_templates (
    id            TEXT PRIMARY KEY,
    household_id  TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    kind          TEXT NOT NULL CHECK (kind IN ('withdrawal', 'deposit', 'transfer')),
    title         TEXT,
    splits        TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(splits)),
    sort_order    INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    UNIQUE (household_id, name)
) STRICT;
