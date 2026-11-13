# Firefly III fixtures

Invented data in the exact shape of Firefly III 6.7.9 API responses, as read
from the owner's instance (structure only; every name, amount and ID here is
made up). The transaction fixtures (`transactions-*.json`) define journal IDs
101 to 113 and accounts 1 to 12; the planning fixtures refer to them.

Notes from the live instance:

- `/v1/subscriptions` returns `paid_dates` only when `start` and `end` are
  passed (the fixture shows that form); each paid date names its
  `transaction_journal_id`.
- Recurrence repetitions use `weekend` 1 to 4 (keep, skip, previous Friday,
  next Monday) and `type` `daily`, `weekly`, `monthly`, `ndom`, `yearly`.
- Rules on the owner's instance all use `trigger: store-journal` and
  `strict: true`. Trigger keywords in use: `currency_is`, `amount_more`,
  `amount_less`, `description_contains`, `description_is`,
  `description_starts`, `category_is`, `source_account_is`,
  `destination_account_is`, `has_no_tag`, `transaction_type`. Action
  keywords: `link_to_bill`, `set_category`, `add_tag`. Values refer to
  objects by name (bill, category, tag, account).
- Budget limits come from `/v1/budgets/{id}/limits`; `start` and `end` are
  local date-times (`end` is 23:59:59 of the last day).
- The owner has no piggy banks; `piggy-banks.json` is empty, so the piggy bank
  importer must be written from the API specification.

Piggy bank fixtures written from the public API specification (6.7.8):
`piggy-banks-spec.json` (three piggy banks: one with a target on account 2,
one whose target is `"0"`, meaning none, and one on an account that does not
exist) and `piggy-bank-events-1.json` (the events of piggy bank 1: one tied to
the transfer journal 108, two without a journal). The spec's events carry no
account; `transaction_journal_id` names the transfer.
