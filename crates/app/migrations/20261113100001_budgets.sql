-- Budgets (ARCHITECTURE.md 5.3), copied exactly. Never edit once merged.

CREATE TABLE budgets (
    id                  TEXT PRIMARY KEY,
    household_id        TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    name                TEXT NOT NULL,
    active              INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    sort_order          INTEGER NOT NULL DEFAULT 0,
    notes               TEXT,
    auto_type           TEXT CHECK (auto_type IN ('reset', 'rollover')),
    auto_period         TEXT CHECK (auto_period IN ('daily', 'weekly', 'monthly', 'quarterly', 'half_year', 'yearly')),
    auto_amount_minor   INTEGER CHECK (auto_amount_minor > 0),
    auto_currency_code  TEXT REFERENCES currencies (code),
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    UNIQUE (household_id, id),
    UNIQUE (household_id, name)
) STRICT;

CREATE TABLE budget_limits (
    id             TEXT PRIMARY KEY,
    household_id   TEXT NOT NULL,
    budget_id      TEXT NOT NULL,
    start_date     TEXT NOT NULL,
    end_date       TEXT NOT NULL,
    amount_minor   INTEGER NOT NULL CHECK (amount_minor > 0),
    currency_code  TEXT NOT NULL REFERENCES currencies (code),
    notes          TEXT,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    FOREIGN KEY (household_id, budget_id) REFERENCES budgets (household_id, id) ON DELETE CASCADE,
    CHECK (start_date <= end_date)
) STRICT;
CREATE INDEX budget_limits_budget ON budget_limits (budget_id, start_date);
