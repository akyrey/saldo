-- Event consumers and webhooks (ARCHITECTURE.md 5.5), copied exactly. Never edit once merged.

CREATE TABLE consumer_cursors (
    consumer    TEXT PRIMARY KEY,                 -- 'webhooks', 'notifications'
    last_seq    INTEGER NOT NULL DEFAULT 0,
    updated_at  TEXT NOT NULL
) STRICT;

-- The secret is stored in clear text because it is needed to sign each delivery.
CREATE TABLE webhook_endpoints (
    id                     TEXT PRIMARY KEY,
    household_id           TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    name                   TEXT NOT NULL,
    url                    TEXT NOT NULL,
    secret                 TEXT NOT NULL,
    event_types            TEXT NOT NULL DEFAULT '["*"]' CHECK (json_valid(event_types)),
    include_import_events  INTEGER NOT NULL DEFAULT 0 CHECK (include_import_events IN (0, 1)),
    active                 INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    consecutive_failures   INTEGER NOT NULL DEFAULT 0,
    disabled_reason        TEXT,
    created_at             TEXT NOT NULL,
    updated_at             TEXT NOT NULL
) STRICT;

CREATE TABLE webhook_deliveries (
    id                TEXT PRIMARY KEY,
    endpoint_id       TEXT NOT NULL REFERENCES webhook_endpoints (id) ON DELETE CASCADE,
    event_seq         INTEGER NOT NULL REFERENCES events (seq),
    status            TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'succeeded', 'dead')),
    attempts          INTEGER NOT NULL DEFAULT 0,
    next_attempt_at   TEXT,
    last_attempt_at   TEXT,
    last_status_code  INTEGER,
    last_error        TEXT,
    created_at        TEXT NOT NULL,
    UNIQUE (endpoint_id, event_seq)
) STRICT;
CREATE INDEX webhook_deliveries_due ON webhook_deliveries (status, next_attempt_at);
