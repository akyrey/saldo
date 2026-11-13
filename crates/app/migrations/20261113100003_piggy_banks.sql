-- Piggy banks (ARCHITECTURE.md 5.3), copied exactly. Never edit once merged.

CREATE TABLE piggy_banks (
    id                   TEXT PRIMARY KEY,
    household_id         TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    name                 TEXT NOT NULL,
    target_amount_minor  INTEGER CHECK (target_amount_minor > 0),
    currency_code        TEXT NOT NULL REFERENCES currencies (code),
    start_date           TEXT,
    target_date          TEXT,
    active               INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    sort_order           INTEGER NOT NULL DEFAULT 0,
    notes                TEXT,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    UNIQUE (household_id, id),
    UNIQUE (household_id, name)
) STRICT;

CREATE TABLE piggy_bank_accounts (
    household_id   TEXT NOT NULL,
    piggy_bank_id  TEXT NOT NULL,
    account_id     TEXT NOT NULL,
    PRIMARY KEY (piggy_bank_id, account_id),
    FOREIGN KEY (household_id, piggy_bank_id) REFERENCES piggy_banks (household_id, id) ON DELETE CASCADE,
    FOREIGN KEY (household_id, account_id) REFERENCES accounts (household_id, id)
) STRICT;

-- The saved amount of a piggy bank is the sum of its events (positive = added).
CREATE TABLE piggy_bank_events (
    id             TEXT PRIMARY KEY,
    household_id   TEXT NOT NULL,
    piggy_bank_id  TEXT NOT NULL,
    account_id     TEXT NOT NULL,
    amount_minor   INTEGER NOT NULL CHECK (amount_minor <> 0),
    split_id       TEXT REFERENCES splits (id) ON DELETE SET NULL,
    date           TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    FOREIGN KEY (household_id, piggy_bank_id) REFERENCES piggy_banks (household_id, id) ON DELETE CASCADE,
    FOREIGN KEY (household_id, account_id) REFERENCES accounts (household_id, id)
) STRICT;
CREATE INDEX piggy_bank_events_piggy ON piggy_bank_events (piggy_bank_id, date);
