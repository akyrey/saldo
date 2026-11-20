# Saldo plan

Saldo is built in nine phases. Each phase has a self-contained brief in `docs/phases/` that an agent with no other context can execute. `docs/ARCHITECTURE.md` is the contract; `docs/STATUS.md` is the running log.

## Phases

| # | Brief | Delivers | Needs |
|---|---|---|---|
| 0 | [phase-0-scaffold.md](phases/phase-0-scaffold.md) | Git repo, Cargo workspace with four empty crates, lints, CI | nothing |
| 1 | [phase-1-core.md](phases/phase-1-core.md) | `saldo-core`: Money, IDs, accounts, groups and splits, kind derivation, validation, balances | 0 |
| 2 | [phase-2-server.md](phases/phase-2-server.md) | `saldo-app` + `saldo-server`: schema, services, event log, REST API, OpenAPI, auth, household | 1 |
| 3 | [phase-3-importer-export.md](phases/phase-3-importer-export.md) | Firefly III importer (ledger entities), JSON/CSV export, attachments, Docker image | 2 |
| 4 | [phase-4-web.md](phases/phase-4-web.md) | React SPA: login, accounts, transactions with the split editor, PWA shell | 2 |
| 5 | [phase-5-planning.md](phases/phase-5-planning.md) | Budgets, bills, piggy banks, recurring transactions, rules, reports; their import and UI | 2 (import parts need 3, UI parts need 4) |
| 6 | [phase-6-events.md](phases/phase-6-events.md) | Webhook delivery, bill reminders, notification channels, SSE stream | 2 (bill reminders need 5) |
| 7 | [phase-7-clients.md](phases/phase-7-clients.md) | Tauri 2 desktop app, PWA install and Web Push polish, backup command | 4, 6 |
| 8 | [phase-8-templates.md](phases/phase-8-templates.md) | Transaction templates, duplicating a transaction | 4, 5 |
| 9 | [phase-9-statements-and-forecast.md](phases/phase-9-statements-and-forecast.md) | Statement import, reconciliation, forecast, subscription suggestions, budget carry-over | 5, 8 |
| 10 | [phase-10-usability.md](phases/phase-10-usability.md) | Dark mode, more filters and saved views, shortcuts and command palette, alerts and digest, new reports, net worth goals and safe to spend | 9 (forecast, carry-over) |
| 11 | [phase-11-bill-history.md](phases/phase-11-bill-history.md) | Bill detail page: linked transactions, a payment chart over the bill's whole duration, and a payment summary | 5 (can run beside 10) |

## Order and parallel work

```
0 ── 1 ── 2 ──┬── 3 ───────────────┐
              ├── 4 ───────────────┼── 7
              ├── 5 (backend first)┤
              └── 6A (webhooks) ── 6B (bill reminders, needs 5) ─┘

8 (templates) needs 4 and 5 and can run beside 6 and 7.
```

- Phases 0, 1 and 2 are strictly sequential.
- After Phase 2, Phases 3, 4, the backend of 5 and part A of 6 can run at the same time. They mostly touch different files: 3 works in `crates/importer`, 4 in `web/`, 5 and 6 add new modules to `crates/app` and `crates/server`.
- Shared files where parallel agents will collide: the router and OpenAPI registration in `crates/server`, `crates/app/src/lib.rs`, `web/openapi.json`, and `docs/STATUS.md`. Each parallel agent works on its own branch (or git worktree) and rebases before merging; conflicts in those files are additive and simple.
- Migrations are created with `sqlx migrate add`, which timestamps the file name, so parallel branches do not clash. A merged migration is never edited.

## How to hand a phase to an agent

Start a fresh session in the repository root and give it this prompt, with the phase number filled in:

> Implement Phase N of Saldo. Read `CLAUDE.md`, `docs/STATUS.md` and `docs/phases/phase-N-*.md` first, then the sections of `docs/ARCHITECTURE.md` that the brief lists. Work through the work packages in order, one small commit each. Stop when the acceptance criteria pass, update `docs/STATUS.md`, and report.

Every brief has the same nine sections: Goal, Read first, Prerequisites, Scope, Work packages, Contracts, Rust concepts to explain, Acceptance criteria, Hand-off.

## Rules that apply to every phase

- Tests first: write the failing test, then the code.
- Before every commit: `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`.
- One work package is one or a few small commits, in conventional-commit format.
- The owner is learning Rust and knows PHP/Laravel and Go well. When a Rust concept appears for the first time, explain it in the session in two or three sentences with a PHP or Go analogy, and add a short comment in the code where it helps. Prefer simple, idiomatic code over clever code.
- If the brief and `ARCHITECTURE.md` disagree, or something in the architecture turns out to be wrong, stop and ask the owner. Record the outcome in `docs/STATUS.md`.
- Do not start work that belongs to a later phase.
