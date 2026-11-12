# Phase 5: budgets, bills, piggy banks, recurring transactions, rules, reports

## 1. Goal

The planning features of Firefly III on top of the ledger, including their import and their screens. When finished, the owner's budgets, bills, piggy banks, recurring transactions and rules are imported from Firefly, recurring transactions are created automatically, rules run on new transactions, bills know which periods are paid, and the reports show net worth, cash flow and spending per category and budget for one member or the whole household.

Bill reminders and notifications are not part of this phase (Phase 6); this phase provides the bill and period data they need.

## 2. Read first

- `CLAUDE.md`, `docs/STATUS.md` (service naming from Phase 2, importer signature from Phase 3, UI conventions from Phase 4)
- `docs/ARCHITECTURE.md` sections 4.3, 5.3, 5.4, 6 (Phase 5 rows), 8.1 (event types), 8.3 (how a period becomes paid), 9.1 (rows 4 to 8 of the mapping table)
- Existing code: `crates/app/src/transactions/`, one entity importer in `crates/importer/src/`, one feature folder in `web/src/features/`

## 3. Prerequisites

Phase 2 is done (backend packages). Phase 3 is done for the import packages. Phase 4 is done for the UI packages. If 3 or 4 are still in progress, do the backend packages and leave the others, marked in `STATUS.md`.

## 4. Scope

In scope: the tables of ARCHITECTURE.md 5.3 and 5.4; pure logic in `core` (schedules, rule evaluation, budget maths); services, events and endpoints; the recurrence runner task; reports; Firefly import of these entities; UI screens.

Out of scope: reminders, notification channels, webhook delivery (Phase 6); exchange-rate conversion in reports (amounts in a non-primary currency are reported separately per currency).

## 5. Work packages

The five feature tracks (2 to 6) are independent after package 1 and can be given to parallel agents. Within a track the order is core → app → server → importer → web.

1. **Migrations**: one migration per feature with the tables from 5.3 and 5.4, copied exactly.
2. **Budgets**: core: spent per limit for a date range, auto-budget period generation (`reset`, `rollover`). App/server: CRUD for budgets and limits; the transaction service now checks `budget_id` belongs to the household and allows it only on Withdrawal splits; deleting a budget clears `splits.budget_id`. Importer: budgets and limits. Web: budget list with progress bars per period, limit editor, budget picker in the transaction editor.
3. **Bills**: core: `occurrences` for bill frequencies with `skip` and month-end clamping. App/server: CRUD; period materialisation up to 60 days ahead (as a service method the Phase 6 scheduler will call, and called on bill create/update); linking a split to a period (explicit endpoint, and automatically when a split is saved with `bill_id` and a date inside the period window); `skip` and `reopen`; deleting the paying split reopens the period; events `bill.paid`, `bill.period_reopened`. Importer: subscriptions, with `paid_dates[]` linked to imported splits. Web: bill list with next due date and paid state, period history, "mark paid" by picking a transaction.
4. **Piggy banks**: app/server: CRUD, linked accounts, events (add/remove money, optionally tied to a transfer split), saved amount = sum of events, cannot exceed the linked accounts' balance, `piggy_bank.target_reached`. Importer: piggy banks and their events. Web: list with progress, add/remove money dialog.
5. **Recurring transactions**: core: `occurrences` for recurrences (daily, weekly, monthly, nth weekday, yearly, `skip`, weekend policy, `repeat_until`, `max_occurrences`). App: `fire(recurrence, date)` creates a group from the template through `TransactionService` with `Origin::Scheduler`; the unique `(recurrence_id, recurrence_date)` makes it idempotent. Server: CRUD, `POST /recurrences/{id}/fire`, and a runner task that fires everything due up to today (household timezone) at startup and hourly. Importer: recurrences. Web: list, editor reusing the split rows of the transaction editor, upcoming dates preview.
6. **Rules**: core: `Trigger` and `Action` enums covering the Firefly keywords listed in the importer mapping, with `kind`/`value` (de)serialisation; `evaluate(rule, split_view) -> Vec<Action>` honouring `strict`, `negate`, `stop_processing`. App: the rule engine runs inside the transaction service on create/update (in the same DB transaction, changes recorded with `Origin::Rule`), `test` (dry run over existing transactions, returns matches) and `apply`. Server: CRUD for groups and rules. Importer: rule groups and rules; unsupported keywords import the rule as inactive and report it. Web: rule list by group with ordering, trigger/action editor, "test" preview.
7. **Reports**: SQL-backed services and endpoints for net worth over time, cash flow (income vs expenses per interval), by category, by tag, budget vs actual; all with `from`, `to`, `interval` (day, week, month, year) and `scope`. Transfers between the household's own accounts are excluded from income and expenses. Web: a dashboard and report pages with Recharts.
8. **Export update**: add the new tables to the JSON and CSV export and bump nothing (format version stays 1 while fields are only added).

