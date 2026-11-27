# Changelog

All notable changes to Saldo. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [Semantic Versioning](https://semver.org/). Each version is the `vX.Y.Z` tag that releases the desktop app and the server image.

## [Unreleased]

## [0.14.0] - 2026-11-28

### Added

- Statement import: upload a bank or card statement (xlsx or CSV), review every line with its proposed match, and commit it. Importing the same or an overlapping file again creates nothing new, and a whole import can be undone. Presets for Widiba, Mediobanca and Satispay; other banks get a profile of their own, with a preview. The review checks that the statement adds up (opening balance plus lines equals closing balance) and offers "Reconcile now". Satispay meal vouchers become a split from their own account.
- Sources: upload Amazon order and PayPal exports, and newer ones whenever you like; only new rows are added. Bank lines they explain get the order's products or the merchant as description (the bank's text stays in the notes), and an Amazon charge can be split by item.
- Enrich from sources: an account page lists existing transactions that stored Amazon and PayPal rows explain, and accepting one books the better description.
- Exports: the JSON export and the CSV zip include statement profiles and the stored Amazon and PayPal rows, and snapshots include the stored export files.

## [0.13.0] - 2026-11-27

### Added

- Snapshots: with `SALDO_SNAPSHOT_DOWNLOAD=true`, the household owner can download the whole instance (database and attachment files) as a zip at `GET /api/v1/export/snapshot`, and `saldo restore <zip>` unpacks it into a fresh Saldo. The copy loses its sessions, API tokens, webhooks and notification channels, so it never reaches the original's endpoints. Only offered while the instance has a single household.

### Changed

- Transactions: clicking the selected saved view (or "Uncategorised") again deselects it and clears the filters.

### Fixed

- Transactions: an amount filter or a numbers-only search typed into the address bar (for example `?amount_min=100`) was ignored, and the saved view with those filters did not show as selected.

## [0.12.1] - 2026-11-27

### Changed

- Credit card plans: in the plan's first month, the purchases of the month before the plan are listed under "<Month>, before the plan" with "Count in this month", so a purchase billed with the first statement counts toward its budget and payment. "Move back" undoes it until that month's payment day.

### Security

- Updated the `url` crate to 2.5.9 (RUSTSEC-2026-0592).

## [0.12.0] - 2026-11-27

### Added

- Credit card plans: give a credit card account a monthly budget, the bank account that pays it and the payment day. The account page shows what is still available this month, the month's transactions and the settlement; the accounts list shows "… left of …" under the card's balance. Saldo books the settlement transfer from the bank account to the card on the payment day and keeps its amount in step with the month's spending until then. "Count in next month" moves a transaction to the next month's budget and settlement without changing its date, and "Move back" undoes it. The plans, budgets and moves are in the export.
- Notes on transactions, budgets, bills and accounts take several lines and support Markdown (lists, links, tables, task lists), with a preview while editing. Account notes are shown formatted on the account page.

## [0.11.0] - 2026-11-21

### Added

- Create an expense or revenue account while entering a transaction: type a new name in the destination or source account field and choose "Create expense account “…”" or "Create revenue account “…”". It takes the currency of your account on the other side, or the household's primary currency. Works in the recurrence editor too.

## [0.10.0] - 2026-11-20

### Added

- Sessions page under "More": every browser and desktop app you are logged in with, when each was last used, "Sign out" for one, and "Sign out everywhere else" (`GET /auth/sessions`, `DELETE /auth/sessions/{id}`, `POST /auth/sessions/revoke-others`). Personal access tokens are not affected.

### Changed

- A Web Push channel whose subscription no longer exists (the push service answers 404 or 410) is paused at once instead of being retried.

### Fixed

- Searching transactions or accounts for text with `%` or `_` matches those characters literally ("50%" no longer finds "500").

### Security

- Updated the email library (lettre 0.11.24: SMTP command injection, STARTTLS reply forgery, misrouted mail) and the bundled SQLite.
- Every response carries a Content-Security-Policy without inline scripts, `X-Frame-Options`, `X-Content-Type-Options` and `Referrer-Policy`, plus HSTS when the server's public URL is https.
- Requests that take longer than 120 seconds are answered with 408.

## [0.9.0] - 2026-11-20

### Added

- Bill history on a bill's page: a chart of every payment over the bill's whole duration, with the expected amount range, each due date marked paid, skipped, open, overdue or scheduled, and today; a summary (payments, total, average, lowest, highest); and the list of linked transactions, including those linked with "Mark paid" (`GET /bills/{id}/history`).

## [0.8.0] - 2026-11-20

### Added

- Dark mode: choose System, Light or Dark under "More" (remembered per device); the theme applies before the page appears, and the desktop title bar follows it.
- More transaction filters: amount range, budget, bill, reconciled and attachments, plus "No category" and "No budget" (`GET /transactions` takes `amount_min`, `amount_max`, `currency`, `has_attachments`, and `none` for `category_id` and `budget_id`).
- Saved views: save the current filters under a name and restore them from a chip above the transaction list; an "Uncategorised" chip is always there (`/saved-views`).
- Keyboard shortcuts and a command palette: ⌘K / Ctrl+K (or the search button) opens pages, actions, templates, saved views and accounts; `g` then `t`/`a`/`b`/`r`/`f` jumps between pages, `n` starts a transaction, `/` searches, `?` lists the shortcuts, and ⌘/Ctrl+Enter saves a form.
- More notifications, chosen per channel: a budget at 80% and 100%, the forecast going below zero in the next 30 days, large transactions (amount set by the owner), recurring transactions booked on their date, piggy bank and net worth goals reached, and a weekly digest on Monday mornings (`GET /notification-types`). Existing channels keep receiving bill reminders only.
- New reports: savings rate, year over year, by payee, and average monthly spending per category (also shown on the dashboard).
- Net worth goals, with progress, the monthly amount still needed and the projected date, managed on the net worth report and drawn on its chart (`/net-worth-goals`).
- "Safe to spend this month" on the dashboard: this month's income minus spending, bills, planned payments, what budgets have left, piggy bank targets and transfers to savings, with a per-day figure and its breakdown (`GET /reports/safe-to-spend`).

