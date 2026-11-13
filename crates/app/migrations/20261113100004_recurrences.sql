-- Recurring transactions (ARCHITECTURE.md 5.4), copied exactly. Never edit once merged.

CREATE TABLE recurrences (
    id               TEXT PRIMARY KEY,
    household_id     TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    title            TEXT NOT NULL,
    description      TEXT,
    first_date       TEXT NOT NULL,
    repeat_until     TEXT,
    max_occurrences  INTEGER CHECK (max_occurrences > 0),
    apply_rules      INTEGER NOT NULL DEFAULT 1 CHECK (apply_rules IN (0, 1)),
    active           INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    notes            TEXT,
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    UNIQUE (household_id, id)
) STRICT;

-- moment: '' for daily; weekday 1-7 (Mon-Sun) for weekly; day 1-31 for monthly;
-- 'week,weekday' (e.g. '2,3' = second Wednesday) for nth_weekday; 'MM-DD' for yearly.
CREATE TABLE recurrence_repetitions (
    id              TEXT PRIMARY KEY,
    household_id    TEXT NOT NULL,
    recurrence_id   TEXT NOT NULL,
    kind            TEXT NOT NULL CHECK (kind IN ('daily', 'weekly', 'monthly', 'nth_weekday', 'yearly')),
    moment          TEXT NOT NULL DEFAULT '',
    skip            INTEGER NOT NULL DEFAULT 0 CHECK (skip >= 0),
    weekend_policy  TEXT NOT NULL DEFAULT 'keep' CHECK (weekend_policy IN ('keep', 'skip', 'previous_friday', 'next_monday')),
    FOREIGN KEY (household_id, recurrence_id) REFERENCES recurrences (household_id, id) ON DELETE CASCADE
) STRICT;

-- Template splits; each firing creates one transaction group from them.
CREATE TABLE recurrence_splits (
    id                      TEXT PRIMARY KEY,
    household_id            TEXT NOT NULL,
    recurrence_id           TEXT NOT NULL,
    position                INTEGER NOT NULL,
    description             TEXT NOT NULL,
    source_account_id       TEXT NOT NULL,
    destination_account_id  TEXT NOT NULL,
    amount_minor            INTEGER NOT NULL CHECK (amount_minor > 0),
    currency_code           TEXT NOT NULL REFERENCES currencies (code),
    foreign_amount_minor    INTEGER CHECK (foreign_amount_minor > 0),
    foreign_currency_code   TEXT REFERENCES currencies (code),
    category_id             TEXT REFERENCES categories (id) ON DELETE SET NULL,
    budget_id               TEXT REFERENCES budgets (id) ON DELETE SET NULL,
    bill_id                 TEXT REFERENCES bills (id) ON DELETE SET NULL,
    piggy_bank_id           TEXT REFERENCES piggy_banks (id) ON DELETE SET NULL,
    tags                    TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(tags)),
    UNIQUE (recurrence_id, position),
    FOREIGN KEY (household_id, recurrence_id) REFERENCES recurrences (household_id, id) ON DELETE CASCADE,
    FOREIGN KEY (household_id, source_account_id) REFERENCES accounts (household_id, id),
    FOREIGN KEY (household_id, destination_account_id) REFERENCES accounts (household_id, id)
) STRICT;