## 6. Contracts

Honour: table definitions in 5.3 and 5.4; endpoints in section 6; event names in 8.1; mapping in 9.1.

Provide to Phase 6:

```rust
// saldo-core
pub fn reminder_due(period: &BillPeriod, bill: &Bill, today: NaiveDate) -> Option<ReminderKind>; // DueSoon | Overdue
// saldo-app
impl BillService {
    pub async fn materialise_periods(&self, ctx: &HouseholdCtx, today: NaiveDate) -> Result<(), AppError>;
    pub async fn open_periods(&self, ctx: &HouseholdCtx) -> Result<Vec<(Bill, BillPeriod)>, AppError>;
    pub async fn record_reminder(&self, ctx: &HouseholdCtx, period: BillPeriodId, kind: ReminderKind, today: NaiveDate) -> Result<(), AppError>; // updates last_reminded_on + appends bill.due_soon / bill.overdue
}
```

`reminder_due` and `record_reminder` are implemented and tested here, so Phase 6 only schedules and delivers.

## 7. Rust concepts to explain

| Concept | Where | Analogy |
|---|---|---|
| Enums with data, exhaustive `match` in depth | `Trigger`, `Action` | A Go interface with a type switch, but the compiler lists the cases you forgot; a PHP enum cannot hold per-case data |
| Iterators, lazy evaluation, `impl Iterator` | `occurrences` | A PHP generator / a Go iterator function (`iter.Seq`) |
| Closures and `Fn` traits | rule evaluation, report grouping | PHP arrow functions / Go func values; the three `Fn` traits say whether the closure only reads, mutates or consumes what it captured |
| Trait objects (`Box<dyn Trait>`) vs generics | only if the rule engine or reports need runtime polymorphism | `dyn Trait` is a Go interface value (dynamic dispatch); generics are resolved at compile time |
| Pattern matching on structs and tuples | schedule and rule code | PHP array destructuring with conditions built in |
| `chrono` dates and overflow-safe date maths | schedules | Carbon, without the mutable-by-default trap |

## 8. Acceptance criteria

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
SQLX_OFFLINE=true cargo test --workspace
cargo llvm-cov --workspace --summary-only     # >= 80%
cd web && npm run lint && npm run typecheck && npm test && npx playwright test
```

- Schedule tests cover: month-end clamping, leap years, `skip`, each weekend policy, nth weekday, `repeat_until` and `max_occurrences`.
- Firing a recurrence twice for the same date creates one group.
- A rule with `link_to_bill` marks the matching bill period paid when a transaction is created.
- Re-running the full Firefly import changes nothing; the report lists every rule imported as inactive.
- Report totals for a month equal the sums computed by an independent SQL query in the test.

## 9. Hand-off

In `docs/STATUS.md`: Phase 5 done (or which tracks are done) with commit hashes; the list of Firefly rule keywords that are not supported; the exact signatures provided to Phase 6 if they differ from section 6.
