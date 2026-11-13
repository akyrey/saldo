# Hand-off: Phase 5, work packages 4 (piggy banks) and 5 (recurring transactions)

Branch `phase-5-piggy-recurrences`, based on main at a653d05. It has not been merged with main, which has since gained budgets, reports, webhooks and notifications. Nothing was pushed.

## Commits

| Commit | What |
|---|---|
| f97319e | core: `recurrence::occurrences` and the `Pattern`/`WeekendPolicy`/`Repetition`/`RecurrenceSchedule` types |
| 523bb70 | core: `piggy_bank::check_event`, `check_imported_event`, `room_to_add` |
| 52feccd | app: `PiggyBankService` (CRUD, linked accounts, events) |
| 4c62cba | app: `RecurrenceService` (CRUD, `fire`, `fire_due`, `preview`), migration `20261113300000_recurrence_fired_through.sql`, `transactions/recurring.rs` |
| bb18591 | server: `/piggy-banks*` and `/recurrences*` routes, the `recurrence-runner` worker, regenerated `web/openapi.json` and `schema.d.ts` |
| 5844d23 | importer: `piggy_banks`, `recurrences` and `piggy_bank_events` entities, with a spec-based fixture |
| 0c403d2 | web: piggy bank list with progress bars, a create form and an add/remove money dialog |
| fde2e3f | web: list of recurring transactions with "create today's transaction", an editor that reuses `SplitRowFields`, and a preview of upcoming dates |
| 3301ebf | web: Playwright flow that creates and fires a recurrence and checks the piggy bank balance guard |

## What was built

### Core (`saldo-core`)

- `recurrence::occurrences(schedule, from, to) -> Vec<NaiveDate>` handles daily, weekly, monthly, nth weekday and yearly repetitions, plus `skip`, the four weekend policies, `repeat_until`, `max_occurrences` (counted from `first_date`, not from the window), month-end clamping and leap years. Each repetition is a lazy `impl Iterator`; the results of several repetitions are merged in date order without duplicates.
  - 21 schedule tests, including a proptest that splitting a window gives the same dates as asking for the whole window.
  - The schedule rules, each covered by a test:
    - A weekend shift never moves a date before `first_date`.
    - A weekend date dropped by `skip` does not count towards `max_occurrences`.
    - A fifth weekday that a month does not have becomes that month's last one.
- `Pattern::parse(kind, moment)` and `moment()` convert to and from the stored `kind`/`moment` columns.
- `RecurrenceId` and `PiggyBankId` live in their feature modules, not in `ids.rs`, to avoid merge conflicts.
- Piggy bank rules are pure functions:
  - An amount of zero is refused.
  - An account's saved amount cannot drop below zero.
  - Adding money cannot push the total past the target, or an account's saved amount past its balance.
  - `target_reached` is reported when an event crosses the target.
  - Imports skip the balance and target checks.

### App (`saldo-app`)

- **`app.piggy_banks`:**
  - `create|update|delete|get|list` (with `_in` variants), `add_event(_in)`, `list_events(_in)` and `linked_side_of_split_in`.
  - Linked accounts must be active-or-not *asset* accounts of the household in the piggy bank's currency.
  - An account still holding saved money cannot be unlinked (409).
  - An event may name a transfer split that moves money into or out of its account.
  - Adding or removing money appends `piggy_bank.updated` with the new saved amount; crossing the target also appends `piggy_bank.target_reached`.
- **`app.recurrences`:**
  - `create|replace|delete|get|list`, `fire(_in)`, `fire_due`, `preview` and `mark_fired_through_in`.
  - Template splits are validated with the ledger rules when the recurrence is saved, as a group dated on the first date.
- **Firing:**
  - `fire` goes through a new crate-internal `TransactionService::create_for_recurrence_in` in `transactions/recurring.rs`. That method calls `create_in`, so the group gets the same validation and the same `transaction.created` event as any other. It then sets `recurrence_id` and `recurrence_date`.
  - Firing twice for the same date returns the existing group with `created: false`; the UNIQUE constraint guards against races.
  - Firing appends `recurrence.fired` with `{ recurrence_id, date, created, group }`.
  - The events come from `Origin::Scheduler` when the runner fires and `Origin::Api` when the endpoint does.
  - A template split with `piggy_bank_id` that becomes a transfer into (out of) a linked account adds (removes) money. The amount is capped to what the rules allow, so a full piggy bank never blocks the firing.
- **`fire_due(ctx)`:**
  - Fires every active recurrence for each date between `fired_through + 1` (or `first_date`) and the household's today. Each date runs in its own transaction.
  - A date that fails (for example, an archived account) is reported in `FireDueReport.failures`, and the watermark stops before it, so it is retried. A database error aborts the run.
  - Inactive recurrences advance the watermark without firing, so reactivating one does not catch up.
