-- Phase 9: reconciling an account against a bank statement. The account's
-- "reconciled up to" date is its latest reconciliation's statement_date.
CREATE TABLE reconciliations (
    id                       TEXT PRIMARY KEY,
    household_id             TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    account_id               TEXT NOT NULL,
    statement_date           TEXT NOT NULL,
    statement_balance_minor  INTEGER NOT NULL,
    currency_code            TEXT NOT NULL REFERENCES currencies (code),
    -- Statement balance minus the cleared amounts; 0 when they agreed.
    difference_minor         INTEGER NOT NULL DEFAULT 0,
    -- The reconciliation transaction booking a difference, if there was one.
    difference_group_id      TEXT REFERENCES transaction_groups (id) ON DELETE SET NULL,
    created_by_user_id       TEXT REFERENCES users (id) ON DELETE SET NULL,
    created_at               TEXT NOT NULL,
    UNIQUE (household_id, id),
    FOREIGN KEY (household_id, account_id) REFERENCES accounts (household_id, id) ON DELETE CASCADE
) STRICT;
CREATE INDEX reconciliations_account ON reconciliations (account_id, statement_date);

-- The splits a reconciliation confirmed (set reconciled = 1 on).
CREATE TABLE reconciliation_splits (
    reconciliation_id  TEXT NOT NULL REFERENCES reconciliations (id) ON DELETE CASCADE,
    split_id           TEXT NOT NULL REFERENCES splits (id) ON DELETE CASCADE,
    PRIMARY KEY (reconciliation_id, split_id)
) STRICT;
CREATE INDEX reconciliation_splits_split ON reconciliation_splits (split_id);
