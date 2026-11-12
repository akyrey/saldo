# Phase 3: Firefly III importer, export, Docker

## 1. Goal

Get the owner's real data in and prove it can get out again. An idempotent CLI imports the ledger from a Firefly III instance through its API; the server exports everything as JSON and CSV; attachments are stored; the whole thing runs from one Docker image.

When finished, the owner runs `saldo-import firefly ...` twice against their instance: the first run imports accounts, categories, tags and all transactions; the second run reports zero changes; the balance check matches Firefly; `saldo export json` produces a complete file.

## 2. Read first

- `CLAUDE.md`, `docs/STATUS.md` (especially the Phase 2 hand-off: service naming and how to run the server)
- `docs/ARCHITECTURE.md` sections 5.2 (`attachments`), 5.6, 9, 11, 12
- The Firefly III OpenAPI spec: `https://raw.githubusercontent.com/firefly-iii/api-docs/main/dist/` (pick the file matching the owner's Firefly version; v6.7.8 was read during planning)
- `crates/app/src/transactions/` and `crates/app/src/accounts/` from Phase 2

## 3. Prerequisites

Phase 2 is done. Ask the owner for: the Firefly III URL, a personal access token (read from an environment variable, never committed or logged), and the Firefly version. Without access to a live instance, build everything against fixtures and mark the live verification as open in `STATUS.md`.

## 4. Scope

In scope: Firefly client; import of currencies, accounts, categories, tags, transactions (with opening balances and reconciliations), attachments; `external_refs` upsert; `import_runs`; dry run; balance check; attachment storage and endpoints; JSON and CSV export (endpoints and CLI); Dockerfile, docker-compose, `.env.example`.

Out of scope: import of budgets, bills, piggy banks, recurrences and rules (Phase 5 adds them to this importer; structure the code so each entity is one module); the web UI in the image (Phase 4 adds the Node build stage).

## 5. Work packages

1. **Confirm the open points** (no code): with `curl` against the owner's instance, check the unconfirmed items listed at the end of ARCHITECTURE.md 9.1 and write the answers into `STATUS.md` under "Findings about Firefly III". Save anonymised sample responses as fixtures under `crates/importer/tests/fixtures/` (replace names, IBANs, descriptions and amounts).
2. **Firefly client** (`crates/importer/src/firefly/`): `reqwest` client with bearer auth, a generic paginated `list<T>` following `meta.pagination.total_pages`, serde models containing only the fields the mapping needs (unknown fields ignored), retry on 429/5xx with backoff. Tests with `wiremock`: pagination, auth header, error statuses.
3. **Import engine** (`engine.rs`, `refs.rs`): `ExternalRefs` service in `saldo-app` (`find`, `upsert`), an `ImportReport` (created/updated/unchanged/skipped per entity plus messages), `--dry-run` (run everything in one transaction and roll back), `import_runs` bookkeeping.
4. **Entity importers**, one module each, in the order of the table in ARCHITECTURE.md 9.1: currencies, accounts (including the `(cash)` and Equity mappings), categories, tags, transactions. Amount strings go through `Money::parse_decimal`. Kind is derived by Saldo and compared with Firefly's `type`; mismatches go into the report. Tests per mapping rule, and the key test: importing the same fixtures twice yields an identical database and an all-unchanged second report.
5. **Balance check**: after the import, compare each asset and liability account's computed balance with Firefly's `current_balance`; print a table and exit non-zero on mismatch unless `--allow-mismatch`.
6. **Attachments**: storage on disk by SHA-256 under `SALDO_DATA_DIR/attachments/`, service + endpoints (upload with size limit and MIME allow-list, metadata, download, delete), then the attachment importer.
7. **Export**: `ExportService` producing the JSON document (format `saldo-export`, version 1) and the CSV zip; `GET /export/json`, `GET /export/csv` (owner only), `saldo export json|csv --household ... --out ...`. Test: export a seeded household and assert every table's rows appear; `transactions.csv` has one row per split.
8. **Docker**: multi-stage Dockerfile (cargo-chef, slim Debian runtime, non-root user, `/data` volume, both binaries), `docker-compose.yml` with health check, `.env.example`, a README section on running, importing and backing up.
9. **Live run** with the owner's data: dry run first, then the real import, then a second run. Record the report summary (counts only, no personal data) in `STATUS.md`.

## 6. Contracts

Honour: the mapping table and idempotency rules in ARCHITECTURE.md 9.1; the export format in 9.2; services are called with `Origin::Import`, never raw SQL from the importer (except through `saldo-app`).

Provide:

- CLI: `saldo-import firefly --url --token-env <VAR> --household <id|name> [--dry-run] [--only <list>] [--since <date>] [--allow-mismatch]`.
- A trait or plain function signature that every entity importer follows, so Phase 5 can add five more:

```rust
pub async fn import_<entity>(cx: &ImportCx, report: &mut ImportReport) -> Result<(), ImportError>;
// ImportCx: firefly client, App, HouseholdCtx, ExternalRefs, dry_run flag
```

- `ExternalRefs::resolve(entity_type, firefly_id) -> Option<local id>` for later importers that reference accounts, categories and splits.

## 7. Rust concepts to explain

| Concept | Where | Analogy |
|---|---|---|
| serde derive, `#[serde(...)]` attributes, `Option` for nullable JSON | Firefly models | Go struct tags with `encoding/json`; Laravel API resources in reverse |
| Generics with trait bounds | `list<T: DeserializeOwned>` | Go generics with a constraint; PHP has only docblock templates |
| `clap` derive | CLI | Artisan command signatures / Go's `cobra`, generated from a struct |
| Error enums with `thiserror`, `#[from]` | `ImportError` | Go sentinel/wrapped errors; PHP exception classes, but as one enum |
| Iterators and closures (`map`, `filter`, `collect`) | mapping code | Laravel collections; Go's `for range` loops written as a pipeline |
| `String` vs `&str` | parsing Firefly fields | An owned string versus a borrowed view of one; Go's `string` versus a slice into a buffer |
| Streams / async iteration (only if used) | pagination | A Go channel you range over; a PHP generator |

## 8. Acceptance criteria

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
SQLX_OFFLINE=true cargo test --workspace
cargo llvm-cov --workspace --summary-only        # >= 80%
docker compose build && docker compose up -d && curl -f localhost:8080/healthz
```

- The double-import test passes on fixtures.
- Against the live instance: second run reports no changes; balance check passes, or every mismatch is explained in `STATUS.md`.
- The token never appears in logs, the report or `import_runs.summary`.
- `saldo export json` output contains every account and every split of the imported household (counts match the database).

## 9. Hand-off

In `docs/STATUS.md`: Phase 3 done with commit hash; Firefly findings; import counts; known gaps (skipped items and why); how to run the importer inside the container; the entity-importer signature Phase 5 must follow.
