# Changelog

All notable changes to Saldo. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [Semantic Versioning](https://semver.org/). Each version is the `vX.Y.Z` tag that releases the desktop app and the server image.

## [Unreleased]

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
