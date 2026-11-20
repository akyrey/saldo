# Phase 11: bill history (linked transactions and a payment chart)

Requested by the owner on 2026-11-20. Before starting, add the endpoint below to `docs/ARCHITECTURE.md` section 6 (the Bills row) and the core functions to section 4, since that file is the contract.

## 1. Goal

Make a bill's detail page answer "what have I actually paid for this, and how has it changed?". When finished, the owner can open `/bills/<id>` and see:

- **A payment chart spanning the bill's whole duration**: every linked payment plotted by date and amount, the expected range (`amount_min` to `amount_max`) as a shaded band, each due date marked by its status (paid, skipped, open, overdue, scheduled), and a marker for today.
- **The linked transactions**: every split linked to the bill, newest first, with date, description, accounts, amount and the due date it paid, each linking to the transaction editor.
- **A short summary**: number of payments, total paid, average, lowest and highest, in the bill's currency.

## 2. Read first

- `CLAUDE.md`, `docs/STATUS.md`
- `docs/ARCHITECTURE.md` sections 4 (`bills::BillSchedule`), 5.2 (`splits.bill_id`), 5.3 (`bills`, `bill_periods`), 6 (conventions, the Bills and Transactions rows), 8.4 (how a period becomes paid)
- Existing code:
  - Core: `crates/core/src/bills.rs` (`BillSchedule::occurrences`, `matching_due_date`)
  - App: `crates/app/src/bills/{mod.rs,view.rs,periods.rs}` (`BillView`, `BillPeriodView`, `pay`, the automatic pairing in `periods.rs`), `crates/app/src/transactions/list.rs` (the `bill_id` filter)
  - Server: `crates/server/src/routes/bills.rs`
  - Web: `web/src/features/bills/{BillDetailPage.tsx,queries.ts,bill-form.ts,BillPages.test.tsx}`, `web/src/features/reports/{charts.tsx,report-model.ts}` (`plotValue`, the exact-decimal tooltip), `web/src/features/live/useLiveEvents.ts`

## 3. Prerequisites

Phases 0 to 5 are done (bills exist with periods and payments). Phase 10 may still be in progress: this phase touches different files except `crates/server/src/routes/bills.rs`'s router, the OpenAPI registration, `web/openapi.json`, `web/src/api/schema.d.ts` and `docs/STATUS.md`, where conflicts are additive.

## 4. Decisions

Proposed in this brief; the owner confirms them before work starts:

1. **One read-only endpoint on the server**, `GET /bills/{id}/history`, not a composition of `/transactions?bill_id=` and `/bills/{id}/periods` in the browser. Two reasons: a split can pay a period through `POST /bill-periods/{id}/pay` without carrying `bill_id`, so the "linked" set is a union only the server can build cheaply; and the summary is money arithmetic (average, totals), which stays in integer minor units on the server. Other clients (desktop, scripts) get it too.
2. **"Linked" means** a split whose `bill_id` is the bill, **or** a split that is some period's `paid_split_id`. Each split appears once.
3. **The span**:
   - starts at the earlier of `first_due_date` and the earliest linked payment (a payment made before the first due date still shows);
   - ends at `end_date` when set, otherwise at the later of today and the last created period's due date (the scheduler creates periods 60 days ahead);
   - is capped at today + 24 months, like the forecast, so a bill ending in 2040 does not plot decades of empty future.
4. **Due dates in the span after the last created period** are computed with `BillSchedule::occurrences` and reported with status `scheduled`. They are not written to `bill_periods`; the scheduler keeps owning that table.
5. **Overdue is derived**, not stored: an `open` period whose due date is before today (household time).
6. **Other currencies**: splits in a currency other than the bill's are listed (with their own currency) but neither plotted nor counted in the summary; the summary reports how many were left out.
7. **Unpaginated**: a bill's history is bounded by its span (a weekly bill over ten years is about 520 due dates). The endpoint returns everything; the web table shows 25 rows and a "Show more" button. If more than 2 000 due dates would be returned, the server keeps the latest 2 000 and sets `truncated: true`.
8. **No new tables, events or export changes**: this phase only reads.

## 5. Scope

In scope: the core functions, the app query, the endpoint with OpenAPI and TS schema regeneration, the bill detail page's chart, table and summary, live updates, and tests.

Out of scope:
- Editing or unlinking a payment from the bill page (the transaction editor already does this).
- Price-change detection or alerts ("this bill went up 12%").
- Exchange-rate conversion of other-currency payments.
- A history chart on the bills list page.
- Changing how periods are paired with payments.

