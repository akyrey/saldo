# Changelog

All notable changes to Saldo. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [Semantic Versioning](https://semver.org/). Each version is the `vX.Y.Z` tag that releases the desktop app and the server image.

## [Unreleased]

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
