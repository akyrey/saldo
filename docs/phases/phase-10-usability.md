# Phase 10: dark mode, saved views, command palette, alerts, new reports, goals

Requested by the owner on 2026-11-20. Before starting a track, add its tables, endpoints and events to `docs/ARCHITECTURE.md` (sections 5, 6, 8 and 10), since that file is the contract. Each track's design below is what goes there.

## 1. Goal

Make Saldo quicker to use every day and more useful for looking ahead. When finished, the owner can:

- **Pick a theme**: system, light or dark, remembered per device, with no flash of the wrong theme on load.
- **Filter transactions further and save the result as a view**: amount range, "no category", "no budget", budget, bill and reconciled state, plus named views shown as chips above the list.
- **Drive the app from the keyboard**: ⌘K / Ctrl+K opens a command palette (pages, actions, templates, saved views, accounts); single-key shortcuts for common actions; `?` lists them.
- **Get more useful notifications**: budget at 80% and 100%, forecast going below zero, piggy bank target reached, large transactions, recurring transactions created, net worth goal reached, and a weekly digest. Each channel chooses which kinds it receives.
- **Read four new reports**: year over year, savings rate, spending by counterparty, and average monthly spend per category.
- **Set net worth goals and see "safe to spend this month"** on the dashboard.

## 2. Read first

- `CLAUDE.md`, `docs/STATUS.md`
- `docs/ARCHITECTURE.md` sections 4.3, 5.1 (`households`), 5.2 (`accounts.asset_role`, `include_net_worth`), 5.3 (budgets, bills, piggy banks), 5.5 (`notification_channels`, `notifications`), 6, 8.1, 8.3, 8.4, 8.5, 10, 12
- Existing code:
  - Dark mode: `web/src/index.css` (tokens; dark values sit under `@media (prefers-color-scheme: dark)` while `@custom-variant dark` expects a `.dark` class), `web/index.html` (`theme-color`), `web/src/features/webhooks/DeliveryLog.tsx` (the only hard-coded colours)
  - Filters: `crates/app/src/transactions/{mod.rs,repo.rs}` (`TransactionFilter`, `list_ids`), `crates/server/src/routes/transactions.rs`, `web/src/features/transactions/{search.ts,TransactionFilters.tsx,TransactionsPage.tsx}`
  - Palette: `web/src/features/shell/{nav.ts,AppLayout.tsx,MoreMenu.tsx}`, `web/src/router.tsx`, `web/src/features/templates/`
  - Alerts: `crates/app/src/notifications/{message.rs,outbox.rs,channels.rs}`, `crates/server/src/workers/{reminders.rs,notifier.rs,mod.rs}`, `crates/app/src/bills/` (`run_reminders`), `crates/app/src/piggy_banks/`, `crates/app/src/budgets/`
  - Reports, goals, safe to spend: `crates/app/src/reports/` (all of it, especially `flows.rs`, `net_worth.rs`, `forecast_sources.rs`), `crates/core/src/{budget.rs,carry.rs,forecast.rs,piggy_bank.rs}`, `web/src/features/reports/` (`DashboardPage.tsx`, `report-model.ts`, `charts.tsx`)

## 3. Prerequisites

Phases 0 to 8 are done. Phase 9 Track E (statement import) does not block this phase; the two touch different files except `router.tsx`, the OpenAPI registration and `STATUS.md`, where conflicts are additive.

## 4. Decisions

