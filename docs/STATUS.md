# Saldo status

Every agent reads this file before starting and updates it before stopping. Keep entries short and factual. Newest entries go at the top of each section.

## Phase state

| Phase | State | Branch / last commit | Notes |
|---|---|---|---|
| 0 Scaffold | done | main / c3cc9b0 | Workspace, 4 crate skeletons, lints, toolchain, CI |
| 1 Core | done | main / b2e1f5e | saldo-core: 75 unit/property tests + 1 doc test, 99.93% line coverage |
| 2 Server | done | main / 4d431a6 | 186 tests workspace-wide, 96.83% line coverage; manual curl flow passed |
| 3 Importer + export | done | main / 2f4c6ba | Live import of the owner's Firefly passed twice (second run: no changes); 26/26 balances match; Docker image builds and runs |
| 4 Web | done | main / 4bd407b | React SPA embedded in the server; 69 Vitest + 6 Playwright flows pass; initial route 130 kB gzipped; PWA install on a phone not yet checked |
| 5 Planning features | not started | | |
| 6 Events | not started | | |
| 7 Clients | not started | | |

States: not started, in progress, blocked (say on what), done.

## Deviations from ARCHITECTURE.md

Record every place where the code differs from the architecture document, why, and whether the owner approved it. Update `ARCHITECTURE.md` in the same commit.