- **Rules hook:** `apply_rules` is stored and shown. Where the engine plugs in is marked by a comment in `crates/app/src/recurrences/fire.rs` (`fire_in`, after the piggy bank step): run the engine on `view.group` there, in the same transaction, with `Origin::Rule`, when `row.apply_rules` is set.

### Server

- **Endpoints:** listed under "API additions" below.
- **`recurrence-runner` worker:**
  - Registered in `workers::start` (`crates/server/src/workers/recurrences.rs`).
  - Runs at startup and then hourly, without an event signal.
  - Loops over `household_contexts()` and logs failures as warnings.
  - Calls `state.events.notify()` when it created something.
  - `run_once` is public for tests.

### Importer

- **Entity order:** `currencies, accounts, categories, tags, piggy_banks, recurrences, transactions, attachments, piggy_bank_events`.
- **`piggy_banks`:**
  - Field mapping:
    - The `accounts[].account_id` refs become the linked accounts.
    - A `target_amount` of `"0"` or `null` means no target.
    - Date-times keep only their date part.
    - `order` becomes `sort_order`.
  - A piggy bank whose account was not imported is skipped and reported.
  - Ref type: `piggy_bank`.
- **`piggy_bank_events`:**
  - Firefly events carry no account. The account comes from the transfer journal (`transaction_journal_id` → `split` ref → the linked side); without one, the first linked account is used.
  - The date is the date part of `created_at`.
  - Events never change in Firefly, so one that was already imported counts as unchanged.
  - Ref type: `piggy_bank_event`.
- **`recurrences`:**
  - Repetitions: `ndom` becomes `nth_weekday`, a yearly moment `2026-03-15` becomes `03-15`, and `weekend` 1 to 4 becomes keep, skip, previous_friday, next_monday. `nr_of_repetitions` becomes `max_occurrences`.
  - Cash accounts resolve by direction, as in the transactions importer.
  - Tags resolve by name. Categories, piggy banks, budgets (ref type `budget`) and bills (ref type `bill`, from `subscription_id`) resolve through `external_refs`. A missing link is dropped with a note.
  - **New recurrences are marked handled up to Firefly's `latest_date`, or today**, because Firefly already created those transactions.
  - Idempotent by comparison; ref type `recurrence`.
- **Fixtures:**
  - The owner has no piggy banks, so the piggy bank shapes come from the public spec (`firefly-iii-v6.7.8-v1.yaml`).
  - New fixture files: `piggy-banks-spec.json` and `piggy-bank-events-1.json`, with a README note.
  - The existing `import.rs` mocks now also serve `piggy-banks.json` (empty) and `recurrences.json`.

### Web

- **`/piggy-banks`:** cards show the saved amount, the target, a progress bar and the linked accounts. The progress percent uses exact `bigint` maths in `progress.ts`.
  - The "New piggy bank" form offers active asset accounts only.
  - The add/remove money dialog sends a signed amount and shows the server's field error.
- **`/recurrences`:** each recurrence shows its schedule as a sentence, its accounts and its last created date.
  - A "Create today's transaction" button fires it and reports whether a transaction was created or already existed.
- **`/recurrences/new` and `/recurrences/$id/edit`:**
  - Fields: title, dates, limits, active, apply rules, repetitions (kind, moment, skip, weekend) and template splits.
  - Template splits reuse `SplitRowFields` through a new optional `template` prop that hides the date and notes. A piggy bank picker appears under each split.
  - An "Upcoming dates" panel asks `POST /recurrences/preview`, debounced by 300 ms.
- **Navigation:** two NAV entries were appended, "Piggy banks" and "Recurring". The mobile tab bar now sizes its columns from the number of entries (an inline `gridTemplateColumns`) instead of the hard-coded `grid-cols-4/5`, so more tabs from other tracks fit.
- **Query keys:** `["piggy-banks"]`, `["recurrences"]`, `["recurrences", "one", id]` and `["recurrences", "preview", body]`.

## API additions

All endpoints from ARCHITECTURE.md 6 for these two areas, plus:

- **`POST /recurrences/preview`** (not in ARCHITECTURE.md): the request is `{ first_date, repeat_until?, max_occurrences?, repetitions[], count? }` and the response is `{ data: ["YYYY-MM-DD", ...] }`. It returns the next dates from the household's today, 5 by default and at most 100, looking 5 years ahead. The editor needs it to preview a schedule that is not saved yet, without duplicating the schedule logic in TypeScript.
- **`POST /recurrences/{id}/fire`:**
  - The body is `{ date? }`; the date defaults to the household's today.
  - It answers 201 when it created a group and 200 with `created: false` when the group already existed. It does not require the date to be a scheduled date, because it doubles as "fire now".
