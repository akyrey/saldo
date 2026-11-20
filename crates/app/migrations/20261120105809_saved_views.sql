-- Phase 10: named filters for a list page, shared by the household.
-- `filters` is the page's URL filters as a JSON object; the server stores
-- it without interpreting the keys (the web app validates them on load).
CREATE TABLE saved_views (
    id            TEXT PRIMARY KEY,
    household_id  TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    page          TEXT NOT NULL CHECK (page IN ('transactions')),
    filters       TEXT NOT NULL CHECK (json_valid(filters)),
    sort_order    INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    UNIQUE (household_id, page, name)
) STRICT;
