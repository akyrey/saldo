# Phase 6 part B hand-off: bill reminders and notifications

Branch `phase-6b-reminders`, built on f336043 (Phase 5 merge). Not merged and not pushed. `docs/STATUS.md` is left for the parent to update from this file.

## What was done (brief items 10–13)

- **Bill scheduler** (`crates/app/src/bills/scheduler.rs`, `crates/server/src/workers/reminders.rs`). `BillService::remind(ctx, today)` creates the missing periods, then calls `reminder_due` and `record_reminder` for each open period. `run_reminders(identity, bills)` does that for every household, using the household's own "today". If one household fails, the error is logged and the others still run. The task `bill_scheduler` runs at startup, every 15 minutes, and after writes. When it records reminders it wakes the notifier. `record_reminder` now returns `bool` (whether it recorded one).
- **Senders** (`crates/server/src/notify/`). The `Notifier` trait uses `#[async_trait]` so it can sit behind `Box<dyn Notifier>`. `Senders` picks the sender by channel kind and gives each send a 15 s timeout. There are four senders:
  - ntfy: JSON publish to the server root, with an optional bearer token.
  - Gotify: `POST /message` with the token in the `X-Gotify-Key` header.
  - email: `lettre` 0.11 with rustls and aws-lc-rs.
  - Web Push: `web-push-native` 0.5 builds the RFC 8291 encrypted request and the VAPID JWT; the request is sent with our own `reqwest` client (10 s timeout, no redirects). I picked it over `web-push` 0.11, which pulls in OpenSSL through `ece` and brings its own HTTP client. `cargo tree` shows no OpenSSL or native-tls.

  The message text is built in `saldo_app::notifications::Message::for_event` and gives the bill name, the amount or amount range, the due date ("due in N days / tomorrow / today") and the days overdue.
- **Notifier** (`crates/app/src/notifications/outbox.rs`, `crates/server/src/workers/notifier.rs`). In one transaction, `fan_out` reads the events after the `notifications` cursor and inserts one row per active channel of every current household member. The dedupe key is `<type>:<period id>:<reminder day>`, and `ON CONFLICT DO NOTHING` drops repeats. The same transaction moves the cursor forward. Then each pending notification of an active channel gets one try per run. A send has 4 attempts in total (the first plus three retries), then the row is `failed`. A failure on one channel only updates that channel's row. The task `notifier` runs every 60 s and after writes.
- **API** (`crates/server/src/routes/notifications.rs`). New endpoints:
  - `GET/POST /notification-channels`, `PATCH/DELETE /notification-channels/{id}`: each user sees and manages only their own channels; another user's channel answers 404.
  - `POST /notification-channels/{id}/test`: returns `{ sent, error }`.
  - `GET /push/vapid-key`: returns `{ public_key | null }`.

  Responses show `settings` (server URL, topic, recipient, push service origin, `has_token`) and never the tokens, push endpoints or keys. Creating an email or Web Push channel answers 422 when the server lacks SMTP or VAPID settings. A PATCH merges `config` fields into the stored ones, so the client never has to send back a token it cannot see.

  `web/openapi.json` and `web/src/api/schema.d.ts` were regenerated.
- **CLI**: `saldo vapid-keys` prints a new key pair.
- **Web** (`web/src/features/notifications/`). The page at `/settings/notifications` is reached from a bell icon in the header (not the NAV, to keep the merge simple). It lists the user's channels, with send test, pause/resume and delete. It has an add form for ntfy, Gotify and email, with server field errors shown on the right field. It also has the "Enable notifications in this browser" flow: permission prompt, `pushManager.subscribe` with the VAPID key, then `POST` of a `webpush` channel. `web/public/push-sw.js` holds the `push` and `notificationclick` handlers (focus an open tab and navigate, or open a new one). Workbox loads it through `importScripts`. The bill form already had `remind_days_before` and `remind_every_days` from Phase 5.

## Deviations from ARCHITECTURE.md (all five approved by the owner on 2026-11-13)

1. **New event types** `notification_channel.created/updated/deleted`, whose data is the secret-free `ChannelView`. CLAUDE.md requires an event for every state change; 8.1 does not list these types.
2. **Notification bookkeeping appends no events.** This covers `notifications` rows, the cursor, and the sent/failed status. It works like webhook deliveries: delivery state, and events here would loop back into the notifier.
3. **The outbox is cross-household.** `fan_out`, `pending` and `mark_*` take no `HouseholdCtx`, because `consumer_cursors` holds one global cursor per consumer. Every row they write carries its event's household, and channels are looked up by that household only.
4. **Channels belong to a user.** Members cannot see each other's channels. This narrows "everything in a household is visible to every member" for this personal setting. Channel events still go into the household's event log, without secrets.
5. **Configuration additions:**
   - `SALDO_SMTP_TLS`.
   - `SALDO_VAPID_SUBJECT`.
   - `SALDO_VAPID_PRIVATE_KEY` (ARCHITECTURE.md 11 names it); the public key is derived from it.
   - The "retried three times" rule is read as 4 attempts in total.
   - Retries happen on the next notifier run (60 s, or earlier after a write); there is no backoff column.
