# Phase 9: statement import, reconciliation, forecast, subscription detection, budget carry-over

Requested by the owner on 2026-11-17. Before starting a track, add its tables, endpoints and events to `docs/ARCHITECTURE.md` (sections 5, 6 and 8.1), since that file is the contract. Each track's design below is what goes there.

## 1. Goal

Less typing and more trust in the numbers. When finished, the owner can:

- **Import a bank statement** (CSV, CAMT.053 or OFX), review it with duplicates and rule results marked, and commit it, then undo the whole import if it was wrong.
- **Reconcile an account** against a statement's date and closing balance, booking any difference as a reconciliation transaction.
- **See a cash-flow forecast**: projected balances up to two years ahead from recurrences, bills and future-dated transactions, with the lowest point marked.
- **Get subscription suggestions**: repeating payments found in past transactions, offered as bills that can be accepted or dismissed.
- **Carry budget leftovers live**: carried amounts are computed when read, so a late edit in an earlier period updates later ones. Overspending can optionally carry too, and hand-made limits carry like auto-budget limits.

## 2. Read first

- `CLAUDE.md`, `docs/STATUS.md`
- `docs/ARCHITECTURE.md` sections 4.2 (balances and signs), 4.3, 5.2 (`splits.reconciled`, equity roles), 5.3 (budgets, bills), 5.4 (recurrences, `fired_through`), 5.6 (`external_refs`, `import_runs`), 6, 8.1, 8.5, 12
- Existing code: `crates/app/src/db.rs` (`begin`), `crates/core/src/budget.rs` (`auto_limits`), `crates/app/src/budgets/`, `crates/core/src/bills.rs` (`matching_due_date`), `crates/app/src/bills/periods.rs` (`materialise_bill`), `crates/core/src/recurrence/`, `crates/app/src/reports/`, `crates/app/src/transactions/`, the rule engine's dry run (`POST /rules/{id}/test`), `web/src/features/{budgets,reports,accounts,transactions}/`

## 3. Prerequisites

Phases 0 to 8 are done. The owner provides anonymised sample statements from their own banks (at least one CSV, and CAMT.053 and/or OFX if their banks offer them) for the parser fixtures. Until they arrive, use the ISO 20022 and OFX specification examples and mark the gap in `STATUS.md`.

## 4. Decisions taken for this phase

Chosen by the owner on 2026-11-17:

- Budget carry-over: computed live, optional carrying of overspending, and also for hand-made limits.
- Reconciled splits can be edited or deleted after a warning in the editor. The server allows it and un-reconciles the split when its amount, date or accounts change.
- Subscription detection suggests **bills** only.
- Statement import runs in the web app (upload, review, commit). There is no CLI command.

Proposed in this brief (confirm or change them when reviewing it; record the confirmation in `STATUS.md`):

- Statement parsers live in `saldo-app::statements::parse` as pure functions over `&[u8]` (no database, no clock). They do not go in `saldo-core`, so the domain crate takes no CSV/XML dependencies.
- Statement drafts (preview lines and per-line edits) append no events, like the other outboxes and drafts in 8.1. Creating, committing, discarding and reverting an import do.
- The forecast counts each bill at `amount_max` (pessimistic), not at the average of the min and max amounts.

## 5. Scope

In scope: the work packages below, including migrations, `core` logic, services, events, endpoints, OpenAPI and TS schema regeneration, web screens, export of new tables, and tests.