- **List filters:** `GET /piggy-banks` and `GET /recurrences` accept `active` as well as `page`/`per_page`.
- **Event types:** `piggy_bank.created|updated|deleted|target_reached` and `recurrence.created|updated|deleted|fired`.

## Deviations from ARCHITECTURE.md (need the owner's approval; ARCHITECTURE.md not edited, as instructed)

1. **New migration `20261113300000_recurrence_fired_through.sql`** adds `recurrences.fired_through TEXT`, the runner's progress. I kept it because the job cannot be done correctly without it:
   - An imported recurrence starts in the past (the fixture's rent starts on 2026-01-02). Without a watermark, the runner would recreate every past occurrence that Firefly already created, and those transactions arrive through the transactions import as well.
   - Deleting a transaction a recurrence created would make it reappear on the next run.
   - Reactivating a recurrence would backfill every date it skipped while inactive.

   `transaction_groups.recurrence_date` alone cannot express "handled but deliberately not created". The watermark is bookkeeping, like a webhook cursor, so moving it appends no event.
2. **The importer runs `piggy_bank_events` after `transactions`.** ARCHITECTURE.md 9.1 has piggy banks at step 6 and transactions at step 9, but an event's `transaction_journal_id` can only be resolved once the splits exist. Piggy banks and their accounts still import at step 6; only the events come last.
3. **Bill and recurrence schedules use separate types.** ARCHITECTURE.md 4.3 sketches one `occurrences(schedule: &Schedule, …)` for both. Bills were built in parallel on another track, so my function is `saldo_core::recurrence::occurrences` with `RecurrenceSchedule`. It is not re-exported at the crate root, so it cannot clash with the bills track's names. The lead may unify the two.
4. **`POST /recurrences/preview`** is a new endpoint (see "API additions").
5. **Piggy bank accounts must be asset accounts.** Firefly also only offers asset accounts.

## Gaps filled by other tracks, and things left undone

- **Budgets and bills in templates:** the transaction service still rejects budget and bill IDs on this branch (`unknown_reference`), so a template split that names one fails validation (tested). After the merge, budget and bill links work as soon as the transaction service knows those tables.
  - The importer resolves them through ref types `budget` and `bill`. If the budgets or bills importers record different ref names, change `BUDGET_REF`/`BILL_REF` in `crates/importer/src/entities/recurrences.rs`.
  - The template column `recurrence_splits.budget_id` is used as is.
- **Rules:** see the "Rules hook" note in the app section above.
- **Export:** the new tables were left out on purpose; the parent adds them after merging.
- **`docs/STATUS.md` and `docs/ARCHITECTURE.md`:** not edited, as instructed. They need these deviations and the API additions.
- **Merge points with main:**
  - lib.rs, `routes/mod.rs`, `workers::start`, `engine::ENTITIES` and its match, `router.tsx`, `AppLayout` NAV.
  - `crates/importer/tests/import.rs`: two mock entries.
  - `SplitRowFields.tsx`: the `template` prop.
  - `web/openapi.json` and `schema.d.ts` must be regenerated after merging.
- **Playwright:**
  - The flow lives in `web/e2e/recurrences.spec.ts`. It uses one login, because the suite shares the login rate limit; on main it should switch to the cached-session login helper.
  - It relies on running after `flows.spec.ts`, which does the setup (files run in name order).
  - Port 8099 was in use by another process here, so I ran the suite with `E2E_PORT=8137`.
- **Runner wrapper coverage:** `workers/recurrences.rs` is at 55% line coverage. Only the logging of failures inside `run_once` is untested; `run_once` itself is tested.

## Test results (at 3301ebf)

- `cargo fmt --all --check`: clean.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `SQLX_OFFLINE=true cargo test --workspace`: 302 passed, 0 failed.
  - core: 112 tests (26 new for recurrences, 11 for piggy banks).
  - app: 12 piggy bank and 10 recurrence integration tests.
  - server: 4 HTTP tests.
  - importer: 4 new tests (first import, second import changes nothing, updates, mapping and no backfill). The existing 6 still pass.
- `cargo llvm-cov`: 95.57% lines workspace-wide; the new core files are 98.9 to 100%, and the app files 92.7 to 100%.
- Web:
  - `npm run lint`, `npm run typecheck`, `npm run build`: clean.
  - `npm test`: 89 passed.
  - `npm run size`: the initial route is 131.8 kB gzipped, up from 130 kB because of the two new icons in the layout.
  - Playwright: 7 passed, the 6 existing flows plus the new one.
