# Phase 4: web app basics

## 1. Goal

A React single-page app, served by the Saldo server, that covers daily use of the ledger: log in, see accounts and balances, browse and filter transactions, and create or edit a transaction with several splits, each with its own source and destination. It is installable as a PWA on a phone.

## 2. Read first

- `CLAUDE.md`, `docs/STATUS.md`
- `docs/ARCHITECTURE.md` sections 6 (API conventions and the Phase 2 and 3 endpoints), 7 (auth, CSRF header), 10 (frontend), 12
- `web/openapi.json`

## 3. Prerequisites

Phase 2 is done and `web/openapi.json` is current. Node LTS is installed. Phase 3 is not required, but imported data makes manual testing much better.

Confirm the stack with the owner before the first commit. The plan of record: React + TypeScript + Vite, TanStack Query, TanStack Router, shadcn/ui on Tailwind, `openapi-typescript` + `openapi-fetch`, `vite-plugin-pwa`, Vitest, Playwright.

## 4. Scope

In scope: project setup; generated API client; auth screens; app shell with navigation and the mine / member / household scope switch; accounts list and detail; transaction list with filters; the transaction editor; categories and tags management; export download; embedding the build into the server; Docker build stage; PWA manifest and service worker (app shell caching only).

Out of scope: budgets, bills, piggy banks, recurrences, rules, reports screens (Phase 5); webhook and notification settings, live updates over SSE (Phase 6); Web Push, Tauri (Phase 7); offline data.

## 5. Work packages

1. **Setup** (`web/`): Vite + React + TypeScript strict mode, ESLint, Prettier, Tailwind, shadcn/ui, Vitest; npm scripts `dev`, `build`, `lint`, `typecheck`, `test`, `gen:api`; dev proxy from `/api` to the local server. Add the web jobs to CI.
2. **API layer** (`web/src/api/`): types generated from `web/openapi.json`; one `openapi-fetch` client with a configurable base URL, the `X-Saldo-Csrf` header, and 401 handling; a `Money` helper that formats and parses decimal strings without ever converting to a JS float for arithmetic (use integer minor units or a decimal library).
3. **Auth and shell**: first-run setup screen, login, logout, current-user query, protected routes, layout with navigation, scope switch stored in the URL.
4. **Accounts**: list grouped by type with balances, detail page with the account's transactions, create/edit forms, archive.
5. **Transactions list**: paginated table with date range, account, category, tag, kind and text filters reflected in the URL; groups with several splits shown expanded.
6. **Transaction editor**: a form for a group with one or more split rows; each row has date, description, source account, destination account, amount, optional foreign amount, category, tags. The kind is shown per row as soon as both accounts are chosen. Server validation errors (`details[].field` such as `splits[1].amount`) are shown on the matching field. This is the headline feature: test it well.
7. **Categories, tags, export**: simple management pages and an owner-only export page with download buttons.
8. **Serve from the binary**: `rust-embed` in `saldo-server` behind a cargo feature, fallback to `index.html` for non-API paths, correct cache headers for hashed assets; add the Node build stage to the Dockerfile.
9. **PWA**: manifest, icons, service worker caching the app shell; installable on Android and iOS; a visible "offline" state instead of broken requests.
10. **End-to-end tests** (Playwright against a server with a temporary database): setup and login; create a three-split transaction with different sources and destinations and see three balances change; a validation error is displayed on the right row.

## 6. Contracts

- The SPA talks to the server only through `web/src/api/`; nothing else builds URLs. The base URL must stay configurable for Phase 7.
- Amounts stay decimal strings on the wire and are never passed through `Number` arithmetic.
- If the API is missing something the UI needs, add it in `saldo-server` following ARCHITECTURE.md section 6, regenerate `web/openapi.json`, and note it in `STATUS.md`. Do not work around it client-side.
- Structure by feature: `web/src/features/<accounts|transactions|...>/`, shared UI in `web/src/components/`. Phases 5 and 6 add feature folders and routes.

## 7. Concepts to explain

This phase is TypeScript, so explain by analogy to what the owner knows from Laravel and Go:

- **Generated client**: like generating Go client code from a proto/OpenAPI file; the compiler tells you when the server contract changed.
- **TanStack Query**: a cache of server responses keyed by request, with automatic refetching; closest to Laravel's cache `remember()` plus invalidation, living in the browser.
- **Rust side**: cargo features (`embed-web`) are like Go build tags; `rust-embed` is Go's `embed.FS`.

## 8. Acceptance criteria

```sh
cd web && npm run lint && npm run typecheck && npm test && npm run build
npx playwright test
cd .. && SQLX_OFFLINE=true cargo test --workspace
docker compose build && docker compose up -d   # the UI loads at http://localhost:8080
```

- The three Playwright flows pass.
- Lighthouse PWA installability check passes; the app installs from Chrome on Android.
- The production JS bundle for the initial route is under 300 kB gzipped.
- Both household members can log in and see each other's changes after a reload.

## 9. Hand-off

In `docs/STATUS.md`: Phase 4 done with commit hash; the confirmed stack and versions; any API additions; the folder convention; how to run the dev setup (two commands).