- 2026-11-13 Phase 4: TypeScript 5.9 instead of the newest 7.0, because `openapi-typescript` and `typescript-eslint` do not support 7 yet. Not an ARCHITECTURE.md change (it names no version).
- 2026-11-13 Phase 4: two API additions for the web app: `GET /setup` (public, `{ "needed": bool }`) and `GET /accounts/balances` (a page of balances with `GET /accounts`'s filters). ARCHITECTURE.md 6 updated.
- 2026-11-13 Phase 3: `append_event` takes `&mut SqliteConnection` instead of `&mut Transaction<'_, Sqlite>`, so the `<method>_in` service variants can share the importer's transaction. Callers still pass `&mut tx` (a transaction dereferences to its connection). ARCHITECTURE.md unchanged (it did not spell the signature out).
- 2026-11-13 Phase 3: imports (`Origin::Import`) may book splits on inactive accounts (rule 8 protects new entries; history may sit on accounts archived since). Not yet approved by the owner.
- 2026-11-13 Phase 3: the export leaves out the event log, sessions/tokens and `import_runs` (audit trail, secrets, bookkeeping; not needed to rebuild the household); attachment files are referenced by SHA-256, not embedded. Not yet approved by the owner.
- 2026-11-13 Phase 3: `saldo-import` takes `--token-env <VAR>` (the brief's contract) instead of ARCHITECTURE.md's `--token`, so the token never appears in shell history or `ps`. ARCHITECTURE.md 9.1 updated.
- 2026-11-13 Phase 2: response types are `saldo-app` views deriving `utoipa::ToSchema` (not server DTOs), because events must carry "the entity as the API returns it" and are written in `saldo-app`. Request DTOs stay in the server. Owner approved. ARCHITECTURE.md 3 and 8.1 updated.
- 2026-11-13 Phase 2: the session cookie is `Secure` only when `SALDO_PUBLIC_URL` is https, so a plain-http LAN or localhost setup can still log in from a browser. Not yet approved by the owner. ARCHITECTURE.md 7 updated.
- 2026-11-13 Phase 2: login, logout and token create/revoke append no event (auth state, not household data; keeps session activity out of webhook payloads). Not yet approved by the owner. ARCHITECTURE.md 8.1 updated.
- 2026-11-13 Phase 2: new event types `household.created` and `household.updated`; setup appends four events in one transaction. ARCHITECTURE.md 8.1 updated.

- 2026-11-13 Phase 1: `SplitKind::derive` returns `Result<SplitKind, InvalidAccountPair>` instead of `Result<_, ValidationError>`, because `derive` has no split index to put in a `ValidationError`. Owner approved. ARCHITECTURE.md 4.1 updated.
- 2026-11-13 Phase 1: `Currency` fields are private (built with `Currency::new`, which checks 0 to 8 places, also on deserialise), so `Money` maths never meets an unchecked scale. Owner approved (2026-11-13). ARCHITECTURE.md 4 updated.
- 2026-11-13 Phase 1: `serde_json` is a normal dependency of `saldo-core`, for `SplitMeta(serde_json::Map)` (the `meta` JSON column, including nested `meta.firefly`). It does no I/O. Owner approved (2026-11-13). ARCHITECTURE.md 3 updated.

## Decisions made during implementation

Choices the briefs left open (crate picked, naming, a trade-off), with one line of reasoning each, so later phases follow them.

- 2026-11-13 Phase 4: built by a delegated agent in a git worktree (branch `phase-4-web`, merged in 749348d) while Phase 3 ran in the main checkout; the export page and the Docker web stage were added after the merge.
- 2026-11-13 Phase 4: stack (owner-confirmed): React 19.3, TypeScript 5.9 (strict + `noUncheckedIndexedAccess`), Vite 8.3, TanStack Query 5.107 and Router 1.170 (code-based routes, every page lazy-loaded), shadcn/ui (new-york, files in `web/src/components/ui/`) on Tailwind 4.3, `lucide-react`, `openapi-typescript` 7.13 + `openapi-fetch` 0.17, `vite-plugin-pwa` 2.0 (Workbox `generateSW`), Vitest 5.1 (jsdom, Testing Library), Playwright 1.65, ESLint 10 + typescript-eslint 8, Prettier 3. Rust: `rust-embed` 8.14 behind the `embed-web` feature of `saldo-server`.
- 2026-11-13 Phase 4: folder convention: `web/src/api/` is the only code that knows URLs (`client.ts`, generated `schema.d.ts` via `npm run gen:api`, `download.ts`, `money.ts` with `bigint` minor units; ESLint forbids `parseFloat`); `web/src/features/<feature>/` holds `queries.ts`, pure logic in `.ts` files with tests beside them, and pages; shared UI in `web/src/components/`; routes registered in `web/src/router.tsx`; whole-app tests use `fakeApi` and `renderApp(path)` from `web/src/test/`; Playwright flows in `web/e2e/` run against `target/debug/saldo` built with `embed-web`. Query keys: `["me"]`, `["accounts", ...]`, `["balances", ...]`, `["transactions", ...]`, `["categories"]`, `["tags"]`, `["members"]`, `["currencies"]`; writes invalidate by prefix (Phase 6's SSE listener can do the same).
- 2026-11-13 Phase 4: the scope switch and all transaction filters live in the URL (`?scope=mine|user:<id>`, `from`, `to`, `account_id`, `category_id`, `tag_id`, `kind`, `q`, `page`). Pickers are native `<select>` (light, phone-friendly); amount input accepts one comma as decimal separator; kind badges use the same table as `SplitKind::derive`; server field errors (`splits[i].field`) appear on the matching field.
- 2026-11-13 Phase 4: the server serves `web/dist` for every non-API GET/HEAD path (`index.html` for extensionless app routes, 404 for missing files with an extension; `/api/*` and `/healthz` keep the JSON 404). Hashed `assets/*` are cached immutably for a year, `index.html`, `sw.js` and the manifest are `no-cache`; all get `nosniff`. The service worker caches the app shell only, never `/api`; an offline banner shows when the browser is offline.
- 2026-11-13 Phase 4: npm 10 (bundled with Node 23) crashes resolving vitest 5's optional peers on `npm install`; `npm ci` from the lock file works. Add or upgrade packages with `npx npm@11 install ...`. CI and the Docker image use Node 24.
- 2026-11-13 Phase 3: entity importer signature for Phase 5: `pub async fn import_<entity>(cx: &ImportCx, report: &mut ImportReport) -> Result<(), ImportError>` in `crates/importer/src/entities/<entity>.rs`, registered in `engine::ENTITIES` and the `match` in `engine::run_entities`. `ImportCx` holds `firefly: FireflyClient` (GET only), `app: App`, `ctx: HouseholdCtx`, `state: tokio::sync::Mutex<State { tx, refs: ExternalRefs }>`, `dry_run`, `since`. Lock `cx.state`, then call services' `*_in(&mut state.tx, ...)` with `Origin::Import`; resolve earlier entities with `state.refs.resolve(entity_type, firefly_id)` (types: `account`, `account.cash_expense`, `account.cash_revenue`, `category`, `tag`, `transaction_group`, `split`, `attachment`) and record new ones with `state.refs.upsert`. Report with `report.created/updated/unchanged/skipped/note`. Item-level problems: `entities::skippable(error)?` turns invalid/conflict/validation errors into a skip and lets database errors abort.
- 2026-11-13 Phase 3: idempotency is by comparison: each importer builds the input it would write and compares it with the stored view (`same(...)`); only differences are written, and `ExternalRefs::upsert` skips unchanged mappings, so a second run leaves the database byte-for-byte identical (tested). Each real run appends one `import.completed` event.
- 2026-11-13 Phase 3: every write service has a `<method>_in(&self, conn: &mut SqliteConnection, ...)` variant (also `get_in`, `list_in`, `balance_in`) that runs inside the caller's transaction; the public method is `begin`, `<method>_in`, `commit`.
- 2026-11-13 Phase 3: Firefly mapping choices: all `initial-balance` accounts map to the one Equity(OpeningBalance) account and `reconciliation` accounts to Equity(Reconciliation); `cash` becomes a `(cash)` expense and a `(cash)` revenue account picked by direction; `import` accounts are skipped; `liabilities` → liability with `debit` → `i_owe`, `credit` → `owed_to_me`; split date = local date of Firefly's date-time (original kept in `meta.firefly.date`); same-currency and zero foreign amounts are dropped; budget and subscription links wait for Phase 5 (reported as notes); attachments on bills wait for Phase 5. Kind differences are notes, not errors: Firefly calls a payment into a liability a withdrawal, Saldo a transfer (41 on the owner's data).
- 2026-11-13 Phase 3: crates: reqwest 0.13 (rustls, json, query, http2), wiremock 0.6 (tests), zip 9 (deflate) for the CSV export.
- 2026-11-13 Phase 3: attachments: files at `SALDO_DATA_DIR/attachments/<sha256>` (written via a temporary file and rename; shared by equal content; deleted with the last reference), 20 MiB limit, MIME allow-list (PDF, PNG, JPEG, GIF, WebP, HEIC, text, CSV, XML, octet-stream), downloads always `attachment` + `nosniff`. Attachable types now: `split`, `account`, `tag`.
- 2026-11-13 Phase 3: `saldo backup <path>` uses `VACUUM INTO` with the path as an escaped SQL literal and checks the file exists afterwards (SQLx's in-memory databases silently write nothing, so its test uses a file database).
- 2026-11-13 Phase 3: Docker: `rust:1.100-slim-trixie` + cargo-chef 0.1.73 builder, `debian:trixie-slim` runtime with `ca-certificates` and `curl` (for the health check), user `saldo` uid 10001, image 207 MB. `.dockerignore` drops `rust-toolchain.toml` so the image's Rust is used.
- 2026-11-13 Phase 2: service method pattern. Writes: `async fn create|update|replace|delete|add_member|... (&self, ctx: &HouseholdCtx, origin: Origin, ...) -> Result<T, AppError>`. Reads take no origin: `get|list|balance|me (&self, ctx, ...)`. Exceptions with no ctx yet: `IdentityService::setup(origin, input)`, `login(email, password)`, `authenticate(token)`; `CurrencyService::list()` is global. Services are public fields of `saldo_app::App` (`app.accounts`, `app.transactions`, ...); a new feature adds one field in `crates/app/src/lib.rs` and one `.merge(...)` in `crates/server/src/routes/mod.rs`.
- 2026-11-13 Phase 2: crates: axum 0.8 (+ `macros`), sqlx 0.9 (sqlite, runtime-tokio, macros, migrate), tokio 1, utoipa 6 + utoipa-axum 0.3 (routes and OpenAPI registered together), tower-http 0.7 (trace, request-id), tracing-subscriber 0.3 (JSON logs), clap 4, argon2 0.6 (default Argon2id params; salt generated by the crate), sha2 0.11 (token hashes), rand 0.10 (`rand::fill` for 256-bit tokens). argon2 and blake2 build with `opt-level = 3` in dev so tests stay fast.
- 2026-11-13 Phase 2: login rate limit is hand-written and in memory: 10 attempts per 15 minutes per email and per client IP (from the socket; behind a reverse proxy all clients share the proxy IP). Unknown emails are checked against a dummy Argon2 hash, so timing does not reveal which emails exist.
- 2026-11-13 Phase 2: sessions last 30 days (fixed, not sliding); API tokens never expire. Removing a member deletes their memberships' sessions, and `authenticate` also requires a current membership.
- 2026-11-13 Phase 2: error envelope codes: `not_found`, `conflict`, `validation_failed` (422; details carry core codes with field paths such as `splits[1].amount`, or code `invalid` for a single bad field), `unauthorized`, `forbidden`, `csrf_failed`, `bad_request` (malformed JSON, query or path), `rate_limited`, `internal` (details only in the log). `AppError` gained `Invalid`, `Unauthorized` and `Internal` beyond the brief's list.
- 2026-11-13 Phase 2: account type and currency never change; `PATCH` with a different `type` is 422. Equity accounts are created only by setup and cannot be deleted. Deleting an account with splits is 409.
- 2026-11-13 Phase 2: transaction input amounts are decimal strings; `currency` defaults to the asset/liability side's currency. `PUT` replaces all splits (they get new IDs). Budget and bill references fail as `unknown_reference` until Phase 5 creates those tables.
- 2026-11-13 Phase 2: lists of tokens, members and currencies are unpaginated `{ "data": [...] }`; every other list has `meta`. `GET /events` combines `after_seq` and `type` with `page`/`per_page`.
- 2026-11-13 Phase 2: `saldo user create --email --name --household [--currency EUR]` reads the password from stdin, so it never appears in shell history or `ps`.
- 2026-11-13 Phase 2: `.sqlx/` is committed and CI sets `SQLX_OFFLINE=true`. Locally, after changing SQL: `export DATABASE_URL=sqlite://$PWD/target/dev.db; sqlx migrate run --source crates/app/migrations; cargo sqlx prepare --workspace`. Without `DATABASE_URL` the macros use `.sqlx/`.
- 2026-11-13 Phase 2: tests use `db::connect_in_memory()` plus the real migrations and a `FixedClock` (not `#[sqlx::test]`), so they need no environment. HTTP tests call the router with `tower::ServiceExt::oneshot`. `crates/server/tests/openapi.rs` checks that `web/openapi.json` is current and that every non-public operation is documented with security and answers 401 without credentials.
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

## Running the server locally

```sh
cargo build -p saldo-server
SALDO_DATABASE_URL=sqlite://$PWD/data/saldo.db SALDO_BIND=127.0.0.1:8080 \
SALDO_PUBLIC_URL=http://localhost:8080 SALDO_LOG=info ./target/debug/saldo serve
# first user, either over HTTP (POST /api/v1/setup) or:
echo 'a long password' | SALDO_DATABASE_URL=sqlite://$PWD/data/saldo.db \
  ./target/debug/saldo user create --email me@example.com --name Me --household Home --currency EUR
```

Defaults: `SALDO_DATABASE_URL=sqlite:///data/saldo.db`, `SALDO_BIND=0.0.0.0:8080`, `SALDO_PUBLIC_URL=http://localhost:8080`, `SALDO_LOG=info`. Migrations run at startup; the database folder is created if missing. With curl, a cookie session needs `-H 'X-Saldo-Csrf: 1' -H 'Origin: <SALDO_PUBLIC_URL origin>'` on writes; a bearer token from `POST /api/v1/auth/tokens` needs neither.

Manual check (2026-11-13): fresh database, `saldo serve`, then setup, login, add member, three accounts, one group with two splits from different sources, balances (-42.50, -17.25, 59.75 EUR), nine events listed; a cookie write without the CSRF header got 403 and an unauthenticated list got 401; no token or password appeared in the log; SIGTERM logged `saldo stopped`.

## Web app development

```sh
SALDO_DATABASE_URL=sqlite://$PWD/data/dev.db SALDO_DATA_DIR=$PWD/data SALDO_BIND=127.0.0.1:8080 SALDO_PUBLIC_URL=http://localhost:5173 cargo run -p saldo-server -- serve
cd web && npm ci && npm run dev    # http://localhost:5173, /api proxied to 127.0.0.1:8080
```

`SALDO_PUBLIC_URL` must be the Vite origin in development (the CSRF check compares it with the browser's `Origin`). Checks: `npm run lint`, `npm run typecheck`, `npm test`, `npm run build`, `npm run size`; Playwright: `npm run build && cargo build -p saldo-server --features embed-web && npx playwright test`. The Docker image builds `web/` in a Node stage and embeds it.

Known gaps: PWA installability was not checked on a real phone (needs a device and https; Lighthouse 12 dropped its PWA category). The manifest (name, `display: standalone`, `start_url`, 192/512 px and maskable icons) and the service worker meet Chrome's install requirements. `npm audit` lists 15 advisories in dev-only build tooling; `npm audit --omit=dev` finds none.

## Importing from Firefly III

Native: `FIREFLY_TOKEN=... SALDO_DATABASE_URL=sqlite://$PWD/data/saldo.db SALDO_DATA_DIR=$PWD/data saldo-import firefly --url https://firefly.example.com --household Home [--dry-run]`.
In the container (see README): `docker compose exec -T -e FIREFLY_TOKEN saldo saldo-import firefly --url ... --household Home`. The household must exist first (`saldo user create` or `POST /setup`).

Live run on the owner's instance (2026-11-13, into a scratch database; counts only):

| entity | first run | second run |
|---|---|---|
| currencies | 1 created, 2 unchanged | 3 unchanged |
| accounts | 865 created, 12 mapped to equity | 877 unchanged |
| categories | 98 created | 98 unchanged |
| tags | 1,197 created | 1,197 unchanged |
| transactions | 12,362 groups created (12,550 splits) | 12,362 unchanged |
| attachments | 114 created, 9 skipped | 114 unchanged, 9 skipped |

Balance check: 26 asset/liability accounts, 0 mismatches, both runs. `saldo export json` held 867 accounts, 12,362 groups and 12,550 splits (equal to the database), `transactions.csv` 12,550 rows. The token appeared nowhere in the database, the export, the reports or `import_runs.summary`. Notes: 41 liability payments are transfers in Saldo (withdrawals in Firefly); 1 same-currency foreign amount dropped; 1,986 budget and 449 subscription links and 7 bill attachments wait for Phase 5.

Known gaps: 9 attachments are skipped because Firefly itself answers HTTP 500 for their files (missing on the Firefly server). Groups deleted in Firefly after an import stay in Saldo (counted as a note). Budgets, subscriptions, piggy banks, rules, recurrences and transaction links are Phase 5.

## Open questions for the owner

- Approve or reject the two Phase 3 deviations marked "not yet approved" (imports may book on archived accounts; what the export leaves out).

- Approve or reject the two Phase 2 deviations marked "not yet approved" (cookie `Secure` only on https; no events for sessions and tokens).


## Findings about Firefly III

Facts checked against the owner's instance (Phase 3 fills this in; see ARCHITECTURE.md section 9.1 for the list of unconfirmed points).

Checked 2026-11-13 against the owner's instance, Firefly III 6.7.9 (API 6.7.9, PostgreSQL), with read-only GET requests only.

- Scale: 876 accounts (750 expense, 87 revenue, 25 asset, 1 liability, 1 cash, 10 initial-balance, 2 reconciliation; 10 inactive), 12,550 splits in 12,362 groups, 98 categories, 1,197 tags, 130 attachments, 42 currencies (3 enabled: EUR 2, USD 2, TKT 0 decimal places), 326 transaction links, 6 budgets, 28 subscriptions, 29 rules, 21 recurrences, 0 piggy banks.
- `limit` has no practical maximum: 5000 returned all 876 accounts on one page. The importer uses 500.
- `GET /v1/transactions` without `type` omits opening balances and reconciliations (12,538 vs 12,550). Always pass `type=all`.
- `/v1/transactions` pages by **split**, not by group: `limit=500` returns 500 splits (453 groups on page 1), and `meta.pagination.total` counts splits. A multi-split group cut by a page boundary arrives as two parts with the same group ID and disjoint splits. The importer merges parts by journal ID, checks the split count against the total, and re-lists (up to 3 passes) if splits are still missing. (An earlier note here blamed unstable paging; that was wrong.)
- Amounts have 12 decimals (`"185.000000000000"`), independent of the currency. The importer trims zero padding to the currency's decimal places before `Money::parse_decimal`; any non-zero digit beyond them is an error.
- Dates are local date-times with an offset (`2026-11-10T08:40:00+01:00`); the split date is the first 10 characters. Some splits are dated in the future (up to 2027-09). `current_balance` is as of `current_balance_date` (today), so the balance check must compare up to that date.
- Liability sign: Saldo's rule (incoming minus outgoing, ARCHITECTURE.md 4.2) gives exactly Firefly's `current_balance` for the loan (`liability_direction` `debit`, balance negative). No sign flip is needed. `debit` maps to `i_owe`, `credit` to `owed_to_me`. The loan's `opening_balance` field has no matching transaction and is informational only.
- Account types on read: `asset`, `expense`, `revenue`, `liabilities` (plural; split `source_type`/`destination_type` say `Loan`), `cash`, `initial-balance` (one per asset account, 10), `reconciliation` (2). Asset roles seen: `defaultAsset`, `sharedAsset`, `savingAsset`, `ccAsset`, `cashWalletAsset`.
- Split groups: every multi-split group (123, up to 18 splits) shares one source; 10 have different destinations; types never mix within a group. So Firefly does force a shared source (for withdrawals) and the kind comparison per split is safe.
- Opening balances: 10 groups `Initial balance account` → `Asset account`. Reconciliations: 2, one in each direction.
- Foreign amounts: 436 splits. One split has `foreign_currency_code` equal to its own currency (EUR/EUR) and a different amount; Firefly ignores it for balances. Saldo rejects it (rule 7), so the importer drops that foreign amount and reports it.
- Cross-currency: TKT asset account (0 decimals) with withdrawals in TKT (some with EUR foreign amounts) and one TKT→EUR transfer with an EUR foreign amount, which matches Saldo's transfer rule.
- Balance check simulated on the downloaded data with Saldo's rules (foreign amount when the account is in the foreign currency, same-currency foreign amounts dropped, up to `current_balance_date`): all 26 asset and liability balances match exactly.
- `/v1/transaction-links` exposes `link_type_id`, `inward_id`, `outward_id`, `notes` on read.
- Piggy bank `accounts[]`: not confirmable, the instance has no piggy banks (Phase 5 must use the spec only).
- Splits carry `subscription_id` and the deprecated `bill_id`; tags as names; `category_id` on all but one split.