## [0.7.0] - 2026-11-17

### Added

- Reconciliation: "Reconcile" on an asset or liability account ticks its transactions off against a bank statement's date and closing balance, and can book a difference as a reconciliation transaction. The account page shows "Reconciled up to …" with the history; the latest reconciliation can be undone. `GET /transactions` takes `reconciled`.
- A Bill picker in the transaction and recurring transaction editors: a split can pay a bill, which marks the bill's matching period paid.

### Changed

- Changing a reconciled transaction's amount, date or accounts takes it out of its reconciliation; the editor asks first.

### Fixed

- Saving a transaction from the web app no longer clears its reconciled flag (`PUT /transactions/{id}` keeps it when the request leaves it out).

## [0.6.0] - 2026-11-17

### Added

- Budget carry-over for every budget, including hand-made limits: carry nothing, what is left, or what is left or overspent into the next limit. The carried amount is worked out when limits are read, so a late edit in an earlier month updates the later ones; progress bars show it next to the base amount.
- A cash-flow forecast (`GET /reports/forecast`, Forecast page, "Next 30 days" card on the dashboard): the balance projected up to two years ahead from transactions already entered with a later date, recurring transactions and bills, with the lowest point and a warning when it goes below zero.
- Subscription suggestions: repeating payments without a bill are listed on the bills page (`GET /subscription-candidates`) and can be added as a bill, which links the past payments, or dismissed.

### Changed

- The `rollover` auto-budget type is gone: existing rollover budgets become automatic limits with carry "what is left", and their limits go back to the base amount (the leftover used to be stored inside it). The Firefly importer maps rollover budgets the same way.
- Write transactions that skip `BEGIN IMMEDIATE` are now rejected by the linter.

## [0.5.0] - 2026-11-17

### Added

- Merging tags: "Merge into…" on the tags page, and a "Case duplicates" page that merges groups of tags whose names differ only in case in one go (`GET /tags/duplicates`, `POST /tags/merge`). The kept tag takes over the others' transactions, recurring transactions, templates, rules and attachments.

### Changed

- Tag names are unique ignoring case: creating or renaming to "Food" when "food" exists is refused. Existing duplicates stay until merged.
- The Firefly importer maps a tag to an existing tag of the same name in another case instead of creating a duplicate, and keeps a tag's Saldo spelling when the names differ only in case.

## [0.4.0] - 2026-11-16

### Added

- Search on the accounts page, across all account types (`GET /accounts?q=`).
- An on/off switch on each bill, recurring transaction, rule and rule group in its list.

### Changed

- The accounts page loads assets and liabilities at once; expense, revenue and equity accounts load only when their card is opened, 50 at a time with "Load more".
- Inactive bills, recurring transactions and rules are dimmed. Inactive bills and recurring transactions move to a folded "Inactive" section; inactive rules stay in place (their order is the order they run in), and an inactive rule group's rules start folded.

## [0.3.0] - 2026-11-16

### Added

- A group with several splits shows its total next to its title in the transaction list.

### Changed

- Transaction list on phones: no more sideways scrolling; the accounts sit under the description, and tapping a description opens it. "New from template" moved to the new-transaction page on phones.
- The list's filters fold behind a "Filters" button (with the number of active filters); search stays in view.
- Accounts in the list show as chips with an icon for their type.
- The transaction editor offers only accounts that fit: never an expense account as source or a revenue account as destination, only own accounts opposite an expense, revenue or equity account, and not the account already on the other side.

## [0.2.2] - 2026-11-16

### Fixed

- Write transactions begin with `BEGIN IMMEDIATE`, so two concurrent writes no longer fail with "database is locked".

## [0.2.1] - 2026-11-16

### Changed

- The macOS app is signed with a stable self-signed certificate instead of ad hoc, so it keeps its keychain access after an update. The first update to this version asks once more.

## [0.2.0] - 2026-11-15

### Added

- In-app desktop updates from GitHub Releases: a check at start and "Check for updates" in the More menu; updates are signature-checked, installed and the app restarts.

## [0.1.3] - 2026-11-15

### Added

- Attachments on transactions and bills in the web app, and listing the attachments of one split, account, tag or bill.
- Sort report tables by clicking a column header.

### Fixed

- Editing a transaction keeps its split IDs and named splits; attachments are deleted with their owner.

## [0.1.2] - 2026-11-15

### Added

- Categories have an optional colour, shown in the transaction list.
- Transactions are listed by day with coloured kinds and signed amounts; the app opens on that list.
- Dates are picked from a calendar; descriptions suggest earlier ones and fields are not autocorrected.

## [0.1.1] - 2026-11-14

### Fixed

- The macOS app is signed ad hoc, so macOS no longer reports it as damaged.

## [0.1.0] - 2026-11-14

### Added

- Accounts, categories, tags and transactions with splits, budgets with auto-budget limits, bills with reminders until paid, piggy banks, recurring transactions, transaction templates and rules.
- Reports: net worth, cash flow, categories, tags and budgets, and a dashboard.
- Signed webhooks with a delivery log; notifications through ntfy, Gotify, email and Web Push.
- A shared household with live updates over server-sent events.
- Import from Firefly III, export as JSON or CSV, and consistent database backups.
- Log in with a password or through Authentik (OpenID Connect).
- The web app (installable as a PWA) and a desktop app for macOS, Windows and Linux with the token in the system keychain.
