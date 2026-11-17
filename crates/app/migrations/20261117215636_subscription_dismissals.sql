-- Phase 9: subscription suggestions the household said no to. Suggestions
-- themselves are computed on each request; only the "no" is stored.
CREATE TABLE subscription_dismissals (
    household_id   TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    account_id     TEXT NOT NULL,
    currency_code  TEXT NOT NULL REFERENCES currencies (code),
    created_at     TEXT NOT NULL,
    PRIMARY KEY (household_id, account_id, currency_code),
    FOREIGN KEY (household_id, account_id) REFERENCES accounts (household_id, id) ON DELETE CASCADE
) STRICT;