Out of scope: direct bank connections (PSD2/GoCardless), exchange-rate conversion (statement lines in a currency other than the account's are skipped with a reason), budget amounts in the forecast, recurrence suggestions, and a CLI statement import.

## 6. Work packages

Package 1 comes first and is small. After it, tracks A to D are independent and can be given to parallel agents. Track E comes after D, because it uses reconciliation (the "Reconcile now" step and the undo check). Within a track the order is core → app → server → web.

### 1. Write-transaction guard (the `BEGIN IMMEDIATE` follow-up)

The fix itself landed on 2026-11-16 (5b061f8): every write goes through `saldo_app::db::begin`. What is missing is a guard against regressions:

- Add `disallowed-methods` to `clippy.toml` for `pool.begin()`, `Connection::begin` and `Acquire::begin`, with the reason "use saldo_app::db::begin (BEGIN IMMEDIATE)". Use the canonical `sqlx_core` paths. Prove that the lint fires by adding a call temporarily, and allow it on `db::begin` only if needed.
- Add a test in `saldo-app` that two concurrent read-then-write transactions both succeed: one waits on the busy timeout instead of failing with code 517.
- Remove the out-of-date SQLite contention item from "Follow-ups" in `STATUS.md`.

### Track A: live budget carry-over

- **Schema** (new migration): `budgets.carry TEXT NOT NULL DEFAULT 'none' CHECK (carry IN ('none', 'leftover', 'all'))`. `budget_limits.amount_minor` now always means the **base** amount.
- **Data migration** in the same file: for budgets with `auto_type = 'rollover'`, set `carry = 'leftover'` and `auto_type = 'reset'`. Their limits in the auto currency with `amount_minor > auto_amount_minor` are set to `auto_amount_minor`, because the carry used to be baked into the stored amount (Firefly imports did the same). The old `CHECK` still allows `'rollover'`, but the service never writes it again. Record this in the architecture doc.
- **Core**: `carried(limits: &[Limit], spends: &[Spend], carry: Carry) -> Result<Vec<Money>, MoneyError>`, one entry per limit:
  - Limits are one budget's limits in one currency, sorted by start date.
  - A limit carries to the next only when the next starts the day after it ends. A gap resets the carry to zero, and the first limit carries zero in.
  - The carry is `left = base + carried_in - spent`. With `leftover` only a positive `left` carries; with `all`, a negative one carries too; with `none` nothing does.
- **Core cleanup**: `auto_limits` no longer adds leftovers; it creates base-amount limits only. Delete the rollover branch and its tests, and move the useful cases to `carried`.
- **App**: the limit view gains `carried`; `available` (what is left) becomes `amount + carried - spent`, so existing clients keep its meaning. Computing a budget's limits loads that budget's whole chain once (a handful of budgets with monthly limits; no caching). Changing `carry` appends `budget.updated`. Derived values append nothing.
- **Server**: the budget's `carry` is on create and `PATCH`; `auto_budget.type` accepts only `reset`. The budget-vs-actual report keeps base amounts only: over several intervals a carried leftover would be counted once per interval it passes through. The Firefly importer maps `rollover` to `reset` plus carry `leftover` and imports its limits at the base amount, by the same rule as the migration.
- **Web**: a "Carry over" select on the budget form (`Nothing`, `What is left`, `What is left or overspent`). Progress bars show `base + carried` when `carried` is not zero, and a negative carry is shown in the danger colour. Remove the `rollover` option from the auto-budget type.

### Track B: cash-flow forecast

- **Core**: `forecast(start: &[(AccountId, Money)], items: &[ForecastItem], from, to, interval) -> Result<Forecast, MoneyError>`. A `ForecastItem` has `date`, an optional `account`, a signed `amount` and a `source`. The result has per-currency points (the balance at the end of each interval) and the lowest point. Items without an account count only in the total.
- **App** (`ReportService::forecast`): the start is each included account's balance as of today, in the household's timezone. Included accounts are asset and liability accounts with `include_net_worth`, filtered by scope, or the one `account_id`. Items come from three sources:
  1. **Existing splits dated after today.** Firefly left some in the data.
  2. **Recurrences.** Active ones, with dates from `max(fired_through + 1, tomorrow)` to `to`, honouring `repeat_until` and `max_occurrences` (counting what has already fired). Each template split moves its asset or liability accounts by `-amount` (source) or `+amount` (destination).
  3. **Bills.** The open periods of active bills, at `amount_max`, dated `max(due_date, tomorrow)` so overdue ones count as due tomorrow. The account is the source account of the split that paid the bill's latest paid period; with no paid period, the item goes in the total only, as "account unknown". A bill that an active recurrence's template split names in `bill_id` is skipped, because the recurrence already counts it.
- **Server**: `GET /reports/forecast?to=&interval=&scope=&account_id=` (`from` is today; `to` at most 2 years ahead, otherwise 422; `interval` is `day`, `week` or `month`, default `week`). The response is `{ range, currencies: [{ currency, start_balance, points: [{ date, balance }], lowest: { date, balance }, items: [{ date, source, source_id, title, account_id, amount }] }] }`.
- **Web**: a "Forecast" report page with a balance line chart (Recharts), a horizon switch (1, 3, 6, 12 or 24 months), an account filter, a warning when the lowest point is below zero, and the item list grouped by date. A "Next 30 days" card goes on the dashboard.

### Track C: subscription detection

- **Schema**: `subscription_dismissals (household_id, account_id, currency_code, created_at, PRIMARY KEY (household_id, account_id, currency_code))` with a foreign key to `accounts`.
- **Core**: `detect_subscriptions(payments: &[Payment], today) -> Vec<Candidate>`. A `Payment` is `split_id`, `date`, `amount`, `counterparty` (the expense account) and `has_bill`. The rules:
  - Group withdrawals by `(counterparty, currency)`. Drop groups where any payment has a bill.
  - Only payments within 20% of the latest payment's amount count (integer maths in minor units), so a price rise keeps the newer payments together.
  - Try frequencies from shortest to longest, comparing the gaps between consecutive payments:

    | Frequency | Gap tolerance | Payments needed |
    |---|---|---|
    | weekly | 7 ± 2 days | 3 |
    | monthly | same day of month ± 4 days, month-end clamped | 3 |
    | quarterly | 91 ± 7 days | 3 |
    | half-year | 182 ± 10 days | 2 |
    | yearly | 365 ± 15 days | 2 |

    At least 75% of gaps must fit. The first frequency that fits wins.
  - Still active: the last payment is no older than one period plus its tolerance.
  - A candidate has the counterparty, currency, frequency, observed `amount_min` and `amount_max`, the first and last payment dates, the next due date, the matched `split_ids` and a `confidence` (the fraction of gaps that fit, in percent as an integer).
- **App** (`SubscriptionService`): `candidates(ctx, today)` scans the last 18 months of withdrawals in scope and leaves out dismissed `(account, currency)` pairs; it is computed on each request, with no table of candidates. Two state changes:
  - `accept`: creates the bill through `BillService` (name = the counterparty account's name, editable; `first_due_date` = the first matched payment). It sets `bill_id` on the matched splits through `TransactionService`, so `matching_due_date` pays their periods, then skips the periods still open with a due date before the last matched payment. All of this runs in one database transaction, so the new bill does not start with overdue reminders.
  - `dismiss`: inserts the dismissal and appends `subscription_dismissal.created` (restoring appends `subscription_dismissal.deleted`, following the `<entity>.<verb>` naming).
- **Server**: `GET /subscription-candidates?scope=`, `POST /subscription-candidates/accept` (`{ account_id, currency, name, frequency, amount_min, amount_max, split_ids }`; 422 if a split is not a withdrawal to that account or already has a bill), `POST /subscription-candidates/dismiss` (`{ account_id, currency }`), and `DELETE /subscription-dismissals/{account_id}/{currency}`.
- **Web**: a "Suggestions" section on the bills page, with each candidate's payments, frequency, amount range and next date, an "Add as bill" dialog prefilled with the candidate, and "Dismiss". A dashboard hint shows when there are candidates.

### Track D: reconciliation

- **Schema**:
  - `reconciliations`: `id`, `household_id`, `account_id`, `statement_date`, `statement_balance_minor`, `currency_code`, `difference_group_id` (nullable, the reconciliation transaction), `created_by_user_id`, `created_at`, with a foreign key to `accounts`.
  - `reconciliation_splits (reconciliation_id, split_id)`, cascading on both sides.
  - An account's "reconciled up to" date is its latest reconciliation's `statement_date`, derived, with no new column.
- **Core**: `reconcile(statement_balance: Money, cleared: &[SignedAmount]) -> Result<Option<Money>, MoneyError>` returns the difference, or `None` when it is zero. Cleared means reconciled or selected splits dated on or before the statement date, plus every `opening_balance` split, which always counts as cleared. Signs follow ARCHITECTURE 4.2; the virtual balance is not part of it.
- **App** (`ReconciliationService`):
  - `draft(ctx, account, statement_date)` returns the cleared balance and the unreconciled splits up to the date.
  - `create(ctx, account, statement_date, statement_balance, split_ids, book_difference)` checks that the splits belong to the account, are unreconciled and are dated on or before `statement_date`. If there is a difference and `book_difference` is false, it is a 422 with the difference in `details`. Otherwise it creates a `reconciliation` group against the Equity(Reconciliation) account dated `statement_date`, sets `reconciled = 1` on the splits, writes the rows and appends one `reconciliation.created` event (with the split IDs), all in one transaction.
  - `delete` works on the latest reconciliation of the account only (otherwise 409). It un-reconciles its splits, deletes the difference group and appends `reconciliation.deleted`.
- **Transactions**:
  - `GET /transactions` gains a `reconciled` filter.
  - In `TransactionService::replace`, a reconciled split whose amount, date or accounts change is set to `reconciled = 0` and removed from `reconciliation_splits`; other edits keep it reconciled.
  - Deleting a group removes its links. The `transaction.*` event shows the new state.
- **Server**: `GET /accounts/{id}/reconciliations` (newest first), `GET /accounts/{id}/reconciliations/draft?statement_date=`, `POST /accounts/{id}/reconciliations`, and `DELETE /reconciliations/{id}`. Accounts must be asset or liability accounts (otherwise 422).
- **Web**:
  - A "Reconcile" action on an asset or liability account opens a page with:
    - the statement date and balance;
    - a list of unreconciled splits with checkboxes, plus "select all up to the date" for the first reconciliation of years of imported history;
    - a running difference;
    - "Finish", or "Book difference and finish".
  - The account page shows "reconciled up to …" and the history, with undo on the latest entry.
  - The transaction editor shows a warning on reconciled splits and asks for confirmation before saving changes to their amount, date or accounts, or before deleting them.
  - Add `reconciliation` to the live-event keys.

### Track E: bank statement import

- **Dependencies** (`cargo add` in `saldo-app`): `csv`, `quick-xml`, `encoding_rs`.
- **Schema**:
  - `statement_profiles`: `id`, `household_id`, `name` (unique per household), `account_id` (nullable default account), `format` (`csv`, `camt053`, `ofx`), `csv` (JSON, CSV only), `created_at`, `updated_at`. The `csv` JSON has:
    - `delimiter`, `encoding` (`utf-8` or `windows-1252`), `skip_rows`, `has_header`
    - `date_column`, `date_format`
    - either `amount_column` (signed) or `debit_column` + `credit_column`
    - `decimal_separator`
    - `description_columns[]`, `counterparty_column`, `counterparty_iban_column`, `external_id_column`
  - `statement_imports`: `id`, `household_id`, `account_id`, `profile_id` (nullable), `filename`, `sha256`, `status` (`draft`, `committed`, `discarded`, `reverted`), `closing_balance_minor` and `closing_date` (nullable, from CAMT `CLBD` or OFX `LEDGERBAL`), `summary` (JSON counts), `created_by_user_id`, `created_at`, `committed_at`.
  - `statement_lines`: `import_id`, `position`, `date`, `amount_minor` (signed), `currency_code`, `description`, `counterparty_name`, `counterparty_iban`, `external_id`, `fingerprint`, `action` (`create`, `link`, `skip`), `status` (`new`, `duplicate`, `possible_match`, `unsupported`), `reason`, `split_id` (the matched or created split), `counterparty_account_id`, `category_id`, primary key `(import_id, position)`.
- **Parsers** (pure, `saldo-app::statements::parse`; one `match` on the format enum, no trait):
  - **CSV**: driven by the profile.
  - **CAMT.053**: one line per `TxDtls` when it has its own amount, otherwise one per `Ntry`. Read `Amt@Ccy`, `CdtDbtInd`, `BookgDt`, the unstructured remittance info, `RltdPties` `Cdtr`/`Dbtr` name and IBAN, and `AcctSvcrRef`. The statement IBAN and the `CLBD` balance are returned too.
  - **OFX 1.x (SGML, unclosed tags) and 2.x (XML)**: read `STMTTRN` (`TRNAMT`, `DTPOSTED`, `FITID`, `NAME`, `MEMO`) and `LEDGERBAL`. Write a small tolerant tag reader for 1.x; do not pull in an OFX crate unless one is maintained and small.
  - Limits: a 5 MB upload limit on the route and 10,000 lines. Never resolve DTDs or external entities. A malformed file is a 422 with the line or element that failed.
- **Duplicates**: the `fingerprint` is SHA-256 of `account_id | date | amount | normalised description`. A line is marked:
  - `duplicate` when `external_refs` (source `bank-statement`, entity `split`, external ID `account_id:` + `external_id`, or the fingerprint when the bank gives no ID) already maps it;
  - `possible_match` when an existing split on the account has the same amount within ±3 days and no external ref (for example, entered by hand or created by a recurrence);
  - `new` otherwise.

  This makes importing the same or an overlapping file again create nothing new. The default actions are `create` for `new`, `link` for `possible_match` and `skip` for `duplicate`.
- **Counterparties**:
  - An IBAN that matches one of the household's own asset or liability accounts makes the line a transfer.
  - Otherwise, an IBAN that matches an expense or revenue account, then a case-insensitive exact name match, picks the account.
  - Failing both, a new expense or revenue account named after the counterparty (or the description) is created on commit. The review screen can change it per line.
- **App** (`StatementService`):
  - `upload` parses, classifies and stores a draft, and appends `statement_import.created`.
  - `update_line` edits a draft line's `action`, `counterparty_account_id`, `category_id`, `description` or `split_id`, with no event.
  - `commit` runs in one transaction:
    - Each `create` line becomes a group through `TransactionService` with `Origin::Import` (withdrawal, deposit or transfer by sign and counterparty) plus an external ref. Rules must run on these groups: check that the engine runs for `Origin::Import`, and if it skips it, add an explicit option instead of changing the Firefly importer's behaviour.
    - Each `link` line only adds the external ref.
    - The service appends `statement_import.committed` and `import.completed` (`{ source: "bank-statement", ... }`).
  - `discard` handles drafts. `revert` handles committed imports: it deletes the groups the import created, unless one is reconciled (409), removes the refs, sets `reverted` and appends `statement_import.reverted`.
  - The preview shows rule results by running the existing dry run on each proposed split.
- **Server**: CRUD on `/statement-profiles`; `POST /statement-profiles/preview` (multipart file + profile, returns the first 20 parsed lines or the parse error, saves nothing, for the mapping editor); `POST /statement-imports` (multipart: file, `account_id`, `profile_id` or `format`); `GET /statement-imports` and `GET /statement-imports/{id}` (with lines); `PATCH /statement-imports/{id}/lines/{position}`; `POST /statement-imports/{id}/commit`; `POST /statement-imports/{id}/discard`; and `POST /statement-imports/{id}/revert`. The account must be an asset or liability account in the line currency.
- **Web**: "Import statement" on an account and in the transactions page, as a wizard:
  1. Choose a file. CAMT and OFX pick the account by IBAN when it matches; a CSV needs a saved profile or a mapping editor with a live preview, saved as a profile.
  2. Review: a table with status badges, rule results, a per-line action and pickers for account and category, plus bulk "skip all duplicates".
  3. Commit.
  4. When the file had a closing balance: "Reconcile now", which opens Track D's page prefilled with the closing date and balance and the imported and linked lines selected.

  An imports list on the account page offers revert. Add `statement_import` to the live-event keys.

### 2. Export and docs (last)

- The JSON and CSV export add `reconciliations` (with split IDs), `statement_profiles`, `subscription_dismissals` and the budgets' `carry`. Format version stays 1, because fields are only added.
- Statement imports and their lines are left out, like `import_runs`. Record that in 9.2.
- Update `ARCHITECTURE.md` 8.5 if any background task changed. None should.

## 7. Contracts

Honour the existing conventions in section 6: the error envelope, amounts as decimal strings, `scope` on every list and report, and pagination on the imports and reconciliation lists. New event types: `reconciliation.created`, `reconciliation.deleted`, `statement_import.created`, `statement_import.committed`, `statement_import.discarded`, `statement_import.reverted`, `subscription_dismissal.created`, `subscription_dismissal.deleted`. Every new service function takes `HouseholdCtx` first and filters by `household_id`.

## 8. Rust concepts to explain

| Concept | Where | Analogy |
|---|---|---|
| Byte slices vs strings, UTF-8 and decoding (`&[u8]`, `String`, `encoding_rs`) | statement parsers | PHP strings are byte arrays with `mb_*` on top; Go's `[]byte` vs `string` is the same split, but Rust checks UTF-8 when converting |
| Streaming XML with an event loop (`quick-xml` `Reader`) | CAMT and OFX 2.x | Go's `xml.Decoder.Token()` loop / PHP's `XMLReader`, not `SimpleXML` loading the whole tree |
| A small state machine as an enum | the OFX 1.x tag reader | A Go `switch state` loop, with the compiler checking every state is handled |
| `HashMap` grouping with the `entry` API | subscription detection | PHP's `$groups[$key][] = $x`, or Go's `m[k] = append(m[k], x)` |
| Folding a running value with `scan` / a `for` loop over a slice | carry-over chain, forecast balances | `array_reduce` that also keeps every intermediate result |
| `TryFrom` for checked conversions | statement line → transaction input | A Laravel form request's validated DTO; a Go constructor returning `(T, error)` |

## 9. Acceptance criteria

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
SQLX_OFFLINE=true cargo test --workspace
cargo llvm-cov --workspace --summary-only     # >= 80%, saldo-core close to 100%
cd web && npm run lint && npm run typecheck && npm test && npx playwright test
```

- Package 1: the clippy lint rejects a `pool.begin()` call, and two concurrent read-then-write transactions both commit.
- Track A:
  - A property test: with `all`, the last limit's `left` equals the sum of the bases minus the sum of what was spent over a gap-free chain.
  - With `leftover`, `carried` is never negative.
  - Editing a transaction in an earlier period changes the next limit's `carried` and `available` with no write to `budget_limits`.
  - The migration test converts a rollover budget and its limits as described.
- Track B: a test household with a recurrence, a bill paid by an account, a bill covered by a recurrence and a future-dated split produces exactly the hand-computed points; an overdue bill period counts tomorrow.
- Track C:
  - Fixtures for monthly, yearly, irregular and price-change payment histories give the expected candidates.
  - Accepting a candidate leaves no open period before the last matched payment.
  - A dismissed pair does not come back.
- Track D:
  - Reconciling with a difference books one reconciliation group of exactly that difference.
  - Undoing the latest reconciliation restores the previous state.
  - Changing a reconciled split's amount un-reconciles it; changing its category does not.
- Track E:
  - The parsers pass on the owner's samples and on the specification examples.
  - Importing the same file twice creates nothing the second time; an overlapping file creates only the new lines.
  - A transfer between two own accounts imported from both statements results in one transfer.
  - Reverting deletes exactly the created groups, and is refused when one of them is reconciled.
  - A billion-laughs XML file and a file over 5 MB are rejected.
- Playwright flows:
  - Import a CSV statement with a new profile, commit, then reconcile from the closing balance.
  - Accept a subscription suggestion.
  - Open the forecast and change the horizon.

## 10. Hand-off

In `docs/STATUS.md`: Phase 9 done (or which tracks are done) with commit hashes; the banks and formats tested with real samples; the owner's confirmation of the proposed decisions in section 4; and any deviation from this brief, approved by the owner.
