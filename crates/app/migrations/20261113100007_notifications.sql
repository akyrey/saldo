-- Notifications (ARCHITECTURE.md 5.5), copied exactly. Never edit once merged.

CREATE TABLE notification_channels (
    id            TEXT PRIMARY KEY,
    household_id  TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    user_id       TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind          TEXT NOT NULL CHECK (kind IN ('ntfy', 'gotify', 'email', 'webpush')),
    name          TEXT NOT NULL,
    config        TEXT NOT NULL CHECK (json_valid(config)),
    active        INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
) STRICT;

-- dedupe_key (e.g. 'bill.overdue:<period id>:2026-11-14') makes sending idempotent.
CREATE TABLE notifications (
    id           TEXT PRIMARY KEY,
    household_id TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    channel_id   TEXT NOT NULL REFERENCES notification_channels (id) ON DELETE CASCADE,
    event_seq    INTEGER NOT NULL REFERENCES events (seq),
    dedupe_key   TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'sent', 'failed')),
    attempts     INTEGER NOT NULL DEFAULT 0,
    last_error   TEXT,
    created_at   TEXT NOT NULL,
    sent_at      TEXT,
    UNIQUE (channel_id, dedupe_key)
) STRICT;