Proposed in this brief and confirmed by the owner on 2026-11-20 (decision 10 was added at the owner's request on the same day):

1. **Theme** is stored per device in `localStorage` (`saldo.theme`: `system`, `light`, `dark`), not on the server. A phone and a laptop may want different themes, and it needs no API.
2. **Saved views are shared by the household**, like templates (everyone sees and edits everything). They store the list's URL filters as a JSON object; the web app validates them on load with `parseTransactionSearch`, as it does any hand-edited URL.
3. **Command palette** uses `cmdk` (the library behind shadcn/ui's Command component), lazy-loaded so the initial route stays under its 300 kB budget. Shortcuts use a small hook of our own, with no hotkey library.
4. **Alert thresholds**:
   - Budgets alert at 80% and at 100% of `amount + carried`, once per limit and threshold.
   - The forecast alert looks 30 days ahead, per asset account in net worth.
   - The large-transaction threshold is one household setting in the primary currency (empty means off).
   - The digest goes out on Mondays at 08:00 household time and covers the previous Monday to Sunday.
   - These are constants, except the large-transaction amount.
5. **An alert fires once per crossing.** An `alert_states` row records that a condition holds. The alert fires when the row is inserted and the row is deleted when the condition stops, so a budget that drops back below 80% after a refund and climbs again alerts again.
6. **Channels choose event types.** A new `notification_channels.event_types` column defaults to the two bill types, so existing channels behave exactly as today.
7. **Year over year and savings rate** are computed on the server, because savings rate is a ratio of money. Year over year is a new endpoint, not a client-side reshaping of cash flow, so other clients get it too.
8. **Safe to spend** is income-based (section 6, Track F): this month's income (received and planned) minus everything already spent or committed this month. It is computed in the primary currency only, and the response lists every part so the number can be checked.
9. **Net worth goals** record the net worth on the day they are created (`start_amount`), so progress runs from there to the target.
10. **Recurring transactions notify when created.** The existing `recurrence.fired` event notifies when `created` is `true` and the origin is `scheduler`. A manual "Fire now" from the web is not notified, since the person who clicked it already knows. A catch-up after downtime sends one notification per created date.

## 5. Scope

In scope: the work packages below, including migrations, `core` logic, services, events, endpoints, OpenAPI and TS schema regeneration, web screens, export of new tables, and tests.

Out of scope:
- Bulk editing of transactions.
- Localised number and date formats.
- Exchange-rate conversion: reports, goals and safe to spend stay per currency or primary-only.
- A monthly digest.
- Per-account or per-user alert thresholds.
- Theme sync across devices.
- Customisable shortcuts.

## 6. Work packages

Tracks A to F follow the owner's order. A, B, D and E are independent. C lists the saved views from B (until B is merged, leave that section of the palette empty). F adds `net_worth_goal.reached` to D's alert scheduler, so F comes after D. Within a track the order is core → app → server → web.

### Track A: dark mode (web only)

- **Tokens**: move the dark values in `web/src/index.css` from `@media (prefers-color-scheme: dark)` to `:root.dark`, so the `dark:` variant and the tokens follow the same switch.
- **No flash of the wrong theme**: add a small inline script at the top of `<head>` in `index.html`. It reads `saldo.theme` inside `try/catch`, resolves `system` with `matchMedia("(prefers-color-scheme: dark)")`, and sets `document.documentElement.classList` and `style.colorScheme` before the first paint. Keep it under 20 lines, with no imports.
- **`web/src/features/theme/`**:
  - `theme.ts` (pure): `resolveTheme(choice, prefersDark)` and `readChoice` / `writeChoice`, which tolerate a missing or throwing `localStorage`.
  - `ThemeProvider.tsx`: listens to `matchMedia` changes while the choice is `system`, and updates `<meta name="theme-color">` (teal in light mode, the dark background in dark mode).
  - `useTheme()`, which returns `{ choice, resolved, setChoice }`.
- **UI**: a three-way choice (System / Light / Dark) in the More menu. Command palette entries come in Track C.
- **Desktop**: when running in Tauri, call `getCurrentWindow().setTheme()` with the resolved theme (`null` for system), so the macOS title bar matches.
- **Audit**:
  - Check every page in both themes: charts (Recharts axis and tooltip colours must use the tokens), the calendar, badges, `DeliveryLog`'s status colours, and focus rings.
  - Replace any fixed colour with a token. Keep the validated chart palette pairs as they are.

### Track B: more filters and saved views

- **Filters** (server: `TransactionFilter`, `repo::ListFilter`, `list_ids` and its count query, and the route's query struct):
  - `amount_min`, `amount_max`: decimal strings, compared against the split's absolute amount and parsed in `currency` (default: the household's primary currency, 422 on a bad decimal or `amount_min > amount_max`). When either is set, only splits in that currency match.
  - `category_id=none` and `budget_id=none`: splits without one. Keep the UUID form; map `none` to a separate boolean parameter in the SQL (`?n = 1 AND s.category_id IS NULL`), not to a magic string compared in SQL.
  - `has_attachments` (bool): the split or its group has an attachment.
  - The web form gains budget, bill, `reconciled` (Any / Yes / No), amount range, "No category" and "No budget". The API already supports budget, bill and `reconciled`.
  - Both SQL copies (ids and count) must stay identical. Add a test that runs every filter through both and checks `total` against the number of IDs on one large page.
- **Saved views**:
  - Schema (new migration):
    ```sql
    CREATE TABLE saved_views (
        id            TEXT PRIMARY KEY,
        household_id  TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
        name          TEXT NOT NULL,
        page          TEXT NOT NULL CHECK (page IN ('transactions')),
        filters       TEXT NOT NULL CHECK (json_valid(filters)),
        sort_order    INTEGER NOT NULL DEFAULT 0,
        created_at    TEXT NOT NULL,
        updated_at    TEXT NOT NULL,
        UNIQUE (household_id, page, name)
    ) STRICT;
    ```
    `page` leaves room for other lists later; only `transactions` is accepted now.
  - **App** (`SavedViewService`): CRUD. `filters` must be a JSON object of string, number or boolean values, at most 30 keys and 2 KB (422 otherwise). The server does not interpret the keys. Events: `saved_view.created`, `saved_view.updated`, `saved_view.deleted`.
  - **Server**: `GET/POST /saved-views` (`?page=transactions`, ordered by `sort_order`, then name, unpaginated), `PATCH/DELETE /saved-views/{id}`. A duplicate name is a 409.
  - **Web**:
    - Chips above the transaction list: a fixed **Uncategorised** chip (`category_id=none`, not stored), then the saved views.
    - The active chip is highlighted when the URL filters equal its filters, ignoring `page`.
    - "Save view…" stores the current filters. A chip's menu offers Rename, Update to current filters and Delete.
    - Add `saved_view` to the live-event keys.
  - **Export**: add `saved_views` (format version stays 1).

### Track C: keyboard shortcuts and command palette (web only)

- **`web/src/features/commands/`**:
  - `shortcuts.ts` (pure): the shortcut table (key sequence, label, group, action ID) and `matchSequence(buffer, event)`. Sequences like `g t` reset after 1 s.
  - `useShortcuts()`: one `keydown` listener on `window`. It ignores events with Ctrl, Alt or Meta held (except ⌘K / Ctrl+K), events from `input`, `textarea`, `select` and `[contenteditable]`, and events while a dialog is open.
  - `CommandPalette.tsx`: lazy-loaded `cmdk` inside the existing Radix dialog.
  - `ShortcutHelp.tsx`: the `?` overlay, generated from the same table.
- **Shortcuts**:

  | Keys | Action |
  |---|---|
  | ⌘K / Ctrl+K | Open the palette |
  | `/` | Focus the transaction search (go to the list first if elsewhere) |
  | `n` | New transaction |
  | `g d`, `g t`, `g a`, `g b`, `g r`, `g f` | Go to dashboard, transactions, accounts, budgets, reports, forecast |
  | `?` | Show shortcuts |
  | ⌘Enter / Ctrl+Enter (in an editor) | Save |
  | Esc (in an editor) | Cancel, with the existing unsaved-changes guard |

- **Palette sections**:
  - Pages: every `PRIMARY_NAV`, `MORE_NAV` and `OWNER_NAV` entry plus the dashboard, forecast and each report.
  - Actions: new transaction, import statement once Track E of Phase 9 exists, and theme (from Track A).
  - Templates: "New from template: <name>".
  - Saved views (Track B).
  - Accounts: typing two or more characters queries `GET /accounts?q=`, debounced 200 ms.
  - Last entry, always: "Search transactions for “…”", which opens `/transactions?q=…`.
- **Mobile**: a search icon in the header opens the palette.
- **Desktop**: check that ⌘K and the single-key shortcuts do not clash with Tauri menu accelerators.
- Run `npm run size` after adding `cmdk`. The palette chunk must not be in the initial route.

### Track D: more useful notifications

- **Schema** (one migration):
  - `notification_channels.event_types TEXT NOT NULL DEFAULT '["bill.due_soon","bill.overdue"]' CHECK (json_valid(event_types))`.
  - `households.large_transaction_minor INTEGER CHECK (large_transaction_minor > 0)`: nullable, in the primary currency.
  - The `alert_states` table:
    ```sql
    -- One row per alert condition that currently holds. Inserted together with
    -- the alert's event; deleted when the condition stops. Bookkeeping: no events.
    CREATE TABLE alert_states (
        household_id  TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
        key           TEXT NOT NULL,   -- e.g. 'budget:<limit id>:80'
        since         TEXT NOT NULL,
        PRIMARY KEY (household_id, key)
    ) STRICT;
    ```
- **Notifying event types**. All are new except the last two, which already exist and only need messages:

  | Type | Emitted by | Key in `alert_states` |
  |---|---|---|
  | `budget.threshold_reached` | alert scheduler | `budget:<limit id>:<80\|100>` |
  | `forecast.below_zero` | alert scheduler | `forecast:<account id>` |
  | `net_worth_goal.reached` | alert scheduler (Track F) | `goal:<goal id>` |
  | `digest.weekly` | alert scheduler | `digest:<ISO year>-W<week>` (never deleted; prune keys older than 8 weeks) |
  | `transaction.large` | `TransactionService::create` / `replace`, one per new split at or above the threshold in the primary currency, for origins `web`, `api` and `rule` only (not `import` or `recurrence`) | none |
  | `piggy_bank.target_reached` (exists since Phase 5) | `piggy_banks/events.rs`, already in the transaction that reaches the target | none |
  | `recurrence.fired` (exists since Phase 5) | `RecurrenceService::fire_in`; notifies only when `created` is `true` and the event's origin is `scheduler` | none |

  Payloads follow 8.1: the entity as the API returns it, plus the figures the message needs (for budgets: the limit view and the threshold; for the forecast: the account, the lowest balance and its date; for the digest: the week's income and expenses, the top three categories, the bills due in the next 7 days, and the budgets over 80%).
- **Core** (pure):
  - `alerts::budget_thresholds(available_total, spent) -> Vec<u8>`: the thresholds reached, using integer maths (`spent * 100 >= total * 80`).
  - `alerts::diff(held: &HashSet<String>, now: &HashSet<String>) -> (to_insert, to_delete)`.
  - `alerts::digest_due(now_local, last_key) -> Option<String>`.
- **App**: `AlertService::run(ctx, today)`, in one transaction per household:
  1. Compute the current conditions. For budgets: active limits containing today, using the existing `carried`/`available`. For the forecast: the 30-day forecast per account, reusing `reports::forecast`.
  2. Diff them against `alert_states`.
  3. Insert the new rows and append each one's event; delete the rows that stopped.
  4. Emit the digest when it is due.

  It returns the number of events appended.
- **Server**: a new `alert-scheduler` task (every 15 minutes and after writes, like `bill-scheduler`) that wakes the notifier when it appended events. Add it to 8.5.
- **Notifier**:
  - `NOTIFYING_EVENTS` grows to all the types above. The notifier reads the event's origin to filter `recurrence.fired`; `Message::for_event` returns `None` for a firing that created nothing.
  - The recurrence message: title "Recurring transaction created: <group title or first split description>", body "<amount> from <source account> to <destination account> on <date>." (for several splits, the total and "N splits"), link `/transactions/<group id>/edit`. A channel receives an event only when the type is in its `event_types`.
  - `Message::for_event` becomes a `match` on the type that calls one small function per type. Split `message.rs` into `message/{mod,bills,budgets,forecast,goals,piggy_banks,recurrences,transactions,digest}.rs` to keep files under 400 lines.
  - Dedupe keys: `<type>:<alert key>` for scheduler alerts, `<type>:<split id>` for large transactions, `<type>:<piggy bank id>:<event id>` for piggy banks, `<type>:<recurrence id>:<date>` for recurrences.
- **API**:
  - `POST/PATCH /notification-channels` accept `event_types` (each must be a notifying type, 422 otherwise), and the channel view returns it.
  - `GET /notification-types` lists `{ type, label, description }` for the settings screen.
  - `PATCH /household` takes `large_transaction_amount` (a decimal string or `null`).
- **Web**:
  - The notification settings page gets a checkbox list per channel.
  - The household settings (owner) get the large-transaction amount.
  - Desktop native notifications reuse the same messages.

### Track E: new reports

All four follow section 6's report conventions (`from`, `to`, `scope`, one entry per currency, primary first, amounts as decimal strings) and count income and expenses the way `cash_flow` does.

- **Savings rate**: add `savings` (`income − expenses`) and `savings_rate_bp` (basis points, `null` when income is zero) to each `cash-flow` interval and to its totals. In core: `savings_rate_bp(income, expenses) -> Option<i64>`, rounded half away from zero, which may be negative. This only adds fields, so the endpoint stays the same. Web: a savings rate line under the cash-flow chart and a "Savings rate" report entry that reuses the cash-flow query.
- **Year over year**: `GET /reports/year-over-year?years=&metric=&category_id=&scope=`.
  - `years` is 2 to 5, default 3, ending with the current year.
  - `metric` is `expenses` (default), `income` or `savings`.
  - With `category_id`, only that category (`none` for uncategorised).
  - Response: per currency, `years: [{ year, months: [12 amounts], total }]`, with months still in the future set to `null`, plus `change` per month and in total versus the previous year (an amount, and `change_bp` when the previous value is not zero).
  - Web: lines per year on a January-to-December axis, and a table.
- **Spending by counterparty**: `GET /reports/by-counterparty` with the same parameters as `by-category`. Withdrawals group by destination account (expense accounts) and deposits by source account (revenue accounts). Add `By::Counterparty` next to `Category` and `Tag` in `flows.rs` and reuse the breakdown view. Web: a report page like by-category, with each name linking to `/accounts/<id>`.
- **Average monthly spend per category**: `GET /reports/category-averages?months=&scope=`.
  - `months` is 3 to 24, default 12: the last complete months before the current one (household time).
  - Per currency and category (including "no category"), the response has `average`, `min` and `max` (monthly expenses, with months without spending counting as zero), `this_month` (so far), and `this_month_vs_average_bp`.
  - In core: `monthly_average(totals: &[i64]) -> i64`, rounded half away from zero.
  - Web: a table sorted by average, with a bar for this month against the average. The dashboard's "Spending this month by category" card shows the average next to each category.

### Track F: net worth goals and safe to spend

- **Goals schema** (new migration):
  ```sql
  CREATE TABLE net_worth_goals (
      id                   TEXT PRIMARY KEY,
      household_id         TEXT NOT NULL REFERENCES households (id) ON DELETE CASCADE,
      name                 TEXT NOT NULL,
      owner_user_id        TEXT REFERENCES users (id) ON DELETE CASCADE, -- NULL: whole household
      target_amount_minor  INTEGER NOT NULL,
      start_amount_minor   INTEGER NOT NULL,   -- net worth when the goal was created
      currency_code        TEXT NOT NULL REFERENCES currencies (code),
      target_date          TEXT,
      reached_on           TEXT,
      active               INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
      notes                TEXT,
      created_at           TEXT NOT NULL,
      updated_at           TEXT NOT NULL,
      UNIQUE (household_id, name)
  ) STRICT;
  ```
- **Core**: `goal::progress(start, current, target, today, target_date, avg_monthly_change) -> GoalProgress` returns:
  - `remaining`
  - `progress_bp` (0 to 10 000, clamped)
  - `required_monthly` (`remaining / months left`, rounded up; `None` without a target date or when it has passed)
  - `projected_date` (`today + ceil(remaining / avg)` months; `None` when the average is not positive)
  - `on_track`

  Property tests cover a target below the start (a goal of reducing debt to zero) as well as above it.
- **App** (`GoalService`):
  - CRUD. Create reads the current net worth for the goal's scope and currency into `start_amount_minor`.
  - The view adds `current` and `progress`, with `avg_monthly_change` taken from the net worth report over the last 12 complete months.
  - Events: `net_worth_goal.created`, `net_worth_goal.updated`, `net_worth_goal.deleted`.
  - The alert scheduler (Track D) sets `reached_on` and appends `net_worth_goal.reached` when `current` reaches the target. "Reaches" means `>=` when the target is above the start and `<=` when it is below.
- **Safe to spend**: `GET /reports/safe-to-spend?scope=`, for the current calendar month in household time and the primary currency only. In core, `safe_to_spend(parts) -> SafeToSpend`; the app gathers the parts:

  | Part | What it counts |
  |---|---|
  | `income_received` | deposits dated from the 1st of the month to today |
  | `income_planned` | forecast items after today and before the month ends that are deposits (recurrences, future-dated splits) |
  | `spent` | withdrawals dated from the 1st of the month to today |
  | `bills` | open and overdue bill periods due by the end of the month, at `amount_max`, using the forecast's rules (a bill covered by a recurrence counts once, as the recurrence) |
  | `planned` | the remaining forecast withdrawals after today and before the month ends |
  | `budgets` | per active budget, `max(available, 0)` of the limits overlapping the rest of the month, prorated by days when a limit runs past the month end; auto-budget periods in the month that do not exist yet count their base amount; a planned withdrawal or bill carrying that budget reduces its reserve (to at least zero), so it is not counted twice |
  | `piggy_banks` | per active piggy bank with a target amount and a future target date: `ceil((target − saved) / months left including this one)` minus what was added this month, at least zero |
  | `savings_transfers` | planned transfers after today and before the month ends into asset accounts with `asset_role = 'savings'` |

  The result: `safe_to_spend = income_received + income_planned − spent − bills − planned − budgets − piggy_banks − savings_transfers`, plus `per_day = safe_to_spend / days left including today` (rounded toward negative infinity; today always counts, so the divisor is at least 1). The response returns every part and its items (`{ source, source_id, title, amount }`), so the dashboard can explain the number.
- **Server**: `GET/POST /net-worth-goals` (`?active=`), `GET/PATCH/DELETE /net-worth-goals/{id}`, and `GET /reports/safe-to-spend`.
- **Web**:
  - The dashboard gets a **Safe to spend** card at the top: the figure, per day, the days left, and an expandable breakdown, in the destructive colour when negative.
  - It also gets a **Goals** card: a progress bar per active goal, the required monthly amount, the projected date and an on-track badge.
  - Goals are managed from the net worth report page (create, edit, archive), and the net worth chart draws each goal's target as a reference line.
  - Add `net_worth_goal` to the live-event keys.
- **Export**: add `net_worth_goals` (format version stays 1). Leave out `alert_states`, as bookkeeping.

### Last: docs and Playwright

- Update `ARCHITECTURE.md` 5, 6, 8.1, 8.4, 8.5 and 10, and `docs/WEBHOOKS.md` (the new event types).
- Playwright flows:
  1. Switch to dark mode, reload, and the page is still dark with no light flash (check the `dark` class before the first paint via `page.addInitScript` timing).
  2. Filter by amount range, save as a view, clear the filters, and click the chip to restore them.
  3. ⌘K, type "budg", Enter lands on budgets; `n` opens the new-transaction editor.
  4. Create a net worth goal and see it on the dashboard with the safe-to-spend card.

## 7. Contracts

Honour the existing conventions in section 6:
- The error envelope.
- Amounts as decimal strings; ratios as integer basis points (`_bp`), never floats.
- `scope` on every list and report.
- Every new service function takes `HouseholdCtx` first and filters by `household_id`.

New event types:
- `saved_view.created`, `saved_view.updated`, `saved_view.deleted`
- `budget.threshold_reached`, `forecast.below_zero`, `digest.weekly`
- `transaction.large`
- `net_worth_goal.created`, `net_worth_goal.updated`, `net_worth_goal.deleted`, `net_worth_goal.reached`

`alert_states` changes append no events, like the other bookkeeping in 8.1. `piggy_bank.target_reached` and `recurrence.fired` already exist; only their notifications are new.

## 8. Rust concepts to explain

| Concept | Where | Analogy |
|---|---|---|
| `HashSet` and set difference | `alerts::diff` | PHP's `array_diff` on keys; in Go, two `map[string]struct{}` compared in loops |
| Integer rounding without floats (`div_euclid`, `checked_mul`, rounding half away from zero) | savings rate, averages, goal maths | `intdiv` plus a manual remainder check in PHP; Go's `/` truncates toward zero just like Rust's |
| Validating untyped JSON with `serde_json::Value` | saved view filters, channel `event_types` | Laravel's `array` validation rule on a JSON column; Go's `map[string]any` with type switches |
| `chrono`'s ISO week (`IsoWeek`) and month arithmetic | digest key, goal months left | Carbon's `isoWeek()` / `addMonthsNoOverflow()` |
| An enum with a variant per report grouping (`By::Counterparty`) | `flows.rs` | A Go `type By int` with `iota` constants, where the compiler forces every `match` to handle the new one |

## 9. Acceptance criteria

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
SQLX_OFFLINE=true cargo test --workspace
cargo llvm-cov --workspace --summary-only     # >= 80%, saldo-core close to 100%
cd web && npm run lint && npm run typecheck && npm test && npx playwright test && npm run build && npm run size
```

- Track A: each theme choice resolves correctly with system light and system dark; a throwing `localStorage` falls back to `system`; no page has a hard-coded colour outside `index.css`.
- Track B:
  - Every filter, alone and combined, returns the same `total` as the number of IDs.
  - `amount_min > amount_max` is a 422 on `amount_min`.
  - `category_id=none` returns only splits without a category.
  - Saved view filters over 2 KB, or with a nested object, are a 422.
  - A duplicate view name is a 409.
- Track C:
  - Shortcuts do not fire while typing in an input.
  - `g t` works and `g` then a 2 s pause then `t` does not.
  - The palette is in its own chunk and the initial route stays under 300 kB.
- Track D:
  - A budget crossing 80%, then 100%, then dropping below 80% after a refund and crossing it again emits exactly three `budget.threshold_reached` events.
  - Running the scheduler twice in a row appends nothing the second time.
  - An existing channel without `event_types` set still receives bill reminders and nothing else.
  - The digest fires once per week, even across a restart.
  - An imported large split emits no `transaction.large`.
  - A scheduled firing of a recurrence notifies a channel subscribed to `recurrence.fired` once; firing the same date again (no new group) and a manual "Fire now" notify nothing.
- Track E:
  - Hand-computed fixtures for savings rate (including zero income and negative savings), year over year (with a partial current year), counterparties (a split to an expense account and a deposit from a revenue account), and averages (a month with no spending counts as zero).
- Track F:
  - `goal::progress` property tests: `progress_bp` is always within 0 to 10 000, and `required_monthly × months left ≥ remaining`.
  - A safe-to-spend fixture with income, a paid and an unpaid bill, a recurrence covering a bill, a budget with a planned withdrawal in it, a piggy bank with a target date and a savings transfer gives exactly the hand-computed parts.
  - Reaching a goal emits `net_worth_goal.reached` once.
- Playwright: the four flows in "Last".

## 10. Hand-off

In `docs/STATUS.md`, record:
- Phase 10 done (or which tracks are done) with commit hashes.
- The owner's confirmation of the decisions in section 4.
- The initial-route size after Track C.
- Any deviation from this brief, approved by the owner.