6. A crash between a successful send and `mark_sent` sends that one notification again after restart. This is at-least-once delivery, the same as webhooks.

## Configuration (documented in `.env.example`)

| Variable | Meaning |
|---|---|
| `SALDO_SMTP_HOST` | Turns email on. Without it, email channels cannot be created. |
| `SALDO_SMTP_PORT` | Default 587 (starttls), 465 (tls), 25 (none). |
| `SALDO_SMTP_TLS` | `starttls` (default), `tls` or `none`. |
| `SALDO_SMTP_USER` / `SALDO_SMTP_PASSWORD` | Set both or neither. The password is never logged; `Debug` is redacted. |
| `SALDO_SMTP_FROM` | Required with a host, e.g. `Saldo <saldo@example.com>`. |
| `SALDO_VAPID_PRIVATE_KEY` | Base64url raw P-256 key from `saldo vapid-keys`. Turns Web Push on. |
| `SALDO_VAPID_SUBJECT` | `mailto:` or `https:` contact. Defaults to `SALDO_PUBLIC_URL` when that is https, and is required otherwise. |

Invalid values stop startup with the variable's name. A partial SMTP setup without a host is also refused.

## Verification

- `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` and `SQLX_OFFLINE=true cargo test --workspace` pass.
- In `web/`: `npm run lint`, `npm run typecheck` and `npm test` pass (95 Vitest tests, 12 of them new).
- Playwright: 9/9 pass, including a new notification-channel flow, after `npm run build` and `cargo build -p saldo-server --features embed-web`. The new flow reuses an earlier flow's session, because the suite already uses up the in-memory login rate limit (10 per 15 minutes).
- New tests:
  - Simulated month: one `bill.due_soon`, `bill.overdue` every `remind_every_days`, nothing after payment, repeat runs on the same day emit nothing, the household timezone decides "today", and every household is covered.
  - Outbox: one row per active channel of each member, paused channels and other households excluded, dedupe on replay, small batches, give-up after 4 attempts.
  - Notifier with fake senders: a failing channel does not block others, restarts and replays send nothing twice, a missing sender configuration is recorded as the error.
  - Senders: wiremock for ntfy, Gotify and Web Push (VAPID header, encrypted body, 410, 302 not followed, URL not in errors); a fake SMTP server for email.
  - Config validation and the API.
- Line coverage (`cargo llvm-cov`): 94.7% in total. By crate: core 99.6%, app 97.0%, server 90.3%, importer 86.7%. New files: 80–100%; email.rs is lowest at 80%, because the TLS branches of the SMTP transport are not exercised.
- Manual check on the real binary, with a local Python request catcher standing in for ntfy:
  - A bill due in two days was reminded right after creation. The catcher was down for the first send, so it failed; it was retried and arrived with the bearer token and the text "29.90–35.00 EUR is due in 2 days (2026-11-15)."
  - A bill due three days ago produced "800.00 EUR was due on 2026-11-10 (3 days overdue)." Linking a transaction marked the period paid.
  - The token never appeared in the server log, and SIGTERM stopped both tasks cleanly.

## Not verified manually

- Real ntfy.sh, Gotify, an SMTP provider over STARTTLS/TLS, and real browser push services. No external services were contacted. Web Push was checked against wiremock only, and the encrypted payload was not decrypted by a real browser.
- Overdue repeats over real days, and the `notificationclick` handler in a real browser. The service worker is blocked in Playwright, and the handlers are not unit-tested.

## Notes for the merge and for Phase 7

- Shared files touched, all additively:
  - `crates/app/src/lib.rs`: one `notifications` field.
  - `crates/server/src/routes/mod.rs`: one merge.
  - `crates/server/src/workers/mod.rs`: `start` spawns two tasks.
  - `state.rs`: a `senders` field.
  - `config.rs`: a `notify` field.
  - `router.tsx`: one route.
  - `AppLayout.tsx`: a bell link.
  - `vite.config.ts`: `importScripts`.
  - `cli.rs` and `main.rs`: `vapid-keys`.
  - `flows.spec.ts`: a module-level `session`.
- Web Push needs https (or localhost) and, on iOS, an installed PWA (16.4+). Phase 7's install prompt should point users to the notifications page after install.
- An expired subscription (HTTP 404/410) currently just fails four times. Deactivating the channel automatically would be a small follow-up.
