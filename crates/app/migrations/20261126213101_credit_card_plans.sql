-- Phase 12: credit card plans. A card (an asset account with the credit_card
-- role) gets a monthly budget, the account that pays it and the payment day
-- of the following month; the server keeps one settlement transfer per month.
CREATE TABLE credit_card_plans (
    account_id             TEXT PRIMARY KEY,
    household_id           TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    settlement_account_id  TEXT NOT NULL,
    settlement_day         INTEGER NOT NULL CHECK (settlement_day BETWEEN 1 AND 31),
    starts_cycle           TEXT NOT NULL,     -- 'YYYY-MM': earlier months are never settled
    created_at             TEXT NOT NULL,
    updated_at             TEXT NOT NULL,
    UNIQUE (household_id, account_id),
    FOREIGN KEY (household_id, account_id) REFERENCES accounts (household_id, id) ON DELETE CASCADE,
    FOREIGN KEY (household_id, settlement_account_id) REFERENCES accounts (household_id, id)
) STRICT;
CREATE INDEX credit_card_plans_settlement ON credit_card_plans (settlement_account_id);

-- A month's budget is the row with the latest from_cycle on or before it.
CREATE TABLE credit_card_limits (
    household_id  TEXT NOT NULL,
    account_id    TEXT NOT NULL,
    from_cycle    TEXT NOT NULL,              -- 'YYYY-MM'
    limit_minor   INTEGER NOT NULL CHECK (limit_minor > 0),
    created_at    TEXT NOT NULL,
    PRIMARY KEY (account_id, from_cycle),
    FOREIGN KEY (household_id, account_id) REFERENCES credit_card_plans (household_id, account_id) ON DELETE CASCADE
) STRICT;

-- A split that counts toward the month after its date.
CREATE TABLE credit_card_moves (
    split_id      TEXT PRIMARY KEY REFERENCES splits (id) ON DELETE CASCADE,
    household_id  TEXT NOT NULL,
    account_id    TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    FOREIGN KEY (household_id, account_id) REFERENCES credit_card_plans (household_id, account_id) ON DELETE CASCADE
) STRICT;
CREATE INDEX credit_card_moves_account ON credit_card_moves (account_id);

-- Bookkeeping for the settlement sync: who manages each month's transfer and
-- what the server last wrote. Not exported; changes append no events.
CREATE TABLE credit_card_cycles (
    household_id          TEXT NOT NULL,
    account_id            TEXT NOT NULL,
    cycle                 TEXT NOT NULL,      -- 'YYYY-MM'
    state                 TEXT NOT NULL CHECK (state IN ('synced', 'frozen', 'detached')),
    group_id              TEXT,               -- the settlement transfer; NULL when none
    written_amount_minor  INTEGER,
    written_date          TEXT,
    written_source_id     TEXT,
    PRIMARY KEY (account_id, cycle),
    FOREIGN KEY (household_id, account_id) REFERENCES credit_card_plans (household_id, account_id) ON DELETE CASCADE,
    CHECK ((group_id IS NULL) = (written_amount_minor IS NULL)),
    CHECK ((group_id IS NULL) = (written_date IS NULL)),
    CHECK ((group_id IS NULL) = (written_source_id IS NULL))
) STRICT;
