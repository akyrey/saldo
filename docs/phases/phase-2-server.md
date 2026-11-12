# Phase 2: `saldo-app` and `saldo-server`

## 1. Goal

A running server with a database: the ledger schema, services that validate through `saldo-core` and append an event for every change, a REST API with a generated OpenAPI document, password login, sessions, API tokens, and the household.

When finished, the owner can start `saldo serve`, call `POST /api/v1/setup`, log in with `curl`, add a second household member, create accounts, post a transaction whose splits have different sources and destinations, read balances, and see the matching rows in `GET /api/v1/events`.

## 2. Read first

- `CLAUDE.md`, `docs/STATUS.md`
- `docs/ARCHITECTURE.md` sections 3, 5 (intro, 5.1, 5.2, `events` in 5.5, 5.6), 6, 7, 8.1, 12
- `crates/core/src/lib.rs` and its doc example

## 3. Prerequisites

Phase 1 is done: `cargo test -p saldo-core` passes and `validate_group` exists. Install `sqlx-cli` (`cargo install sqlx-cli --no-default-features --features sqlite`).

## 4. Scope

In scope: migrations for identity, ledger, `events`, `external_refs`, `import_runs`; repositories and services for households, users, sessions, accounts, categories, tags, transactions; event append; the endpoints marked Phase 2 in ARCHITECTURE.md section 6; OpenAPI; configuration; structured logging; `saldo serve`, `saldo user create`, `saldo openapi`.

Out of scope: attachments and export (Phase 3); budgets, bills, piggy banks, recurrences, rules, reports (Phase 5), though `splits.budget_id` and `splits.bill_id` columns are created now and stay NULL; webhook delivery, notifications, SSE (Phase 6); any UI.

## 5. Work packages

Packages 1 to 6 are sequential. After 6, packages 7 to 9 can be done in any order.

1. **Database plumbing** (`crates/app/src/db.rs`, `crates/app/migrations/`): connection pool with the pragmas from ARCHITECTURE.md section 5, migrations embedded with `sqlx::migrate!`. First migration: the SQL of sections 5.1, 5.2, the `events` table and its triggers from 5.5, and 5.6, copied exactly. Seed migration: a list of common currencies. Test: migrations apply to an in-memory database; updating or deleting an event row fails.
2. **Cross-cutting types** (`ctx.rs`, `clock.rs`, `error.rs`, `events.rs`): `HouseholdCtx { household_id, user_id, role }`; `Clock` trait with `SystemClock` and a test `FixedClock`; `IdGen` (UUIDv7 from the clock); `AppError` (`NotFound`, `Conflict`, `Validation(Vec<ValidationError>)`, `Forbidden`, `Database`); `append_event(&mut tx, ctx, event)`.
3. **Identity services** (`identity/`): create the first user and household (and its two Equity accounts) in one transaction; add and remove members; Argon2id hashing; session and token creation (store SHA-256, return the token once), lookup by token, expiry, revocation. Tests include: wrong password, expired session, token of a removed member.
4. **Ledger services** (`accounts/`, `categories/`, `tags/`, `transactions/`): repository (SQL) and service (transaction + validation + event) per area. `TransactionService::create/replace/delete/get/list`; create and replace load the referenced accounts, call `core::validate_group`, write the group, splits and tags, append `transaction.created|updated|deleted`. `AccountService::balance(ctx, id, date)`. Deleting an account that has splits is a `Conflict`; account type cannot be changed. Tests: every service test asserts the event row; a household-isolation test proves a ctx for household A cannot read or write household B.
5. **Server skeleton** (`crates/server/src/`): `clap` CLI (`serve`, `user create`, `openapi`), configuration from `SALDO_*` environment variables with clear startup errors, `tracing` logs with request IDs, `AppState`, graceful shutdown, `/healthz`, the error-to-envelope mapping, the response envelope types.
6. **Auth layer**: an Axum extractor that turns the cookie or bearer token into `HouseholdCtx` (401 otherwise), the CSRF check for cookie requests, login rate limiting, `POST /setup`, `/auth/*`, `/household*`.
7. **Ledger endpoints**: `/currencies`, `/accounts`, `/categories`, `/tags`, `/transactions` with filters, pagination and `scope`. Request and response DTOs live in the server crate, derive `utoipa::ToSchema`, and convert to and from core types (amounts as decimal strings).
8. **Events endpoint**: `GET /events?after_seq=&type=`.
9. **OpenAPI**: utoipa document covering every route, served at `/api/v1/openapi.json`; `saldo openapi > web/openapi.json` committed; a test that fails when the committed file is stale.

