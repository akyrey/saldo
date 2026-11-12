# Phase 6: webhooks, bill reminders, notifications

## 1. Goal

Turn the event log (written since Phase 2) into things that happen outside the database. When finished: every domain event is delivered to the owner's webhook endpoints with an HMAC signature and retries; a bill produces one reminder a few days before it is due and a repeated reminder until it is paid, on ntfy/Gotify, email and Web Push; and an open browser tab updates live when the other household member changes something.

## 2. Read first

- `CLAUDE.md`, `docs/STATUS.md` (Phase 5 hand-off: the bill service signatures)
- `docs/ARCHITECTURE.md` sections 5.5, 6 (Phase 6 rows), 8 (all of it), 12
- `crates/app/src/events.rs` and one service that appends events

## 3. Prerequisites

Phase 2 is done: part A below can start. Phase 5's bills track is done: part B can start. Phase 4 is done for the UI packages.

## 4. Scope

In scope: migrations for the rest of section 5.5; the dispatcher, delivery worker, bill scheduler and notifier tasks; webhook and notification-channel endpoints; the SSE stream; settings screens; the service-worker push handler.

Out of scope: prompting to install the PWA and the Tauri desktop notifications (Phase 7); notification types other than bill reminders (the design allows more later); event retention or pruning.

## 5. Work packages

**Part A: webhooks and live stream (needs Phase 2)**

1. **Migrations**: `consumer_cursors`, `webhook_endpoints`, `webhook_deliveries`, copied from 5.5.
2. **Background task harness** (`crates/server/src/workers/`): a small helper that runs a named loop with an interval, a wake-up signal (`tokio::sync::Notify`, triggered after commits that append events), a `CancellationToken` for shutdown, and error logging that never kills the loop. All tasks take the `Clock`.
3. **Signing** (`saldo-core` or `saldo-app`, pure function): `sign(secret, timestamp, body) -> String` and `verify(...)`, HMAC-SHA256, constant-time comparison. Tests with fixed vectors. Document the verification recipe for receivers in `docs/WEBHOOKS.md`, with a short PHP and a Go example.
4. **Dispatcher**: reads events after the `webhooks` cursor in batches, inserts deliveries for matching endpoints (`event_types` supports exact names, `prefix.*` and `*`; origin `import` only if opted in), advances the cursor in the same transaction. Tests: matching rules; a crash between batches loses and duplicates nothing.
5. **Delivery worker**: picks due deliveries, POSTs with the headers from 8.2, 10 s timeout, no redirects, bounded concurrency; success, retry schedule with jitter, `dead` after 8 attempts, endpoint deactivation after 20 consecutive dead deliveries. Tests with `wiremock` and the fake clock: 2xx, 500 then 200, timeout, exhaustion, signature verifiable by the receiver.
6. **Webhook API**: CRUD (owner only, secret generated server-side and shown once), `test` (appends a `webhook.test` event for that endpoint), `rotate-secret`, deliveries list, `redeliver`.
7. **SSE**: `GET /events/stream` sending `{seq, type, entity_type, entity_id}` for the caller's household, resuming from `Last-Event-ID`, with keep-alive comments. Web: one subscription that invalidates the matching TanStack Query keys.
8. **Web**: webhook settings page (endpoints, event filter, delivery log with status and redeliver button).

**Part B: bill reminders and notifications (needs Phase 5 bills)**

9. **Migrations**: `notification_channels`, `notifications`.
10. **Bill scheduler task**: at startup and every 15 minutes, per household: `materialise_periods`, then for each open period `core::reminder_due` and `record_reminder`. Tests with the fake clock over a simulated month: exactly one `bill.due_soon`; `bill.overdue` every `remind_every_days`; nothing after the period is paid; restarting the task twice on the same day emits nothing new; household timezone decides "today".
11. **Channel senders**: a `Notifier` trait with implementations for ntfy, Gotify, SMTP email (`lettre`) and Web Push (VAPID); configuration validated at startup; message text built from the event (bill name, amount range, due date, days overdue).
12. **Notifier task**: reads events after the `notifications` cursor, inserts one `notifications` row per active channel of each household member with the dedupe key, sends, retries three times. Tests: dedupe on replay; one failing channel does not block the others.
13. **Notification API and web**: channel CRUD, `test` send, `GET /push/vapid-key`; settings page; service-worker `push` and `notificationclick` handlers and the browser subscription flow; per-bill reminder settings (`remind_days_before`, `remind_every_days`) in the bill form.

## 6. Contracts

Honour: header names, signature format and retry schedule in 8.2; reminder rules in 8.3; table definitions in 5.5.

Provide:

- `docs/WEBHOOKS.md`: event catalogue, payload shape, signature verification, retry behaviour.
- `trait Notifier { async fn send(&self, channel: &Channel, message: &Message) -> Result<(), NotifyError>; }` so more channel kinds can be added.
- All tasks are restart-safe: killing the process at any point and starting again neither loses nor duplicates deliveries or reminders (duplicates of a webhook POST are allowed; receivers deduplicate on the event ID).

## 7. Rust concepts to explain

| Concept | Where | Analogy |
|---|---|---|
| Spawning long-running tasks, `JoinHandle` | worker harness | `go func()` plus a `sync.WaitGroup`; a Laravel queue worker process, but inside the server |
| `tokio::select!` | loop waiting for interval, wake-up or shutdown | Go's `select` over channels |
| Channels and `Notify` | waking the dispatcher | Go channels; `Notify` is a channel carrying no data |
| Cancellation and graceful shutdown | `CancellationToken` | Go's `context.Context` cancellation |
| `async` in traits, `Box<dyn Notifier>` | channel senders | A Go interface with several implementations chosen at run time; Laravel notification channels |
| Lifetimes, where they appear | borrowed payloads in signing | The reference must not outlive what it points to; mostly the compiler infers it, and when it asks, name the relationship |
| Streams | SSE | A Go channel piped to an `http.Flusher`; a PHP generator feeding a streamed response |
| `Mutex` / interior mutability (only if needed) | shared worker state | Go's `sync.Mutex`, but the data lives inside the mutex so it cannot be touched unlocked |

## 8. Acceptance criteria

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
SQLX_OFFLINE=true cargo test --workspace
cargo llvm-cov --workspace --summary-only     # >= 80%
cd web && npm run lint && npm run typecheck && npm test
```

Manual, recorded in the hand-off:

- Create a webhook pointing at a local request catcher; create a transaction; the request arrives with a signature that the PHP or Go example in `docs/WEBHOOKS.md` verifies. Stop the catcher; deliveries retry; start it; they arrive.
- Create a bill due in two days with `remind_days_before = 3`: a reminder arrives on ntfy. Move the due date into the past: overdue reminders arrive; link a transaction: they stop.
- Two browsers logged in as the two members: a transaction created in one appears in the other without a reload.

## 9. Hand-off

In `docs/STATUS.md`: Phase 6 done with commit hash; which channels were verified for real; the VAPID and SMTP configuration variables; anything Phase 7 needs for Web Push on installed PWAs and desktop notifications.