## 6. Work packages

Order: core → app → server → web. One commit per package.

### Package 1: core (pure)

In `crates/core/src/bills.rs` (or a new `bills/history.rs` if the file passes 400 lines):

- `history_span(first_due: NaiveDate, end_date: Option<NaiveDate>, last_period_due: Option<NaiveDate>, earliest_payment: Option<NaiveDate>, today: NaiveDate) -> (NaiveDate, NaiveDate)`: decision 3. `from <= to` always holds.
- `DueStatus` enum: `Paid`, `Skipped`, `Open`, `Overdue`, `Scheduled`, with `fn due_status(stored: Option<PeriodStatus>, due: NaiveDate, today: NaiveDate) -> DueStatus` (`None` means "no period row", which is `Scheduled`).
- `PaymentStats { count, total, average, min, max }` and `fn payment_stats(amounts: &[i64]) -> Option<PaymentStats>` (`None` for an empty slice). `average` rounds half away from zero; use `checked_add` and return a typed error on overflow rather than panicking.

Tests: unit tests for every branch of `history_span` (with and without `end_date`, a payment before `first_due_date`, the 24-month cap, an `end_date` already past) and `due_status`; property tests that `from <= to`, that `min <= average <= max`, and that `count` equals the slice length.

### Package 2: app

`crates/app/src/bills/history.rs`, with `BillService::history(ctx, id, today) -> Result<BillHistoryView, AppError>`:

1. Load the bill (`find`, which already filters by household; 404 otherwise).
2. One query for the linked splits (decision 2), joined to their group and accounts:
   ```sql
   SELECT ... FROM splits s
   JOIN transaction_groups g ON g.id = s.group_id
   WHERE g.household_id = ?1
     AND (s.bill_id = ?2
          OR s.id IN (SELECT paid_split_id FROM bill_periods
                      WHERE household_id = ?1 AND bill_id = ?2 AND paid_split_id IS NOT NULL))
   ORDER BY s.date DESC, s.id
   ```
   Use `query_as!`; run `cargo sqlx prepare --workspace` and commit `.sqlx/`.
3. Load the bill's periods, compute the span with `history_span`, and add the `scheduled` due dates from `BillSchedule::occurrences` after the last period up to the span's end.
4. Pair each split with the period it paid (`paid_split_id`); a linked split that paid no period has `due_date: null`.
5. Compute the summary with `payment_stats` over the splits in the bill's currency.

The view (amounts as decimal strings, like every other view):

```json
{
  "bill_id": "…", "currency": "EUR",
  "range": { "from": "2024-01-15", "to": "2027-01-15" },
  "amount_min": "39.00", "amount_max": "45.00",
  "due_dates": [
    { "date": "2024-01-15", "status": "paid", "period_id": "…", "paid_split_id": "…" },
    { "date": "2026-12-15", "status": "scheduled", "period_id": null, "paid_split_id": null }
  ],
  "payments": [
    { "split_id": "…", "group_id": "…", "date": "2024-01-14", "description": "…",
      "amount": "42.50", "currency": "EUR",
      "source": { "id": "…", "name": "Checking" }, "destination": { "id": "…", "name": "Gym" },
      "due_date": "2024-01-15" }
  ],
  "summary": { "count": 23, "total": "977.50", "average": "42.50", "min": "39.00", "max": "45.00",
               "other_currency_count": 0 },
  "truncated": false
}
```

`summary` fields other than the counts are `null` when there are no payments in the bill's currency.

Tests (integration, against SQLite): a split linked by `bill_id` only, a split linked by `pay` only, a split linked both ways (listed once), a split from another household's bill never appears, an other-currency split (listed, not summed), an overdue period, scheduled dates beyond the last period, an `end_date` in the past, and a bill with no payments.

### Package 3: server

- `GET /bills/{id}/history` in `crates/server/src/routes/bills.rs`, tag `bills`, using the household's timezone for "today" as the bill scheduler does. 404 for an unknown or other-household bill.
- Register it in OpenAPI, regenerate `web/openapi.json` and the TS schema.
- Route test: 200 with the shape above, 404, and 401 without a session.

### Package 4: web

