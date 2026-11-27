-- Phase 13 Tracks C and D: Amazon and PayPal exports stored per household,
-- their matches with statement lines and splits, and what a line's matched
-- source proposes.

-- One uploaded file. The file itself is kept at <data dir>/sources/<sha256>
-- so a parser fix can read it again.
CREATE TABLE source_documents (
    id                  TEXT PRIMARY KEY,
    household_id        TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    kind                TEXT NOT NULL CHECK (kind IN ('amazon_orders', 'paypal')),
    filename            TEXT NOT NULL,
    sha256              TEXT NOT NULL,
    first_date          TEXT,
    last_date           TEXT,
    row_count           INTEGER NOT NULL,
    rows_added          INTEGER NOT NULL,
    rows_updated        INTEGER NOT NULL,
    created_by_user_id  TEXT REFERENCES users (id) ON DELETE SET NULL,
    created_at          TEXT NOT NULL,
    UNIQUE (household_id, sha256)
) STRICT;

-- One Amazon item or PayPal row, unique per household by its stable ID, so
-- a newer overlapping export adds only new rows.
CREATE TABLE source_rows (
    id                TEXT PRIMARY KEY,
    household_id      TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    kind              TEXT NOT NULL CHECK (kind IN ('amazon_orders', 'paypal')),
    external_id       TEXT NOT NULL,
    -- The upload that first brought the row, and the latest one holding it.
    document_id       TEXT NOT NULL REFERENCES source_documents (id),
    last_document_id  TEXT NOT NULL REFERENCES source_documents (id),
    date              TEXT NOT NULL,
    time              TEXT,
    -- Signed as the source writes it: an Amazon item's cost is positive;
    -- PayPal writes payments negative and bank fundings positive.
    amount_minor      INTEGER NOT NULL,
    currency_code     TEXT NOT NULL REFERENCES currencies (code),
    -- Amazon: item. PayPal: funding, withdrawal, payment, refund, other.
    row_kind          TEXT NOT NULL,
    -- Amazon: the order ID.
    group_key         TEXT,
    counterparty      TEXT,
    text              TEXT,
    status            TEXT,
    -- Amazon: cancelled or paid by gift card alone (never reaches a bank).
    excluded          INTEGER NOT NULL DEFAULT 0 CHECK (excluded IN (0, 1)),
    raw               TEXT NOT NULL CHECK (json_valid(raw)),
    UNIQUE (household_id, kind, external_id)
) STRICT;
CREATE INDEX source_rows_date ON source_rows (household_id, kind, date);

-- Which source rows explain which statement line or split. A draft's
-- matches point at its line; a commit fills in the split.
CREATE TABLE source_matches (
    household_id         TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    source_row_id        TEXT NOT NULL REFERENCES source_rows (id) ON DELETE CASCADE,
    statement_import_id  TEXT REFERENCES statement_imports (id) ON DELETE CASCADE,
    line_position        INTEGER,
    split_id             TEXT REFERENCES splits (id) ON DELETE CASCADE,
    match_type           TEXT NOT NULL CHECK (match_type IN ('order_id', 'amount_date', 'approximate', 'funding_pair', 'amount_lag')),
    amount_diff_minor    INTEGER NOT NULL DEFAULT 0,
    created_at           TEXT NOT NULL
) STRICT;
CREATE INDEX source_matches_row ON source_matches (source_row_id);
CREATE INDEX source_matches_line ON source_matches (statement_import_id, line_position);
CREATE INDEX source_matches_split ON source_matches (split_id);

-- What a line's matched source proposes (description, counterparty, notes,
-- item amounts), whether the owner keeps it, and whether an Amazon charge
-- is split by item.
ALTER TABLE statement_lines ADD COLUMN proposal TEXT CHECK (proposal IS NULL OR json_valid(proposal));
ALTER TABLE statement_lines ADD COLUMN use_proposal INTEGER NOT NULL DEFAULT 1 CHECK (use_proposal IN (0, 1));
ALTER TABLE statement_lines ADD COLUMN split_by_item INTEGER NOT NULL DEFAULT 0 CHECK (split_by_item IN (0, 1));
