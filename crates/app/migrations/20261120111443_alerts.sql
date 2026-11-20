-- Phase 10: more notifications.

-- Which event types a channel receives. Existing channels keep exactly what
-- they had: bill reminders.
ALTER TABLE notification_channels
    ADD COLUMN event_types TEXT NOT NULL DEFAULT '["bill.due_soon","bill.overdue"]'
    CHECK (json_valid(event_types));

-- Splits at or above this amount (primary currency, minor units) append
-- `transaction.large`. NULL: off.
ALTER TABLE households
    ADD COLUMN large_transaction_minor INTEGER CHECK (large_transaction_minor > 0);

-- One row per alert condition that currently holds (or, for the weekly
-- digest, per week sent). Inserted together with the alert's event and
-- deleted when the condition stops, so a later crossing alerts again.
-- Bookkeeping: changes append no events.
CREATE TABLE alert_states (
    household_id  TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    key           TEXT NOT NULL,   -- e.g. 'budget:<limit id>:80'
    since         TEXT NOT NULL,
    PRIMARY KEY (household_id, key)
) STRICT;