## 6. Contracts

Honour: table definitions in ARCHITECTURE.md section 5 exactly; API conventions and endpoints in section 6; auth rules in section 7; event payload shape in 8.1.

Provide to later phases:

```rust
// crates/app
pub struct HouseholdCtx { pub household_id: HouseholdId, pub user_id: UserId, pub role: Role }
pub trait Clock: Send + Sync { fn now(&self) -> DateTime<Utc>; }
pub enum Origin { Api, Import, Scheduler, Rule, System }
pub async fn append_event(tx: &mut Transaction<'_, Sqlite>, ctx: &HouseholdCtx, origin: Origin, event: NewEvent) -> Result<i64, AppError>;
pub struct App { /* pool, clock, id generator */ }   // entry point holding all services
```

- Every public service method has the shape `async fn name(&self, ctx: &HouseholdCtx, origin: Origin, ...) -> Result<T, AppError>`. The importer (Phase 3) calls these with `Origin::Import`.
- A module per feature under `crates/app/src/` and `crates/server/src/routes/`, registered in one place each, so Phases 5 and 6 add modules without editing existing ones.
- `.sqlx/` is committed and CI builds with `SQLX_OFFLINE=true`.

## 7. Rust concepts to explain

| Concept | Where | Analogy |
|---|---|---|
| `async`/`await`, futures | first service method | Go runs blocking-looking code on goroutines; Rust marks functions `async` and you `await` each call, like JavaScript promises. PHP-FPM has neither: each request has its own process |
| Tokio runtime, `tokio::spawn` | `main.rs` | The Go scheduler, but as a library you start yourself; `tokio::spawn` is `go func()` |
| `Arc<T>` and shared state | `AppState` | A reference-counted pointer shared between tasks; in Go you would just share a pointer and rely on the GC. A Laravel singleton in the container, but safe across threads |
| `Send` / `Sync` | when a handler fails to compile | Compile-time proof that a value may cross threads; what Go's race detector checks at run time |
| Lifetimes in signatures | `Transaction<'_, Sqlite>` | The transaction borrows the connection; the `'_` says "cannot outlive it". Like a Go `*sql.Tx` you must not use after the connection closes, enforced by the compiler |
| Axum extractors | auth, `Json<T>`, `Path<T>` | Laravel form requests and route-model binding, or Go middleware that fills a context value; here they are typed function arguments |
| Tower middleware/layers | logging, CSRF | Laravel middleware / Go `func(http.Handler) http.Handler` |
| SQLx checked macros | repositories | The query is checked against the real schema at compile time; like PHPStan understanding your SQL, or `sqlc` in Go without the generated files |
| `From`/`Into` and `?` conversion | `AppError` → HTTP response | Go's error wrapping, done automatically by the type system |
| Traits for testing | `Clock` | A Go interface with a fake implementation; Laravel's `Carbon::setTestNow` without global state |
| Derive macros with attributes | `serde`, `utoipa` | PHP attributes or Go struct tags that generate code at compile time |

## 8. Acceptance criteria

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
SQLX_OFFLINE=true cargo test --workspace
cargo llvm-cov --workspace --summary-only     # >= 80%
cargo run -p saldo-server -- openapi | diff - web/openapi.json
```

Manual check, recorded in the hand-off: start the server on a fresh database and run the flow from the Goal with `curl` (setup, login, add member, three accounts, one group with two splits with different sources, balances, events).

- Cross-household access is covered by tests at both service and HTTP level.
- A failed validation returns 422 with the `code()` values from `saldo-core` and writes no rows and no event.
- No endpoint except `/healthz`, `/setup`, `/auth/login` and the OpenAPI document works without authentication.

## 9. Hand-off

In `docs/STATUS.md`: Phase 2 done with commit hash; the crates chosen (argon2, rate limiting, etc.); the exact service method naming pattern; how to run the server locally (env vars, command); anything in ARCHITECTURE.md that had to change.
