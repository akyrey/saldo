# Phase 8 hand-off: transaction templates and duplicating transactions

Branch `phase-8-templates`, created from `f7e5a23`. Not pushed, not merged. `docs/STATUS.md` is untouched; fold this file into it when merging.

## Done

- `docs/ARCHITECTURE.md` (first commit): table `transaction_templates` in 5.4, the Templates row in 6, `template.*` in 8.1, `templates.csv` in 9.2.
- Migration `20261113800000_transaction_templates.sql` (as in the brief; `splits` is a JSON array, `UNIQUE (household_id, name)`).
- `saldo-app` `templates` module (`app.templates: TemplateService`): `create`, `update`, `delete`, `get`, `list`, `from_transaction`, each with an `_in` variant where it writes; `all_in` for the export. Events `template.created|updated|deleted` carry `TransactionTemplateView`.
  - The checks cover only what the template contains. Accounts, categories, budgets, bills and tags must belong to the household (filtered by household; same `existing` helpers as transactions). Amounts must parse in their currency: the given one, else the asset/liability side's, else the primary currency.
  - Deleted references: when a template is read, every reference that no longer exists is dropped (one query per entity type for the whole list). Deletes are never blocked.
  - `from_transaction` copies the group without dates, reconciled flags and `meta`. Opening balances and reconciliations get 422 on `kind`.
- Server: `GET/POST /transaction-templates`, `GET/PATCH/DELETE /transaction-templates/{id}`, `POST /transactions/{id}/template` (`{ name }`), all in `routes/templates.rs` under the OpenAPI tag `templates`. `web/openapi.json` and `schema.d.ts` are regenerated.
- Export: `templates` in the JSON document; `templates.csv` has one row per template split (a template with no splits still gets one row) and shows names instead of IDs. The format version stays 1.
- Web:
  - `/templates` page (under "More"): use (opens the editor), rename, move up/down, delete with a confirm step.
  - "New from template…" picker on the transactions page and in the new-transaction editor. It opens `/transactions/new?template=<id>`.
  - On the edit page: "Duplicate" opens `/transactions/new?duplicate=<id>`, a copy dated today and not reconciled, without attachments. "Save as template" asks for a name.
  - Live-events key `template` refreshes `["templates"]`.

## Deviations and additions (need the owner's OK)

1. **Amounts in a template must be positive** (a 0 or negative amount is 422). **Two given accounts must form the template's kind** (422 on `splits[i].destination_id`). **Budgets are allowed on withdrawal templates only.** These are the ledger rules applied to what the template contains, so a used template does not fail on save for something that was already known.
2. **A template may have zero splits.** The editor then opens with one empty row.
3. **`PATCH` also takes `kind`, `title` and `splits`**, not only `name` and `sort_order`; `splits` replaces all of them. When only the kind changes, the kept splits are checked again.
4. **"Save as template" also works in the new-transaction editor.** It saves whatever the form holds, any field may be empty, and no date is stored. This is how to create a template that leaves parts empty, which the brief asks for; otherwise templates could only come from complete transactions. The kind comes from the template being used, else from the first split whose accounts form a kind, else withdrawal.
5. **A template's kind narrows the editor's account pickers.** For example, a withdrawal offers asset/liability accounts as source and expense accounts as destination. A chosen account always stays visible.
6. **`TemplateId` lives in `saldo-app`, not `saldo-core`**, because core never deals with templates.
7. **The schema names are `TransactionTemplateView`, `TransactionTemplateSplitView`, and so on.** The obvious name `TemplateSplitView` was already the recurrence schema, and utoipa silently kept only one of them.
8. **Bug fix in the editor:** `SplitRow` now carries `bill_id` and sends it on save. Before, editing a transaction in the web app silently removed its bill link, because the field was never sent. There is still no bill picker; the value is only kept.
9. **The template's `currency` is not put into the editor**, which has no currency field. When the transaction is saved, the server uses the default currency (the account side's). Foreign amount and foreign currency are carried over.
10. Recurrences keep their own split type. Their template splits require the accounts and the amount, so sharing a type would have forced it.

## Files Phase 7 may also touch

These were changed additively: `crates/app/src/lib.rs` (one field), `crates/server/src/routes/mod.rs` (one `.merge`), `web/src/router.tsx` (a route and `validateSearch` on `/transactions/new`), `nav.ts`, and `live-events.ts`.

In `SplitRowFields.tsx`, only a `kindHint` prop and the `types` prop on the two `AccountSelect`s were added; the tag field is unchanged.

`TransactionEditor.tsx`, `TransactionEditorPage.tsx`, `TransactionsPage.tsx` and `editor-model.ts` got real changes.

## Checks

- `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, and `SQLX_OFFLINE=true cargo test --workspace`: 487 tests, all pass (16 new: 14 app, 2 HTTP).
- `cargo llvm-cov --workspace --summary-only`: **95.69% lines** in total. `templates/*` is between 95.8% and 98.9% of lines, and `routes/templates.rs` is at 100%.
- In `web/`, `npm run lint`, `npm run typecheck` and `npm test` all pass: 163 Vitest tests, of which 9 are templates page/flow tests and 10 are prefill/reorder model tests.
- Playwright: all 13 flows pass, including the new `e2e/templates.spec.ts`. It saves a transaction as a template, uses it with a new amount through the picker, duplicates a transaction, and checks the templates page; it logs in once.
