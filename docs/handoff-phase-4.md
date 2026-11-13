# Phase 4 hand-off (web app)

Written by the Phase 4 agent on branch `phase-4-web` instead of editing `docs/STATUS.md`, because Phase 3 was editing that file at the same time. Merge this into `STATUS.md` after both branches are merged, then delete this file.

## State

Phase 4 is done except for two parts that were moved out of scope because they depend on Phase 3: the export page (WP7) and the Docker build stage (WP8). See "Left undone".

## Commits (oldest first)

| Commit | What |
|---|---|
| 7b2963e | feat(server): `GET /setup` and `GET /accounts/balances` |
| d6f52e2 | chore(web): Vite, React, TypeScript, Tailwind, shadcn/ui, ESLint, Vitest |
| 369d80a | feat(web): generated API client, error envelope handling, Money helpers |
| 26cdc06 | feat(web): auth screens, shell, accounts, transaction editor, categories, tags |
| 133092c | feat(web): PWA icons and the initial-route bundle size check |
| a22f20c | feat(server): serve the web app from the binary (`embed-web` feature) |
| 46afa4c | fix(web): route guard handles a 401 before any session is known |
| c916d30 | test(web): Playwright flows |
| 6db9970 | ci(web): web job in `.github/workflows/ci.yml` |
| e373a23 | test(web): editor component tests |
| 9beb045 | test(web): whole-app tests against a fake API, coverage |
| 11ea554 | test(web): a second member sees the owner's changes |

## Stack and versions (confirmed by the owner)

React 19.3.1, TypeScript 5.9.3 (strict, plus `noUncheckedIndexedAccess`), Vite 8.3.8 with `@vitejs/plugin-react` 6.3.0, TanStack Query 5.107.1, TanStack Router 1.170.46 (code-based routes, lazy page components), shadcn/ui components (new-york style, written into `src/components/ui/`) on Tailwind 4.3.5 and `radix-ui` 1.8.1, `lucide-react` icons, `openapi-typescript` 7.13.0 + `openapi-fetch` 0.17.0, `vite-plugin-pwa` 2.0.0 (Workbox `generateSW`), Vitest 5.1.0 with jsdom and Testing Library, Playwright 1.65.0, ESLint 10.14 + typescript-eslint 8.72, Prettier 3.10. Rust: `rust-embed` 8.14 (optional).

- TypeScript is 5.9, not 7.0 (the current latest). TypeScript 7 is the new Go-based compiler, and `openapi-typescript` (peer `typescript ^5`) and `typescript-eslint` (`<6.1`) do not support it yet.
- npm 10.9 (bundled with Node 23) crashes with `Cannot read properties of null (reading 'edgesOut')` when it resolves vitest 5's optional peers on `npm install`. `npm ci` from the committed lock file works. To add or upgrade packages, use npm 11: `npx npm@11 install -D <pkg>`. CI uses Node 24.
- shadcn components were written by hand from the shadcn sources (the CLI is interactive); `components.json` is there so `npx shadcn add <component>` works later.

## API additions (in `saldo-server`, following ARCHITECTURE.md 6)

Add these two rows to the API table in ARCHITECTURE.md 6 when merging; the file was left alone to avoid conflicts.

- `GET /api/v1/setup` (public): `{ "data": { "needed": true } }` while no user exists. The SPA uses it to choose between the setup and login screens. `IdentityService::needs_setup()` in `crates/app/src/identity/status.rs`.
- `GET /api/v1/accounts/balances?type=&active=&scope=&date=&page=&per_page=`: a page of `BalanceView`s for the same accounts and order as `GET /accounts` with the same filters. Without it the account list needs one request per account. `AccountService::balances()` in `crates/app/src/accounts/balances.rs`; `AccountService::list` and `balance` now share helpers with it (`list_rows`, `balance_view`). No new SQL.
- Tests: `crates/server/tests/web_api.rs`; `crates/server/tests/openapi.rs` lists `GET /api/v1/setup` as public; `web/openapi.json` regenerated.

## Serving the app from the binary (WP8 without Docker)

- `cargo build -p saldo-server --features embed-web` embeds `web/dist` (run `npm run build` first). Without the feature the server is API-only, as before. `rust-embed` uses `debug-embed`, so debug builds embed too.
- `crates/server/src/spa.rs`: the fallback for paths no route matched. `/api/...` and `/healthz` keep the JSON 404; other `GET`/`HEAD` paths serve the file, or `index.html` for extensionless paths (app routes), or 404 for missing files with an extension. `assets/*` (hashed) get `Cache-Control: public, max-age=31536000, immutable`; everything else (`index.html`, `sw.js`, the manifest) gets `no-cache`. All responses get `X-Content-Type-Options: nosniff`. The logic takes a lookup function, so its unit tests run in every build; `crates/server/tests/web_embed.rs` runs only with the feature.
- `server.rs` changed by one call: `crate::spa::with_web_app(router)`.
- Docker: the Node build stage must run `npm ci && npm run build` in `web/` before `cargo build --release --features embed-web`.

## Folder convention (`web/`)

