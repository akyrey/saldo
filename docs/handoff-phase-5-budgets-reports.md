# Hand-off: Phase 5, work packages 2 (budgets) and 7 (reports)

Branch `phase-5-budgets-reports`, based on main at a653d05. Not pushed, not merged. `docs/STATUS.md` and `docs/ARCHITECTURE.md` are untouched; the lead folds this file into them.

## Commits

| Commit | What |
|---|---|
| 86d968b | feat(core): calendar periods and budget maths (spent, available, auto limits) |
| 8bf3425 | feat(app): budget and limit services with auto-budget limits; budgets on withdrawal splits only |
| ca74e7f | feat(server): budget and limit endpoints and the auto-budget task |
| 8a25683 | feat(importer): budgets, limits and budget links on withdrawals |
| fbf8e20 | docs(importer): list budgets in the `--only` help |
| 318534b | feat(app): net worth, cash flow, category, tag and budget reports |
| ad78356 | feat(server): report endpoints |
| 66ca617 | feat(web): budget picker in the transaction editor |
| a63d4af | feat(web): budget pages, a dashboard and report pages with Recharts |
| 99615ed | test(web): Playwright flow for budgets and reports |

No migration was added; the Phase 5 migrations on main were enough.

## What was built

- **core** (`period.rs`, `budget.rs`): `Period` (daily ... yearly; weeks from Monday, quarters from Jan/Apr/Jul/Oct, half years from Jan/Jul) with `start_of`, `range_of`, `next_start`, `ranges(from, to)` (clipped) and `count`; `DateRange`. `AutoBudgetType` (`reset`, `rollover`), `AutoBudget`, `Limit`, `Spend`, `spent(range, currency, spends)`, `available(limit, spent)`, `auto_limits(auto, last, spends, today)`. All re-exported at the crate root.
- **app** (`budgets/`): `BudgetService` (`app.budgets`) with `create|update|delete|get|list` and `create_limit|update_limit|delete_limit|get_limit|list_limits`, each with an `_in` variant; `create_auto_limits(ctx, today)` for the scheduler. Events `budget.*` and `budget_limit.*`. Deleting a budget clears `splits.budget_id` in the same transaction (limits cascade).
- **transactions**: `prepare.rs` loads the household's budgets into the `AccountLookup` (an unknown or foreign budget fails as `unknown_reference`) and then rejects a budget on a split whose kind is not withdrawal (`422`, field `splits[i].budget_id`, code `invalid`).
- **server**: `GET/POST /budgets` (`?active=`), `GET/PATCH/DELETE /budgets/{id}`, `GET/POST /budgets/{id}/limits` (`?from=&to=`, overlapping limits), `PATCH/DELETE /budget-limits/{id}`; `GET /reports/{net-worth,cash-flow,by-category,by-tag,budget-vs-actual}?from=&to=&interval=&scope=`. Background task `auto-budgets` (startup and hourly) in `crates/server/src/workers/budgets.rs`, registered in `workers::start`.
- **app** (`reports/`): `ReportService` (`app.reports`). SQL sums per day (and currency, and category/tag/budget); Rust folds the days into intervals (`series.rs`). Responses: `{ range: { from, to, interval }, currencies: [...] }`, primary currency first, one entry per currency, no conversion.
- **importer**: `entities/budgets.rs` (entity `budgets`, after `tags`, before `transactions`), refs `budget` and `budget_limit`; the transaction importer now sets `budget_id` from the `budget` ref. Re-running changes nothing (tested with a byte-for-byte snapshot including `budgets` and `budget_limits`).
- **web**: `features/budgets/` (list with month switcher and one progress bar per limit, quick-add form with automatic limit, detail page with settings, delete and the limit editor), budget picker in each split row, `features/reports/` (dashboard at `/reports`, `/reports/$report` with from/to/interval in the URL, chart plus table per currency). Nav gained Budgets and Reports; the phone tab bar now sizes its columns to the number of items.

## Decisions

