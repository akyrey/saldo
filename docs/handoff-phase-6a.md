# Hand-off: Phase 6 Part A (webhooks and live stream)

Branch `phase-6a-webhooks`, based on `a653d05`. Work packages 3 to 8 of `docs/phases/phase-6-events.md`. Part B (bill reminders, notifications) is not included. `docs/STATUS.md` and `docs/ARCHITECTURE.md` are unchanged, as instructed: the lead should fold the decisions and deviations below into them.

## Commits

| Commit | What |
|---|---|
| `0bdce30` feat(app): sign and verify webhook payloads with HMAC-SHA256 | WP3: `saldo_app::webhooks::signing::{sign, signature_header, verify}`, fixed test vectors |
| `7131be7` feat(app): manage webhook endpoints, dispatch events and deliver them with retries | WP4, WP5, WP6 (service): `WebhookService` (CRUD, test, rotate, deliveries, redeliver), dispatcher, delivery worker |
| `f5f2951` feat(server): run the webhook dispatcher and delivery worker in the background | Registers both tasks in `workers::start` |
| `284fc58` feat(server): add the webhook API | WP6 routes |
| `f32c6d0` feat(server): stream household events over SSE at /events/stream | WP7 server side |
| `7e0a8f9` feat(web): refresh queries live from the household event stream | WP7 web side |
| `c983739` feat(web): add the webhook settings page with delivery log | WP8 |
| `3875be7` docs(webhooks): document events, payload, signature checks and retries | `docs/WEBHOOKS.md` |
| `f621292` test(server): run the webhook tasks end to end | Worker registration test |
| `4f6574b` test(web): check live updates across two members and the webhook page in Playwright | Manual check 3, automated |

## Where things are

- `crates/app/src/webhooks/`: `signing.rs` (pure HMAC), `matching.rs` (pure filter rules and input checks), `retry.rs` (pure schedule), `endpoints.rs` (owner-only service methods), `dispatch.rs`, `delivery.rs`, `repo.rs`. `App.webhooks` was added as the last `App` field.
- `crates/app/src/events.rs`: appended `EventNotice`, `EventService::latest_seq`, `EventService::notices_after`.
- `crates/server/src/routes/webhooks.rs`, `crates/server/src/routes/event_stream.rs`, `crates/server/src/workers/webhooks.rs`.
- `web/src/features/live/` (`live-events.ts`, `useLiveEvents.ts`), `web/src/features/webhooks/`, route `/webhooks`, owner nav entry "Webhooks".

## API additions

- `GET/POST /webhooks`, `GET/PATCH/DELETE /webhooks/{id}`, `POST /webhooks/{id}/test`, `POST /webhooks/{id}/rotate-secret`, `GET /webhooks/{id}/deliveries`, `POST /webhook-deliveries/{id}/redeliver`. All owner-only (403 for members).
  - `GET /webhooks` is unpaginated `{ "data": [...] }` (like tokens and members).
  - `POST /webhooks` → 201 `{ "data": { "webhook": WebhookView, "secret": "whsec_…" } }`; rotate-secret answers 200 with the same shape. The secret appears nowhere else (not in `GET`, not in events).
  - `POST /webhooks/{id}/test` → 202 `{ "data": { "event_id" } }`.
  - `PATCH` takes `name`, `url`, `event_types`, `include_import_events`, `active`; `active: true` on an inactive endpoint also resets `consecutive_failures` and clears `disabled_reason`.
- `GET /events/stream` (SSE): messages `id: <seq>` + `data: {"seq","type","entity_type","entity_id"}`; resumes from `Last-Event-ID` or `?after_seq=`; without either it starts at the household's newest event; keep-alive comment every 15 s.
- `web/openapi.json` and `web/src/api/schema.d.ts` regenerated.

## Decisions

