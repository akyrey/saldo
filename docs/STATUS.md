# Saldo status

Every agent reads this file before starting and updates it before stopping. Keep entries short and factual. Newest entries go at the top of each section.

## Phase state

| Phase | State | Branch / last commit | Notes |
|---|---|---|---|
| 0 Scaffold | done | main / c3cc9b0 | Workspace, 4 crate skeletons, lints, toolchain, CI |
| 1 Core | done | main / b2e1f5e | saldo-core: 75 unit/property tests + 1 doc test, 99.93% line coverage |
| 2 Server | in progress | | |
| 3 Importer + export | not started | | |
| 4 Web | not started | | |
| 5 Planning features | not started | | |
| 6 Events | not started | | |
| 7 Clients | not started | | |

States: not started, in progress, blocked (say on what), done.

## Deviations from ARCHITECTURE.md

Record every place where the code differs from the architecture document, why, and whether the owner approved it. Update `ARCHITECTURE.md` in the same commit.

- 2026-11-13 Phase 1: `SplitKind::derive` returns `Result<SplitKind, InvalidAccountPair>` instead of `Result<_, ValidationError>`, because `derive` has no split index to put in a `ValidationError`. Owner approved. ARCHITECTURE.md 4.1 updated.
- 2026-11-13 Phase 1: `Currency` fields are private (built with `Currency::new`, which checks 0 to 8 places, also on deserialise), so `Money` maths never meets an unchecked scale. Owner approved (2026-11-13). ARCHITECTURE.md 4 updated.
- 2026-11-13 Phase 1: `serde_json` is a normal dependency of `saldo-core`, for `SplitMeta(serde_json::Map)` (the `meta` JSON column, including nested `meta.firefly`). It does no I/O. Owner approved (2026-11-13). ARCHITECTURE.md 3 updated.

## Decisions made during implementation

Choices the briefs left open (crate picked, naming, a trade-off), with one line of reasoning each, so later phases follow them.

- 2026-11-13 Phase 1: public API of `saldo-core` (all re-exported at the crate root): `Money`, `Currency`, `CurrencyCode`, `MAX_DECIMAL_PLACES`, ID newtypes (`HouseholdId`, `UserId`, `AccountId`, `GroupId`, `SplitId`, `CategoryId`, `TagId`, `BudgetId`, `BillId`), `AccountType`, `EquityRole`, `Account`, `SplitKind`, `NewSplit`, `NewTransactionGroup`, `SplitMeta`, `ValidatedGroup`, `ValidatedSplit`, `AccountLookup`, `validate_group`, `balance_deltas`, `apply_deltas`, errors `IdError`, `CurrencyError`, `MoneyError`, `InvalidAccountPair`, `ValidationError`, `Reference`.
- 2026-11-13 Phase 1: `AccountLookup` owns cloned accounts (no lifetime parameter) and also holds the household ID and the known category/budget/bill/tag IDs, so invariant 5 is checked in core. Services must load every referenced ID filtered by household; a missing one fails validation.
- 2026-11-13 Phase 1: `ValidationError` codes beyond the brief's list: `account_outside_household`, `unknown_reference`, `foreign_amount_not_positive`, `foreign_currency_same_as_amount`, `foreign_currency_mismatch`. Split indexes are zero-based.
- 2026-11-13 Phase 1: `Money::parse_decimal` is strict: optional leading `-`, digits, optional `.` plus 1..=places digits. No `+`, spaces, commas, exponents or bare `.5` / `7.`.
- 2026-11-13 Phase 1: the currency of a split comes from the source account if it is an asset or liability, otherwise the destination; this equals the per-kind rule in ARCHITECTURE.md 4.2 (6).
- 2026-11-13 Phase 1: `clippy.toml` allows `unwrap`/`expect` in tests only; the workspace lints still deny them elsewhere under `-D warnings`.
- 2026-11-13 Phase 0: license `MIT` (owner's choice; single `LICENSE` file); edition 2024, resolver 3; `publish = false` on every crate; `rust-toolchain.toml` pins `stable` (1.100.0 at scaffold time).
- 2026-11-13 Phase 0: internal crates (`saldo-core`, `saldo-app`) are declared once as path dependencies in `[workspace.dependencies]` and pulled in with `.workspace = true`.
- 2026-11-13 Phase 0: CI uses `dtolnay/rust-toolchain@stable` and `Swatinem/rust-cache@v2` for cargo caching.
- 2026-11-11 Planning: app named Saldo; UUIDv7 over ULID; React + TypeScript SPA with PWA and Tauri 2; SQLite only; everything in a household is visible to every member.

## Open questions for the owner

- Phase 3 will need the URL and a personal access token of the Firefly III instance, and its version number.

## Findings about Firefly III

Facts checked against the owner's instance (Phase 3 fills this in; see ARCHITECTURE.md section 9.1 for the list of unconfirmed points).

- none yet
