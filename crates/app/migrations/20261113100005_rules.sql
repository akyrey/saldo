-- Rules (ARCHITECTURE.md 5.4), copied exactly. Never edit once merged.

CREATE TABLE rule_groups (
    id            TEXT PRIMARY KEY,
    household_id  TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    title         TEXT NOT NULL,
    description   TEXT,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    active        INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    UNIQUE (household_id, id)
) STRICT;

CREATE TABLE rules (
    id               TEXT PRIMARY KEY,
    household_id     TEXT NOT NULL,
    rule_group_id    TEXT NOT NULL,
    title            TEXT NOT NULL,
    description      TEXT,
    trigger_on       TEXT NOT NULL CHECK (trigger_on IN ('create', 'update', 'manual')),
    strict           INTEGER NOT NULL DEFAULT 1 CHECK (strict IN (0, 1)),          -- 1 = all triggers must match
    stop_processing  INTEGER NOT NULL DEFAULT 0 CHECK (stop_processing IN (0, 1)),
    active           INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    sort_order       INTEGER NOT NULL DEFAULT 0,
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    FOREIGN KEY (household_id, rule_group_id) REFERENCES rule_groups (household_id, id) ON DELETE CASCADE
) STRICT;

-- kind/value are the serialised form of the core Trigger and Action enums.
CREATE TABLE rule_triggers (
    id        TEXT PRIMARY KEY,
    rule_id   TEXT NOT NULL REFERENCES rules (id) ON DELETE CASCADE,
    position  INTEGER NOT NULL,
    kind      TEXT NOT NULL,
    value     TEXT,
    negate    INTEGER NOT NULL DEFAULT 0 CHECK (negate IN (0, 1)),
    UNIQUE (rule_id, position)
) STRICT;

CREATE TABLE rule_actions (
    id               TEXT PRIMARY KEY,
    rule_id          TEXT NOT NULL REFERENCES rules (id) ON DELETE CASCADE,
    position         INTEGER NOT NULL,
    kind             TEXT NOT NULL,
    value            TEXT,
    stop_processing  INTEGER NOT NULL DEFAULT 0 CHECK (stop_processing IN (0, 1)),
    active           INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    UNIQUE (rule_id, position)
) STRICT;