- Signing lives in `saldo-app` (core may not depend on `hmac`). Crates: `hmac` 0.13 (workspace), `reqwest` in `saldo-app` (was importer-only), `futures-util` 0.3 in `saldo-server` (for `stream::unfold`), `wiremock` and tokio `test-util` as app dev-dependencies. Constant-time comparison is `Mac::verify_slice`.
- Secret: `whsec_` + 64 hex characters (256 random bits); the HMAC key is the whole string as UTF-8.
- Matching: `*`, `<entity>.*`, or exact `<entity>.<verb>` (lowercase letters, digits, `_`); 1 to 50 patterns, deduplicated; default `["*"]`.
- Endpoints receive only events with `occurred_at >= endpoint.created_at`. Without this, an endpoint created before the dispatcher's first run (or right after a restore) would receive the whole history. One side effect: an endpoint receives its own `webhook.created` event.
- `webhook.test` goes only to the endpoint named by its `entity_id`, ignoring that endpoint's filter, so "Send test" always reaches the URL.
- The dispatcher and delivery worker are system tasks without a `HouseholdCtx` (`WebhookService::dispatch`, `dispatch_batch`, `deliver_due`): the `webhooks` cursor is one global position in the event log. Every event only matches endpoints of its own household, and every ctx-facing method still filters by `household_id`.
- Dispatcher: batches of 200 events; each batch inserts deliveries (`INSERT OR IGNORE`) and moves the cursor in one transaction. Runs at startup, on every write (EventSignal), and every 60 s (for the importer CLI, which cannot signal the server process). After queueing deliveries it calls `EventSignal::notify()` to wake the delivery worker.
- Delivery worker: 50 due deliveries per round, 4 in flight (`JoinSet` + `Semaphore`), loops while rounds are full; runs on every signal and every 15 s (retries become due by time). 10 s timeout, `redirect::Policy::none()`, `User-Agent: Saldo-Webhook/<crate version>`. The HTTP client is built per run (building it is fallible; `App::new` is not).
- Retry: after failed attempt n (1 to 7) wait 1 min, 5 min, 30 min, 2 h, 6 h, 12 h, 24 h plus 0 to 10% random jitter; the 8th failure is `dead`. 20 dead in a row (counter reset by any success) deactivate the endpoint with `disabled_reason`, in the same transaction as a `webhook.updated` event with origin `system` and the household's first owner as actor.
- Deliveries of inactive endpoints stay `pending` and are not sent until the endpoint is active again.
- Delivery bookkeeping (attempt results, redeliver) appends no event: it is the outbox itself, and an event per attempt would feed back into webhooks. Endpoint CRUD, rotate-secret and test do append events (`webhook.created/updated/deleted/test`).
- Redeliver sets `status = pending`, `attempts = 0`, `next_attempt_at = now`; works for any delivery, including succeeded ones.
- `last_error` is short and URL-free (`HTTP 500 Internal Server Error`, `timed out after 10 s`, `connection failed`, or the error chain via `reqwest::Error::without_url`), because Home Assistant webhook URLs are secrets.
- SSE: `AppState` gained `shutdown: CancellationToken`, cancelled by `serve` when the shutdown signal arrives. Without it an open stream would keep axum's graceful shutdown waiting forever. Streams wake on the EventSignal, poll every 15 s as a fallback, and end on a database error (the browser reconnects with `Last-Event-ID`).
- Web live updates: one `EventSource` per tab, opened in `AppLayout` (`useLiveEvents`). Notices are batched for 250 ms, then mapped to query-key prefixes: account → accounts, balances; transaction → transactions, balances, accounts; category/tag → own key + transactions; household_member → members; household → me; webhook → webhooks. **Unknown entity types invalidate every query**, so Phase 5 entities refresh correctly before anyone adds their keys; add them to `KEYS` in `web/src/features/live/live-events.ts` to make it cheaper.
- Mobile bottom nav: the column count is now `repeat(nav.length, …)` instead of a hard-coded `grid-cols-4/5`, because owners now have 6 entries (Part B may add more).

## Deviations from ARCHITECTURE.md (for the lead to record)

1. Section 6: `GET /events/stream` also accepts `?after_seq=` (clients that cannot set `Last-Event-ID`), and starts at the newest event when neither is given.
2. Section 6: the response shapes above (`{ webhook, secret }` for create and rotate, 202 `{ event_id }` for test, unpaginated webhook list) are not spelled out there.
3. Section 8.2: added rules not in the document: endpoints skip events older than themselves; `webhook.test` bypasses the filter and reaches only its endpoint; pending deliveries of inactive endpoints wait; jitter is 0 to 10%.
4. Section 7 / CLAUDE.md "every service function takes a `HouseholdCtx` first": `dispatch`, `dispatch_batch` and `deliver_due` take none (global cursor; see Decisions). Not yet approved by the owner.
5. Section 8.1 "every state change appends an event": delivery status changes and redeliver append none (outbox bookkeeping, avoids a feedback loop). Not yet approved by the owner.

