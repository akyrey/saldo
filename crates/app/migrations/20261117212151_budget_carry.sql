-- Phase 9: what a budget carries from one limit into the next, computed when
-- limits are read (saldo_core::carried). budget_limits.amount_minor is now
-- always the base amount.
ALTER TABLE budgets ADD COLUMN carry TEXT NOT NULL DEFAULT 'none'
    CHECK (carry IN ('none', 'leftover', 'all'));

-- A 'rollover' auto-budget stored its carry inside each limit's amount (and
-- Firefly III did the same). Its limits in the auto currency go back to the
-- base amount; the carry is now computed. The CHECK on auto_type still allows
-- 'rollover', but nothing writes it any more.
UPDATE budget_limits
SET amount_minor = (SELECT b.auto_amount_minor FROM budgets b WHERE b.id = budget_limits.budget_id)
WHERE budget_id IN (SELECT id FROM budgets WHERE auto_type = 'rollover')
  AND currency_code = (SELECT b.auto_currency_code FROM budgets b WHERE b.id = budget_limits.budget_id)
  AND amount_minor > (SELECT b.auto_amount_minor FROM budgets b WHERE b.id = budget_limits.budget_id);

UPDATE budgets SET carry = 'leftover', auto_type = 'reset' WHERE auto_type = 'rollover';
