# Saldo

Self-hosted personal finance server in Rust, replacing Firefly III. Cargo workspace: `saldo-core` (pure domain), `saldo-app` (services, SQLx, migrations), `saldo-server` (Axum API, binary `saldo`), `saldo-importer` (binary `saldo-import`); `web/` holds the React SPA.

## Before you do anything

1. Read `docs/STATUS.md` (current state, decisions, deviations).
2. Read the brief for your phase in `docs/phases/`, then the sections of `docs/ARCHITECTURE.md` it lists.
3. `docs/ARCHITECTURE.md` is the contract. If it is wrong or the brief contradicts it, stop and ask the owner; do not work around it.
4. Stay inside your phase's scope and stop when its acceptance criteria pass. Update `docs/STATUS.md` last.

## The owner

A professional PHP/Laravel and Go developer who knows only the basics of Rust and is learning it through this project.

- The first time a Rust concept appears (ownership, borrowing, lifetimes, traits, enums with data, `Result` and `?`, async, `Arc`, generics, macros), explain it in two or three sentences with an analogy to PHP/Laravel or Go. Add a short code comment where the analogy helps a later reader.
- Write simple, idiomatic Rust. No clever generics, no macros of our own, no premature abstraction. If a simpler version costs a little performance, choose the simpler version.

## Before every commit

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```

All three must pass. From Phase 4, also run in `web/`: `npm run lint`, `npm run typecheck`, `npm test`.

## Commits

- Small commits: one work package or one logical step each.
- Conventional format: `<type>(<scope>): <description>`, with types feat, fix, refactor, docs, test, chore, perf, ci and the crate or area as scope, for example `feat(core): derive split kind from account types`.
- Add a body only when the reason is not obvious.
- No `Co-Authored-By` trailer or other AI attribution in commits or pull requests.
- Commit only when the owner has asked for the phase to be implemented; never push without being asked.

## Code rules

- Tests first. Target 80% line coverage (`cargo llvm-cov`); `saldo-core` close to 100%.
- Money is `i64` minor units plus a currency code. Never use floats for amounts, in Rust, SQL, JSON or TypeScript.
- `saldo-core` does no I/O and never reads the clock or generates IDs itself; those are parameters.
- Every service and repository function takes a `HouseholdCtx` first and filters by `household_id`.
- Every state change appends an event in the same database transaction.
- No `unsafe`. No `unwrap()` or `expect()` outside tests; return a typed error.
- Errors: `thiserror` enums in `core` and `app`; the server maps them to the API error envelope. Never swallow an error; never log secrets or tokens.
- SQL goes through SQLx's checked macros (`query!`, `query_as!`). After changing a query or migration, run `cargo sqlx prepare --workspace` and commit `.sqlx/`.
- Never edit a merged migration; add a new one with `sqlx migrate add`.
- Add dependencies with `cargo add`; shared ones go in `[workspace.dependencies]`.
- Keep files focused (aim for under 400 lines) and functions short (under 50 lines).