- Budget limits of one budget in one currency may not overlap (`409`). This also keeps auto-generated limits from doubling a hand-made one.
- Auto-budget limits: created on budget create/update (not for `Origin::Import`) and by the hourly task, from the period after the budget's latest limit up to the period containing the household's today; with no limit yet, only the current period. `rollover` adds the previous limit's unspent amount (same currency only); overspending is not carried over (Firefly's `adjusted` type is not supported).
- Spent = withdrawals booked on the budget, dated inside the limit, in the limit's currency or with a foreign amount in it.
- Limit IDs are passed as `&str` (no `BudgetLimitId` newtype in core, to avoid touching `ids.rs` while other tracks edit core).
- `GET /budgets/{id}/limits` is paginated in memory (one budget's limits are few) and newest first.
- Reports: income = deposits, expenses = withdrawals, in the split's own currency (foreign amounts ignored). Transfers, opening balances and reconciliations are neither, so transfers between own accounts are excluded in every scope. Scope filters splits by account owner with the transaction list's rule (either account owned by that user); net worth filters accounts by owner (joint accounts only count for the household). Net worth sums asset and liability accounts with `include_net_worth`, without virtual balances. Budget vs actual counts a limit in the interval its start date falls in; limits are not scoped, spending is; spending without a budget is an item with `budget_id: null`. By tag counts a split once per tag; untagged/uncategorised is `id: null`.
- `interval` defaults to `month`; at most 1000 intervals per report (`422` otherwise); `from`/`to` are required.
- Importer: a Firefly budget link on a split that is not a withdrawal in Saldo (its destination is not an expense account, e.g. Firefly's "withdrawal" into a liability) is dropped and reported as a note; a link to a budget that was not imported is a note too. `auto_budget_type` `none`/null → no automatic limit; anything other than `reset`/`rollover` → imported without it, with a note.
- Web: Recharts 3.10 (`npx npm@11 install recharts`), only imported from `features/reports/charts.tsx`, reached through lazy routes; initial route 132 kB gzipped. Charts plot `Number(minor) / 10^places` (drawing only); tooltips, tables and totals show the server's exact strings. Series colours `--chart-1`/`--chart-2` in `index.css` (a blue/orange pair validated for colour-vision deficiencies in light and dark). Every chart has a table view. Query keys `["budgets", ...]` and `["reports", ...]`; saving a transaction marks both stale.

## Deviations from ARCHITECTURE.md

- None in endpoints or tables. Additions within the documented endpoints: `GET /budgets?active=`, `GET /budgets/{id}/limits?from=&to=`, and the report response shapes (ARCHITECTURE.md does not define them). The lead may want to add a line to section 6 describing the report rules above.

## Test results

- `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `SQLX_OFFLINE=true cargo test --workspace`: pass, 272 tests. `cargo llvm-cov --workspace`: 95.70% lines (core budget/period ~99%, app budgets 92–100%, reports 95–100%, importer budgets 84%).
- Report acceptance: `crates/app/tests/reports.rs` compares cash flow, category and net worth totals for a month with independent SQL queries.
- Web: `npm run lint`, `npm run typecheck`, `npm test` (83 tests), `npm run build`, `npm run size` (132 kB gzipped initial route), Playwright 7/7 including the new `e2e/planning.spec.ts`.

## Merge notes

Shared files touched (appended only): `crates/core/src/lib.rs`, `crates/app/src/lib.rs`, `crates/server/src/routes/mod.rs`, `crates/server/src/workers/mod.rs` (`start` now spawns the auto-budget task), `crates/importer/src/engine.rs` (`ENTITIES` is now `[&str; 7]`), `crates/importer/tests/import.rs` (`mount_budgets` called from the three mock servers; `budgets` and `budget_limits` added to the snapshot), `web/src/router.tsx`, `web/src/features/shell/AppLayout.tsx`, `web/src/app.test.tsx` (fake `GET /budgets`), `web/openapi.json` and `web/src/api/schema.d.ts` (regenerate after merging), `.sqlx/`, `web/package.json` and lock file.

## Left undone

- Budgets are not in the export (the lead adds them after merging, as agreed).
- Report pages offer no CSV download; reports do not convert currencies (out of scope).