- **Query**: `historyQuery(id)` in `web/src/features/bills/queries.ts`, key `["bills", "history", id]`. Invalidate it on `transaction.*` and `bill.*` live events (check `useLiveEvents.ts`: if `bills` is already invalidated as a prefix on those events, nothing more is needed; otherwise add it), and after pay, skip and reopen on the page.
- **Model** (`web/src/features/bills/history-model.ts`, pure and unit-tested): turns the view into chart rows with `plotValue` (dates as epoch days for a numeric time axis, amounts as plotted numbers, the exact decimal strings kept for the tooltip), and splits the due dates into one series per status.
- **Chart**: `PaymentHistoryChart` in `web/src/features/reports/charts.tsx` (it stays the only module that imports Recharts; split it into `charts/` files if it passes 400 lines), loaded on the bill page with `React.lazy` so Recharts stays out of the initial route:
  - a numeric, time-scaled X axis over `range` with month or year ticks depending on the span;
  - a `ReferenceArea` for `amount_min` to `amount_max`, in a muted token colour;
  - the payments as a line with dots (`--chart-1`);
  - due-date markers along the bottom of the plot, one symbol per status, with a legend; overdue uses the destructive token;
  - a `ReferenceLine` at today;
  - the exact-decimal tooltip: date, description, amount, and the due date it paid;
  - an `aria-label` that summarises the chart; the table below is the accessible alternative.
- **Page** (`BillDetailPage.tsx`; move pieces into `BillHistory.tsx` and `LinkedTransactions.tsx` to keep files under 400 lines). Order on the page: header and description, a **Summary** row (count, total, average, lowest, highest; a note when other-currency payments were left out), the **Payment history** chart card, the **Linked transactions** card, then the existing **Periods** card and attachments.
  - Linked transactions table: date, description, from → to, amount (in the split's currency), due date paid (or "Not matched to a due date"), and a link to `/transactions/<group id>/edit`. 25 rows, then "Show more".
  - Empty states: no payments shows "No transactions linked to this bill yet" in both cards, and the chart still draws the band and the due dates.
- **Tests** (Vitest): the model (status series, plotted values, exact strings kept), and the page rendering the summary, the table, "Show more" and the empty state with a mocked API.

### Last: docs and Playwright

- Update `ARCHITECTURE.md` (section 4 for the core functions, section 6 for the endpoint).
- One Playwright flow: create a bill, create two withdrawals linked to it (one through `bill_id` in the editor, one through "Mark paid"), open the bill, and see both rows in the table, a chart with two payment points, and the summary total.

## 7. Contracts

- The endpoint follows section 6: error envelope, amounts as decimal strings, dates as `YYYY-MM-DD`.
- `BillService::history` takes `HouseholdCtx` first and every query filters by `household_id`, including the subquery on `bill_periods`.
- No events: the endpoint only reads.
- `due_dates[].status` is one of `paid`, `skipped`, `open`, `overdue`, `scheduled`.

## 8. Rust concepts to explain

| Concept | Where | Analogy |
|---|---|---|
| Comparing `Option`s and choosing a fallback (`Option::map_or`, `.into_iter().chain(...).min()`) | `history_span` | PHP's `min(array_filter([...]))` or `??`; in Go, a pointer checked for `nil` before comparing |
| An enum derived from data rather than stored (`DueStatus`) | `due_status` | A Laravel accessor that computes `status` from columns; a Go method returning a typed constant |
| `checked_add` and integer rounding half away from zero | `payment_stats` | PHP's `intdiv` with a manual remainder check; Go's `/`, which also truncates toward zero |
| A `HashMap` from split ID to period, built once and looked up per row | pairing payments with due dates | A PHP array keyed by ID (`array_column($periods, null, 'paid_split_id')`); a Go `map[string]Period` |

## 9. Acceptance criteria

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
SQLX_OFFLINE=true cargo test --workspace
cargo llvm-cov --workspace --summary-only     # >= 80%, saldo-core close to 100%
cd web && npm run lint && npm run typecheck && npm test && npx playwright test && npm run build && npm run size
```

- A split linked both by `bill_id` and by `pay` appears once; one linked only by `pay` appears.
- Another household's splits and bills never appear (404 for its bill).
- The span starts at an earlier payment when there is one, ends at a past `end_date`, and never goes beyond today + 24 months.
- An open period before today is `overdue`; due dates beyond the last created period are `scheduled` and no `bill_periods` row is written.
- The summary is hand-checked on a fixture with an odd total (average rounds half away from zero) and excludes an other-currency split, which still appears in `payments`.
- Recharts is not in the initial route chunk (`npm run size` stays under its budget).
- Playwright: the flow in "Last".

## 10. Hand-off

In `docs/STATUS.md`, record:
- Phase 11 done with commit hashes.
- The owner's confirmation of the decisions in section 4, and any change to them.
- The initial-route size after the chart is added.
- Any deviation from this brief, approved by the owner.
