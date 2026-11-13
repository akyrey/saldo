# Phase 8: transaction templates and duplicating transactions

Requested by the owner on 2026-11-13. Before starting, confirm the design choices marked **(confirm)** with the owner, then add the table and endpoints below to `docs/ARCHITECTURE.md` (sections 5 and 6), since that file is the contract.

## 1. Goal

Entering a frequent transaction takes a few clicks. The owner can:

- **Save a template** of a withdrawal, deposit or transfer: any part may be left empty (the amount, the date, the description, an account...).
- **Use a template**: the transaction editor opens filled in from it; the owner fills in what is missing, changes anything else, and saves a normal transaction.
- **Duplicate a transaction**: the same editor opens filled in from an existing transaction, with today's date, without saving a template.
- **Save as template** from an existing transaction.

## 2. Read first

`CLAUDE.md`, `docs/STATUS.md`, `docs/ARCHITECTURE.md` sections 4.2 (groups and splits, kind derivation), 5.2 and 6 (transactions), 8.1 (events), 9.2 (export); the transaction editor in `web/src/features/transactions/`; the recurrence template code from Phase 5 (it already stores a transaction to create later).

## 3. Prerequisites

Phases 4 and 5 are done (the editor, budgets and bills exist).

## 4. Design

- **Table** `transaction_templates`: `id`, `household_id`, `name` (unique per household), `kind` (`withdrawal`, `deposit`, `transfer`; tells the editor which accounts to offer when the template has none), `title`, `splits` (JSON array of partial splits: `description`, `source_id`, `destination_id`, `amount`, `currency`, `foreign_amount`, `foreign_currency`, `category_id`, `budget_id`, `bill_id`, `tag_ids`, `notes`; every field optional), `sort_order`, `created_at`, `updated_at`. No `date`: using a template defaults to today.
- **Validation**: a template is checked only for what it contains: referenced accounts, categories, budgets, bills and tags must belong to the household, and amounts must parse in their currency. The full ledger rules run when the transaction is saved, as for any transaction.
- **Deleted references**: a template whose account (or category, ...) is deleted keeps working; the missing field is dropped when it is loaded and the editor shows it empty. **(confirm)** The alternative is to block the delete.
- **Server API**: CRUD on `/transaction-templates`; `POST /transactions/{id}/template` (body: `name`) saves an existing group as a template, without its date and reconciled flag. There is no "instantiate" endpoint: the web editor fills itself from the template and posts a normal `POST /transactions` (`Origin::Api`, so rules run as usual).
- **Duplicate** is web only: the editor loads the group and opens a new, unsaved copy with today's date and `reconciled` cleared. **(confirm)** Whether attachments should be copied (proposal: no).
- **Events**: `template.created`, `template.updated`, `template.deleted`; add `template` to the live-events keys in the web app.
- **Export**: templates added to the JSON document and a `templates.csv`; format version stays 1.
- **Importer**: Firefly III has no templates; nothing to import.
- **Recurrences**: keep their own stored transaction; if the partial-split type fits both, share it, otherwise do not force it.

## 5. Work packages

1. Migration, `saldo-app` `TemplateService` (CRUD, `from_transaction`), with tests first.
2. Server routes with utoipa docs; regenerate `web/openapi.json` and the TS schema.
3. Web: a templates page (list, rename, delete, reorder); "New from template" in the transactions page and the editor; "Duplicate" and "Save as template" on a transaction; the editor accepts a partial prefill and keeps the split rows' behaviour.
4. Export update, live-events keys, a Playwright flow (save a transaction as a template, use it with a new amount, duplicate a transaction).

## 6. Acceptance criteria

The standard checks from `CLAUDE.md` (Rust and web), coverage at least 80%, and the Playwright flow above.

## 7. Hand-off

In `docs/STATUS.md`: Phase 8 done with commit hash and the answers to the **(confirm)** items.
