# Phase 12: credit card plans (monthly budget, available amount, automatic settlement)

Requested by the owner on 2026-11-26. Before starting, add the tables below to `docs/ARCHITECTURE.md` section 5 (a new 5.4f), the endpoints to section 6 (a new Credit cards row), the events to 8.1, the worker to 8.5, the export files to 9.2 and the core module to section 4, since that file is the contract.

## 1. Goal

Make a credit card answer "how much can I still spend on it this month, and what will the bank pay for it?". When finished, the owner can:

- **Give a credit card account a monthly plan**: a spending budget (for example 1 500.00 EUR), the bank account that pays the card, and the day of the following month on which it pays (for example the 15th).
- **See what is still available** in the current month: budget − spending, updated with every transaction on the card, on the account page and in the accounts list.
- **Get the settlement transfer automatically**: for every month, a transfer from the bank account to the card, dated on the settlement day of the following month, whose amount follows the month's spending until that date and is then left alone.
- **Move a transaction to the next month by hand** ("Count in next month") and move it back, without changing its date. A moved transaction counts toward the next month's budget and settlement instead of its own.

## 2. Read first

- `CLAUDE.md`, `docs/STATUS.md`
- `docs/ARCHITECTURE.md` sections 4.1 and 4.2 (split kinds, balance sign convention), 5.2 (`accounts.asset_role`, `transaction_groups`, `splits`), 5.4 (`recurrences`, `fired_through`: the same idempotency idea), 6 (conventions), 8.1 (events and origins), 8.5 and 8.6 (background tasks, the `alert-scheduler`'s "after writes" wake-up), 9.2 (export)
- Existing code:
  - Core: `crates/core/src/{period.rs,carry.rs,kind.rs,money.rs}`
  - App: `crates/app/src/accounts/{input.rs,view.rs,balances.rs}`, `crates/app/src/recurrences/fire.rs` (creating groups from a background task through `TransactionService::create_in`), `crates/app/src/transactions/{mod.rs,list.rs}`, `crates/app/src/alerts/` (a per-household run with origin `scheduler`), `crates/app/src/export/`
  - Server: `crates/server/src/workers/{mod.rs,recurrences.rs}`, `crates/server/src/routes/accounts.rs`
  - Web: `web/src/features/accounts/`, `web/src/features/transactions/`, `web/src/features/live/useLiveEvents.ts`

## 3. Prerequisites

Phases 0 to 11 are done. Phase 9 Track E is not started and touches different files.

## 4. Decisions

Decisions 1 to 4 were chosen by the owner on 2026-11-26; the others were confirmed by the owner the same day, with 13 changed to a budget per month.

1. **The settlement amount is what was spent**: the month's withdrawals from the card minus deposits (refunds) to it, after moves. The budget is only the ceiling used for "available"; it never changes the transfer. *(owner, 2026-11-26)*
2. **The settlement transfer is live**: it is created as soon as a month has spending, dated on the settlement day of the following month, and its amount follows the month until that date. *(owner, 2026-11-26)*
3. **A card month is a calendar month** (1st to last day, household time); edge cases are moved by hand. *(owner, 2026-11-26)*
4. **A transaction can only be moved to the next month** (and back). Its date is unchanged. *(owner, 2026-11-26)*
5. **Only asset accounts with `asset_role = 'credit_card'` can have a plan**, at most one each. The paying account must be an active asset account other than the card, **in the card's currency** (a transfer between currencies would need a foreign amount the plan cannot know). Otherwise 422.
6. **What counts toward a month**: splits that are a withdrawal with the card as source (+amount) or a deposit with the card as destination (−amount), in the month of their date, or the following month when moved. Transfers (including the settlement itself and any other payment to the card), opening balances and reconciliations never count. A month's spending may be negative (more refunds than purchases); then there is no settlement.
7. **The settlement day is 1 to 31**, clamped to the last day of a shorter month (31 → 28 or 29 February). No weekend shifting; the owner can edit a single transfer's date by hand (decision 10).
8. **A plan starts with the month it is created in.** Earlier months (imported history) never get a settlement, are not listed as cycles, and their transactions cannot be moved.
9. **Each month's settlement has a state**, stored per card and month:
   - `synced`: the server owns it. While today (household time) is before the settlement date, the transfer is created, updated (amount, date, paying account, description) or deleted to match the month. With no spending (zero or negative) there is no transfer.
   - `frozen`: from the settlement date on, or once the transfer's split is reconciled, the transfer is never changed again. Later changes to the month are shown as a **difference after settlement** in the month's view, not booked.
   - `detached`: the owner edited or deleted the transfer by hand (the server compares the group with the amount, date and accounts it last wrote). The server leaves that month alone for good; the month's view still shows spending and available, and says the settlement is managed by hand.
10. **The transfer is kept in sync by a background task, `card-settlements`**, at startup, every 15 minutes and after writes (the `alert-scheduler`'s wake-up), not inside `TransactionService`. The reads (`spent`, `available`) are always computed live, so only the transfer can lag by a moment. Its changes go through `TransactionService::create_in`/`replace_in`/`delete_in` with origin `scheduler`, so each appends the usual `transaction.*` event in the same database transaction. Rules do not run on settlements (like imports, they would fight the sync).
11. **Settlement description**: "<card name> — <Month YYYY>" (for example "Visa — November 2026"), in English like the other server-made text. No category, budget or tags.
12. **Moves are rows in their own table**, not a column on `splits`, so the ledger and the transaction editor stay untouched; deleting a split deletes its move (`ON DELETE CASCADE`). A move is allowed only on a split that counts toward the card (decision 6), from a month that is not before the plan's start and whose settlement is not `frozen`, into a month whose settlement is not `frozen`. Moving back has the same checks. *(Changed 2026-11-27, owner's request: a split from the month just before the plan's start can be moved into its first month while that month is not closed; see STATUS.md.)* A move whose split later stops counting toward the card (its accounts changed) is ignored by the maths and removed by the next `card-settlements` run.
13. **The budget is per month** *(owner, 2026-11-26)*. A plan keeps a list of budgets, each effective from a month onward; a month's budget is the latest one that starts on or before it. Setting a new budget takes effect from a month the owner picks (the current month by default, or a later one) and never changes an earlier month's budget or "available". Setting one for a month that already has its own entry replaces that entry. Budgets for months before the current one cannot be set. Changing the payment day or the paying account applies to every `synced` month at the next run.
14. **Deleting a plan** deletes the transfers of its `synced` months that are still before their date, keeps `frozen` and `detached` ones, and deletes its moves and month rows, in one transaction.
15. **Events**: `credit_card_plan.created`/`.updated`/`.deleted` (the plan view), `credit_card_move.created`/`.deleted` (`{ account_id, split_id, from_cycle, to_cycle }`), origin `api`. Month rows are bookkeeping (no events, like `alert_states`); the transfers append their own `transaction.*` events.
16. **Export**: `credit_card_plans.csv`, `credit_card_limits.csv` and `credit_card_moves.csv`. Month rows are not exported: an import rebuilds them on the next run, and frozen transfers are ordinary transactions in `transactions.csv`.

## 5. Scope

In scope: the core module, the migration, the app service and worker, the endpoints with OpenAPI and TS schema regeneration, the plan form, the card panel on the account page, available amount in the accounts list, moving transactions, live updates, export, and tests.

Out of scope:
- Statement closing days other than the month end, weekend or holiday shifting.
- Credit card alerts ("80% of the card budget used"); they would fit `alert-scheduler` later.
- Paying a card from an account in another currency.
- Filling the plan from Firefly's `monthly_payment_date` in the importer.
- Including card budgets in safe to spend or in `/reports/budget-vs-actual`.
- Liability accounts as cards (the owner's cards are asset accounts with the `credit_card` role).

## 6. Work packages

Order: core → app → server → web. One commit per package.

### Package 1: core (pure), `crates/core/src/card_cycle.rs`

- `Cycle { year: i32, month: u32 }` deriving `Ord` (months compare in time order), with `of(date)`, `next()`, `previous()`, `first_day()`, `last_day()`, `settlement_date(day: u8) -> NaiveDate` (the clamped day in the **following** month), and `Display`/`FromStr` as `YYYY-MM`.
- `effective_cycle(date: NaiveDate, moved: bool) -> Cycle`.
- `CardEntry { date, amount: i64 /* signed: + spent, − refunded */, moved: bool }` and `fn cycle_spending(entries: &[CardEntry], cycle: Cycle) -> Result<i64, MoneyError>` with `checked_add`.
- `fn available(limit: i64, spent: i64) -> Result<i64, MoneyError>`.
- `fn limit_for(limits: &[(Cycle, i64)], cycle: Cycle) -> Option<i64>`: the amount of the latest entry starting on or before `cycle` (decision 13); `None` before the first.
- `SettlementState { Synced, Frozen, Detached }`.
- `Transfer { amount, date, source }`, `Stored { Missing, Transfer(Transfer), Other }` (what the database holds for the last written group), `SettlementInput` and
  `fn settlement_step(input: SettlementInput) -> SettlementStep { change: Change, next_state }` with `Change { None, Create(Transfer), Update(Transfer), Delete }`: the whole of decision 9 as one decision table, including detecting a hand edit (stored differs from written). On the settlement date the transfer gets a last update and the month freezes in the same step, so a run missed before the date still books the right amount.
- `fn is_closed(state, settlement_date, today) -> bool`: a month refuses moves once frozen or from its settlement date.

Tests: every row of the decision table; clamping (31 January → 28/29 February settlement, 30 → 31-day months); `Cycle` parsing, ordering and the December → January rollover; property tests that `cycle_spending` over all cycles equals the sum of all amounts, and that moving one entry shifts exactly its amount from one cycle to the next.

### Package 2: migration and app, `crates/app/src/credit_cards/`

Migration (`sqlx migrate add credit_card_plans`):

```sql
CREATE TABLE credit_card_plans (
    account_id             TEXT PRIMARY KEY,
    household_id           TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
    settlement_account_id  TEXT NOT NULL,
    settlement_day         INTEGER NOT NULL CHECK (settlement_day BETWEEN 1 AND 31),
    starts_cycle           TEXT NOT NULL,     -- 'YYYY-MM', decision 8
    created_at             TEXT NOT NULL,
    updated_at             TEXT NOT NULL,
    UNIQUE (household_id, account_id),
    FOREIGN KEY (household_id, account_id) REFERENCES accounts (household_id, id) ON DELETE CASCADE,
    FOREIGN KEY (household_id, settlement_account_id) REFERENCES accounts (household_id, id)
) STRICT;

-- Decision 13: a month's budget is the row with the latest from_cycle <= it.
-- The plan's first row has from_cycle = starts_cycle.
CREATE TABLE credit_card_limits (
    household_id  TEXT NOT NULL,
    account_id    TEXT NOT NULL,
    from_cycle    TEXT NOT NULL,             -- 'YYYY-MM'
    limit_minor   INTEGER NOT NULL CHECK (limit_minor > 0),
    created_at    TEXT NOT NULL,
    PRIMARY KEY (account_id, from_cycle),
    FOREIGN KEY (household_id, account_id) REFERENCES credit_card_plans (household_id, account_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE credit_card_moves (
    split_id      TEXT PRIMARY KEY REFERENCES splits (id) ON DELETE CASCADE,
    household_id  TEXT NOT NULL,
    account_id    TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    FOREIGN KEY (household_id, account_id) REFERENCES credit_card_plans (household_id, account_id) ON DELETE CASCADE
) STRICT;

-- Bookkeeping for decision 9; not exported, no events.
CREATE TABLE credit_card_cycles (
    household_id    TEXT NOT NULL,
    account_id      TEXT NOT NULL,
    cycle           TEXT NOT NULL,            -- 'YYYY-MM'
    state           TEXT NOT NULL CHECK (state IN ('synced', 'frozen', 'detached')),
    group_id        TEXT,                     -- the settlement transfer, NULL when none
    written_amount_minor INTEGER,             -- what the server last wrote
    written_date    TEXT,
    PRIMARY KEY (account_id, cycle),
    FOREIGN KEY (household_id, account_id) REFERENCES credit_card_plans (household_id, account_id) ON DELETE CASCADE
) STRICT;
```

`group_id` has no foreign key (like `transaction_groups.recurrence_id`); a missing group on a `synced` row whose `written_*` is set means the owner deleted it, so the row becomes `detached`.

`CreditCardService` (every function takes `HouseholdCtx` first and filters by `household_id`):

- `get_plan`, `put_plan` (create or replace; decision 5's checks; `starts_cycle` = the current month on create, kept on replace; on create it also writes the first budget), `delete_plan` (decision 14).
- `set_limit(ctx, account_id, from_cycle, amount, today)`: decision 13; 422 on `from_cycle` before the current month. Appends `credit_card_plan.updated`.
- `cycles(ctx, account_id, from: Cycle, to: Cycle, today)`: one query for the card's counting splits dated from `from.previous().first_day()` to `to.last_day()` (a split dated in `from`'s previous month counts toward `from` when moved; one dated after `to` never counts toward `to`), joined with `credit_card_moves`; maths through `cycle_spending` and `limit_for`; settlement from `credit_card_cycles` and its group.
- `cycle(ctx, account_id, cycle, today)`: one month with its splits (date, description, counterparty, signed amount, `moved_in`/`moved_out`) and its settlement.
- `move_split`, `unmove_split` (decision 12; events per decision 15).
- `sync_settlements(ctx, today)`: for every plan of the household and every month from `starts_cycle` to the current month, plus the previous months whose rows are still `synced`: compute spending, detect hand edits, apply `settlement_action` through `TransactionService` with origin `scheduler`, update `credit_card_cycles`, and drop stale moves (decision 12). One database transaction per card, so a failure on one card does not stop the others; failures are returned in a report like `FireDueReport`.

Tests (integration, SQLite): a new budget from next month leaves this month and earlier ones unchanged, one from this month leaves earlier ones unchanged, setting the same month twice replaces it, a past month is refused; a withdrawal and a refund in the same month; a transfer to the card that does not count; a move into the next month and back; a move refused into or out of a frozen month and before `starts_cycle`; a split whose accounts change after a move; the sync creating, updating, deleting the transfer, freezing it on its date and on reconcile; a hand-edited and a hand-deleted transfer becoming `detached` and never touched again; a 31st settlement day in February; deleting a plan (decision 14); another household's card is a 404 everywhere; running the sync twice changes nothing the second time.

### Package 3: server

- Worker `card-settlements` in `crates/server/src/workers/`, registered beside `recurrence-runner`: at startup, every 15 minutes and on the after-write signal; today in household time; logs failures without amounts or names.
- Endpoints (tag `credit-cards`):
  - `GET /accounts/{id}/credit-card` (404 when the account has no plan), `PUT /accounts/{id}/credit-card` (`{ monthly_limit, settlement_account_id, settlement_day }`, 201 on create, 200 on replace; on replace `monthly_limit` is ignored, the budget changes through the next endpoint), `PUT /accounts/{id}/credit-card/limits/{cycle}` (`{ amount }`, 200; decision 13). The plan view lists `limits: [{ from_cycle, amount }]`, `DELETE /accounts/{id}/credit-card` (204).
  - `GET /accounts/{id}/credit-card/cycles?from=YYYY-MM&to=YYYY-MM` (default: the last 6 months up to the current one; at most 36): `[{ cycle, from, to, limit, spent, available, moved_in, moved_out, settlement: { state, group_id, date, amount } | null, difference_after_settlement }]`, newest first.
  - `GET /accounts/{id}/credit-card/cycles/{cycle}`: one month as above plus `splits: [{ split_id, group_id, date, description, counterparty: { id, name }, amount, moved_in, moved_out }]`.
  - `PUT /accounts/{id}/credit-card/moves/{split_id}` (204; moves to the next month) and `DELETE` the same path (204; moves back). 409 when a frozen month is involved, 422 when the split does not count toward the card.
  - `GET /accounts` and `GET /accounts/balances` add `credit_card: { limit, spent, available, cycle } | null` for the current month, computed in one query for the page, not one per account.
- OpenAPI registration, regenerate `web/openapi.json` and the TS schema.
- Route tests: shapes, 401, 404 for another household, 409 and 422 cases, the defaults and the 36-month cap on `cycles`.

### Package 4: export

`credit_card_plans.csv`, `credit_card_limits.csv` and `credit_card_moves.csv` in `crates/app/src/export/`, with tests; update ARCHITECTURE.md 9.2.

### Package 5: web

- **Plan form**: on the account edit page, when the asset role is "Credit card", a "Monthly plan" section: budget (money input in the card's currency; on an existing plan, "Change budget" asks "from which month", the current one by default, and lists earlier budgets with their months), "Paid from" (asset accounts in the same currency, excluding the card), "Payment day" (1–31, with "the last day in shorter months"). Save calls `PUT`, "Remove plan" calls `DELETE` with a confirm dialog that says open transfers are deleted.
- **Card panel** on the account page (`CreditCardPanel.tsx`): the current month's spent / budget with a progress bar (destructive token above 100%), the available amount, and the settlement ("€812.40 from Checking on 15 Dec", with the state: "updates until then", "booked", "managed by hand"). Below it the month's transactions with "Count in next month" / "Move back" on each row (a small "from Oct"/"to Dec" badge on moved ones), and a month picker for earlier months showing the difference after settlement when there is one.
- **Accounts list**: a card with a plan shows "€687.60 left of €1,500.00" under its balance.
- **Live updates**: invalidate the credit card queries on `transaction.*`, `credit_card_plan.*` and `credit_card_move.*`.
- **Model** (`credit-card-model.ts`, pure): progress percentage in integer basis points (no floats for amounts; the bar width is the only float), labels for states, month labels.
- **Tests** (Vitest): the model, the form's validation and same-currency filtering, the panel's states and the move buttons with a mocked API.

### Last: docs and Playwright

- Update ARCHITECTURE.md (sections listed at the top).
- One Playwright flow: create a credit card account with a plan (budget 500.00, paid from Checking, day 15), add two withdrawals of 100.00 and 50.00 this month, see "350.00 left" and a 150.00 settlement transfer dated the 15th of next month; move the 50.00 one to next month and see 400.00 left and the transfer at 100.00.

## 7. Contracts

- Endpoints follow section 6: error envelope, amounts as decimal strings, dates as `YYYY-MM-DD`, months as `YYYY-MM`.
- Amounts are `i64` minor units in the card's currency throughout; no floats.
- Every service function takes `HouseholdCtx` first; every query filters by `household_id`, including the joins on `credit_card_moves` and `credit_card_cycles`.
- Every plan and move change appends its event in the same database transaction; every transfer change appends its `transaction.*` event with origin `scheduler` in the same transaction as the `credit_card_cycles` update.
- `saldo-core` takes `today` as a parameter; it never reads the clock.

## 8. Rust concepts to explain

| Concept | Where | Analogy |
|---|---|---|
| Deriving `Ord` on a struct (`Cycle`) so fields compare in order | `Cycle` | A PHP `<=>` on `[$year, $month]` arrays; a Go `Less` method comparing year then month |
| An enum whose variants carry data (`SettlementAction::Create { amount, date }`) and a `match` that must handle every variant | `settlement_action`, the sync | A Laravel action class per case, but checked by the compiler; a Go interface with one struct per case and a type switch, without the missing-case risk |
| Implementing `FromStr` and `Display` to parse and print `YYYY-MM` | `Cycle` | `Carbon::createFromFormat` and `->format()`; Go's `Stringer` plus a `Parse` function |
| One database transaction per card and collecting failures instead of stopping at the first `?` | `sync_settlements` | A Laravel `DB::transaction` inside a `foreach` with a `try`/`catch` that records the error; Go's loop that appends to `[]error` and continues |

## 9. Acceptance criteria

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
SQLX_OFFLINE=true cargo test --workspace
cargo llvm-cov --workspace --summary-only     # >= 80%, saldo-core close to 100%
cd web && npm run lint && npm run typecheck && npm test && npx playwright test && npm run build && npm run size
```

- Spending counts withdrawals minus refunds and ignores transfers, including the settlement.
- Moving a split shifts exactly its amount to the next month's spending, available and settlement; moving it back restores both months.
- The settlement transfer appears with the first spending, follows changes, disappears when spending drops to zero or below, is frozen on its date or when reconciled, and is never touched again after a hand edit or delete.
- With payment day 31, January's month settles on 28 or 29 February, and every month whose following month is shorter settles on that month's last day.
- Months before the plan's start get no settlement and refuse moves.
- A budget changed from a month on leaves every earlier month's budget and available amount as it was.
- Another household's card, plan, moves and splits never appear (404).
- Running `card-settlements` twice in a row changes nothing the second time (no new events).
- Playwright: the flow in "Last".

## 10. Hand-off

In `docs/STATUS.md`, record:
- Phase 12 done with commit hashes.
- Any deviation from this brief, approved by the owner.