## Test results (2026-11-13)

- `cargo fmt --all --check`: pass. `cargo clippy --all-targets -- -D warnings`: pass.
- `SQLX_OFFLINE=true cargo test --workspace`: 276 passed, 0 failed. New: 16 unit tests (signing, matching, retry, secret), `crates/app/tests/webhooks.rs` (10: CRUD, owner-only, isolation, matching, test events, crash between batches, old events, inactive endpoints), `webhook_delivery.rs` (7, wiremock + `FixedClock`: 2xx with headers and a verifiable signature; 500 then 200 after the backoff; timeout; no redirects; 8 failures → dead, then redeliver; 20th dead deactivates with a system event; unreachable receiver), `event_notices.rs` (1), server `webhooks.rs` (3), `event_stream.rs` (3: new events, `Last-Event-ID` resume, shutdown ends streams), worker registration (1).
- `cargo llvm-cov --workspace --summary-only`: 95.28% lines workspace-wide; webhooks module files 93 to 100%.
- Web: `npm run lint`, `typecheck` pass; `npm test` 82 passed (13 new); `npm run build` passes; `npm run size`: initial route 130.9 kB gzipped (unchanged; the webhooks page is lazy). Playwright: 7 passed (1 new flow).

## Manual checks

1. **Signed delivery verified (pass).** Fresh database, `saldo serve` on 127.0.0.1:18080, a Python catcher on 127.0.0.1:18099 logging headers and raw body. Created a webhook (`transaction.*`, `webhook.test`) and a transaction via the API. One POST arrived with all 8.2 headers (`x-saldo-signature: t=1794570205,v1=34fa…`). The Go `Verify` function copied verbatim from `docs/WEBHOOKS.md` returned `verified: true`, and `false` for the same body with one byte appended. PHP was not run: no PHP binary on this machine. Note for receivers: header names arrive lowercase (HTTP/1.1 via reqwest), so look them up case-insensitively (Go's `http.Header.Get` does).
2. **Retry after the receiver comes back (pass).** Stopped the catcher and created a second transaction: the delivery was `pending`, attempt 1, `connection failed`, next attempt ~59 s later. Restarted the catcher: the delivery arrived at the next due run, `succeeded` on attempt 2 (73 s after the first try) with a valid signature.
3. **Live update across two members (pass, automated).** Playwright flow in `web/e2e/flows.spec.ts`: two browser contexts logged in as owner and member. The member's open Transactions page showed the owner's new transaction without a reload. The same flow creates a webhook in the UI, sees the `whsec_…` secret, sends a test and sees `webhook.test` in the delivery log.
4. **Extra checks.** SIGTERM with an open `curl -N` SSE stream: log shows `shutting down`, both webhook tasks stopped, `saldo stopped`; the process exited. Neither the API token nor the webhook secret appears in the server log.

## Left undone, and why

- PHP example not executed (no PHP installed); the Go example was.
- An open SSE stream does not re-check its session: a revoked session or removed member keeps receiving notices (IDs and types only, no data) until the connection drops. Checking the session on every wake-up would fix it; left out to keep the stream simple.
- The delivery log page shows deliveries but no response bodies (not stored by design).
- No `docs/STATUS.md` / `ARCHITECTURE.md` updates (lead's job, per the brief).

## Merge notes for Part B

Shared files touched, appending only: `crates/app/src/lib.rs` (`webhooks` module and `App.webhooks`), `crates/app/src/events.rs` (appended block), `crates/server/src/routes/mod.rs` (two modules, two `.merge` lines at the end), `crates/server/src/workers/mod.rs` (`start` now has `let mut workers` and `webhooks::register(&mut workers, state);`), `crates/server/src/state.rs` (`shutdown` field), `crates/server/src/server.rs` (graceful-shutdown closure), `web/src/router.tsx` (`webhooksRoute`), `web/src/features/shell/AppLayout.tsx` (`useLiveEvents()`, OWNER_NAV entry, dynamic grid columns), `web/e2e/flows.spec.ts` (new flow at the end), root `Cargo.toml` (`hmac`, `futures-util`). After merging, rerun `cargo sqlx prepare --workspace`, `saldo openapi > web/openapi.json` and `npm run gen:api`. Part B's notifier can reuse `consumer_cursors` with consumer `notifications` the same way `dispatch.rs` does.