- `src/api/`: the only code that knows URLs. `schema.d.ts` is generated (`npm run gen:api` from `web/openapi.json`); `client.ts` builds the `openapi-fetch` client (configurable base URL via `VITE_SALDO_API_URL`, default same-origin; sends `X-Saldo-Csrf: 1`; reports 401s) and `call()` turns the error envelope into an `ApiError`; `money.ts` does exact arithmetic on decimal strings with `bigint` minor units. ESLint forbids `parseFloat`.
- `src/features/<feature>/`: one folder per feature (`auth`, `shell`, `accounts`, `transactions`, `categories`, `tags`, plus `shared`). Each has `queries.ts` (TanStack Query options), pure logic in plain `.ts` files with tests next to them (`editor-model.ts`, `grouping.ts`, `account-form.ts`, `search.ts`), and page components. Phases 5 and 6 add folders and register routes in `src/router.tsx`.
- `src/components/`: shared UI; `src/components/ui/` holds the shadcn components.
- `src/test/`: Vitest setup, a fake API (`fakeApi`) and `renderApp(path)` for whole-app tests.
- `e2e/`: Playwright flows; `e2e/start-server.mjs` starts `target/debug/saldo` on a fresh temporary database.
- Scope switch: `?scope=mine|user:<id>` on every page inside the app (kept across links by the router); transaction filters (`from`, `to`, `account_id`, `category_id`, `tag_id`, `kind`, `q`, `page`) are URL search params too.
- Query keys: `["me"]`, `["accounts", ...]`, `["balances", ...]`, `["transactions", ...]`, `["categories"]`, `["tags"]`, `["members"]`, `["currencies"]`. Writes invalidate by prefix (Phase 6's SSE listener can do the same).

## Dev setup (two commands)

```sh
SALDO_DATABASE_URL=sqlite://$PWD/data/dev.db SALDO_BIND=127.0.0.1:8080 SALDO_PUBLIC_URL=http://localhost:5173 cargo run -p saldo-server -- serve
cd web && npm ci && npm run dev    # http://localhost:5173, /api is proxied to 127.0.0.1:8080
```

`SALDO_PUBLIC_URL` must be the Vite origin, because the CSRF check compares it with the browser's `Origin`. `SALDO_DEV_API` changes the proxy target. Checked with curl through the proxy: setup 201, login 200, cookie write 201.

## Test results (2026-11-13)

| Check | Result |
|---|---|
| `npm run lint` (ESLint + Prettier) | clean |
| `npm run typecheck` | clean |
| `npm test` | 61 tests pass, run three times; line coverage 89% of `src/` (`npm run coverage`), `api/` and the pure models 97 to 100% |
| `npm run build` | passes; service worker precaches 33 files |
| `npm run size` | initial route 129.9 kB gzipped for `/accounts` (128.0 kB for `/login`), budget 300 kB |
| `npx playwright test` | 5 pass, run four times: setup + logout + login (wrong password shown); a three-split transaction (Checking→Groceries, Credit card→Groceries, Salary→Savings) changes the balances to -42.50, -17.25, 100.00 (and Groceries 59.75); a 422 on `splits[1].amount` shows on split 2 only; a second member sees the owner's transaction and balances; manifest, icons and `sw.js` served with the right cache header |
| `SQLX_OFFLINE=true cargo test --workspace` | 195 pass |
| `cargo test -p saldo-server --features embed-web` | 46 pass |
| `cargo fmt --check`, `cargo clippy --all-targets -D warnings` (with and without `embed-web`) | clean |

To run Playwright locally: `cd web && npm run build && cargo build -p saldo-server --features embed-web && npx playwright install chromium && npx playwright test`.

## Decisions

- Routes are code-based (no router file generation), with every page lazy-loaded.
- Pickers use native `<select>` (shadcn style) rather than Radix Select: better on phones, lighter, and easy to drive in tests. Tags are toggle buttons.
- Amount input accepts a single comma as decimal separator (`12,50` → `12.50`); everything else is validated by the server, and its errors are shown on the field (`splits[i].field`) or the row (`splits[i]`).
- Kind badges are computed in the browser with the same table as `SplitKind::derive`; "Not allowed" appears for pairs the server will reject.
- On an account page, splits where the account is the source show a negative amount.
- The 401 listener sends the person to `/login?redirect=...` only when a session was known; otherwise the route guard decides between setup and login.
- The service worker caches the app shell only and never `/api`; an offline banner appears when the browser goes offline, and failed requests say "Cannot reach the server".
- Icons are rendered from `public/favicon.svg` with `node scripts/make-icons.mjs` (Playwright's Chromium); the maskable icon has a 10% safe zone.

## Left undone

- Export page (WP7): out of scope by the owner's instruction, it needs Phase 3's `/export` endpoints. Add `src/features/export/` with an owner-only route (`me.role === "owner"`) and download links built in `src/api/`.
- Docker build stage (WP8) and the `docker compose` acceptance line: out of scope by the owner's instruction (Phase 3 owns the Dockerfile).
- Lighthouse installability and installing from Chrome on Android were not checked here (no device; Lighthouse 12 dropped the PWA category). The manifest has name, `display: standalone`, `start_url`, 192 and 512 px icons plus a maskable one, and a service worker with a fetch handler, which is what Chrome requires. Check on a phone after deploying behind https.
- `docs/STATUS.md` and the ARCHITECTURE.md 6 table were not edited (see above).
- `npm audit` reports 15 advisories (13 moderate, 2 low) in dev-only dependencies such as `workbox-build`'s glob, minimatch and deepmerge; `npm audit --omit=dev` finds none, so nothing ships to the browser.
