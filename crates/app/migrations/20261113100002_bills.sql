-- Bills and their periods (ARCHITECTURE.md 5.3), copied exactly. Never edit once merged.

CREATE TABLE bills (
    id                  TEXT PRIMARY KEY,
    household_id        TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    name                TEXT NOT NULL,
    amount_min_minor    INTEGER NOT NULL CHECK (amount_min_minor >= 0),
    amount_max_minor    INTEGER NOT NULL,
    currency_code       TEXT NOT NULL REFERENCES currencies (code),
    first_due_date      TEXT NOT NULL,
    end_date            TEXT,
    repeat_freq         TEXT NOT NULL CHECK (repeat_freq IN ('weekly', 'monthly', 'quarterly', 'half_year', 'yearly')),
    skip                INTEGER NOT NULL DEFAULT 0 CHECK (skip >= 0),
    remind_days_before  INTEGER NOT NULL DEFAULT 3 CHECK (remind_days_before >= 0),
    remind_every_days   INTEGER NOT NULL DEFAULT 1 CHECK (remind_every_days >= 1),
    active              INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    sort_order          INTEGER NOT NULL DEFAULT 0,
    notes               TEXT,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    UNIQUE (household_id, id),
    UNIQUE (household_id, name),
    CHECK (amount_min_minor <= amount_max_minor)
) STRICT;

-- One row per due date. The scheduler creates them; reminders are tracked here.
CREATE TABLE bill_periods (
    id                TEXT PRIMARY KEY,
    household_id      TEXT NOT NULL,
    bill_id           TEXT NOT NULL,
    due_date          TEXT NOT NULL,
    status            TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'paid', 'skipped')),
    paid_split_id     TEXT REFERENCES splits (id) ON DELETE SET NULL,
    paid_at           TEXT,
    last_reminded_on  TEXT,
    reminder_count    INTEGER NOT NULL DEFAULT 0,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL,
    UNIQUE (bill_id, due_date),
    FOREIGN KEY (household_id, bill_id) REFERENCES bills (household_id, id) ON DELETE CASCADE
) STRICT;
CREATE INDEX bill_periods_open ON bill_periods (status, due_date);
