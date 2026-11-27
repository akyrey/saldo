-- Phase 13 Track B: importing bank and Satispay statements.

-- How to read one bank's export, saved per household.
CREATE TABLE statement_profiles (
    id             TEXT PRIMARY KEY,
    household_id   TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    name           TEXT NOT NULL,
    -- The account a file is imported into unless the upload names another.
    account_id     TEXT REFERENCES accounts (id) ON DELETE SET NULL,
    format         TEXT NOT NULL CHECK (format IN ('csv', 'xlsx')),
    -- saldo_app::statements::parse::Mapping as JSON.
    mapping        TEXT NOT NULL CHECK (json_valid(mapping)),
    -- Part column header -> asset account ID, or null to keep it as a note
    -- (Satispay's meal vouchers; brief decision 13).
    part_accounts  TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(part_accounts)),
    -- [{ "pattern": text, "account_ids": [...] }]: a line whose description
    -- contains the pattern is a transfer with one of those own accounts.
    transfer_rules TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(transfer_rules)),
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    UNIQUE (household_id, name)
) STRICT;

-- One uploaded file: a draft to review, then committed, discarded or
-- reverted.
CREATE TABLE statement_imports (
    id                     TEXT PRIMARY KEY,
    household_id           TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    account_id             TEXT NOT NULL,
    profile_id             TEXT REFERENCES statement_profiles (id) ON DELETE SET NULL,
    filename               TEXT NOT NULL,
    sha256                 TEXT NOT NULL,
    status                 TEXT NOT NULL CHECK (status IN ('draft', 'committed', 'discarded', 'reverted')),
    currency_code          TEXT NOT NULL REFERENCES currencies (code),
    opening_balance_minor  INTEGER,
    closing_balance_minor  INTEGER,
    -- The last booked date of the file, for "Reconcile now".
    closing_date           TEXT,
    -- opening + lines - closing; NULL without both balances.
    gap_minor              INTEGER,
    -- Counts, unsupported rows with their reasons, and the findings about
    -- existing transactions.
    summary                TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(summary)),
    created_by_user_id     TEXT REFERENCES users (id) ON DELETE SET NULL,
    created_at             TEXT NOT NULL,
    committed_at           TEXT,
    UNIQUE (household_id, id),
    FOREIGN KEY (household_id, account_id) REFERENCES accounts (household_id, id) ON DELETE CASCADE
) STRICT;
CREATE INDEX statement_imports_account ON statement_imports (account_id, created_at);

-- The importable lines of a file. Rows that are not transactions are kept
-- in the import's summary with their reasons.
CREATE TABLE statement_lines (
    import_id                TEXT NOT NULL REFERENCES statement_imports (id) ON DELETE CASCADE,
    position                 INTEGER NOT NULL,
    row_number               INTEGER NOT NULL,
    accounting_date          TEXT NOT NULL,
    value_date               TEXT NOT NULL,
    booked_date              TEXT NOT NULL,
    -- Signed for the account; zero only when parts paid it all.
    amount_minor             INTEGER NOT NULL,
    currency_code            TEXT NOT NULL REFERENCES currencies (code),
    description              TEXT NOT NULL,
    category                 TEXT,
    counterparty_name        TEXT,
    counterparty_iban        TEXT,
    external_id              TEXT,
    -- The key in external_refs (source 'bank-statement', entity 'split'):
    -- account ID + ':' + external_id, or the fingerprint without one.
    ref_key                  TEXT NOT NULL,
    notes                    TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(notes)),
    -- [{ "column": text, "amount_minor": int }] (brief decision 13).
    parts                    TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(parts)),
    status                   TEXT NOT NULL CHECK (status IN ('new', 'duplicate', 'possible_match', 'skipped_before', 'not_matched_reconciled')),
    action                   TEXT NOT NULL CHECK (action IN ('create', 'link', 'skip')),
    reason                   TEXT,
    -- The matched existing split (link), or the first created one (create).
    split_id                 TEXT,
    -- The group a create made, so a revert can delete it.
    created_group_id         TEXT,
    counterparty_account_id  TEXT,
    category_id              TEXT,
    PRIMARY KEY (import_id, position)
) STRICT;
CREATE INDEX statement_lines_ref ON statement_lines (ref_key);
