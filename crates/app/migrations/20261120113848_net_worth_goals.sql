-- Phase 10: net worth goals. The net worth on the day a goal is created is
-- kept as its start, so progress runs from there to the target; a target
-- below the start (paying debt off) works the same way.
CREATE TABLE net_worth_goals (
    id                   TEXT PRIMARY KEY,
    household_id         TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    name                 TEXT NOT NULL,
    owner_user_id        TEXT REFERENCES users (id) ON DELETE CASCADE, -- NULL: the whole household
    target_amount_minor  INTEGER NOT NULL,
    start_amount_minor   INTEGER NOT NULL,
    currency_code        TEXT NOT NULL REFERENCES currencies (code),
    target_date          TEXT,
    reached_on           TEXT,
    active               INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    notes                TEXT,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    UNIQUE (household_id, name)
) STRICT;
