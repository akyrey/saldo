# Phase 7: desktop and mobile clients

## 1. Goal

The same web app, delivered as a desktop application and as a polished installable mobile app, both talking to the one Saldo server. When finished, the owner has a Tauri desktop app on their computer that connects to their server and shows native notifications, and both household members have Saldo installed on their phones from the browser, receiving bill reminders as push notifications.

## 2. Read first

- `CLAUDE.md`, `docs/STATUS.md` (Phase 4 conventions, Phase 6 push configuration)
- `docs/ARCHITECTURE.md` sections 7 (bearer auth), 9.2 (backup), 10 (desktop notes), 11
- `web/src/api/` (base URL handling) and the service worker from Phases 4 and 6
- Tauri 2 documentation: check current APIs before using them, since they change between minor versions

## 3. Prerequisites

Phases 4 and 6 are done. The server is reachable over HTTPS (Web Push and service workers require it outside localhost). Tauri's system prerequisites are installed for the owner's OS.

## 4. Scope

In scope: Tauri 2 desktop shell; server-URL onboarding; bearer-token auth for the desktop app with the token in the OS keychain; native desktop notifications; PWA install experience and push permission flow on Android and iOS; `saldo backup`; release builds.

Out of scope: Tauri mobile, app stores, auto-update infrastructure, offline data. Record them in `STATUS.md` as possible follow-ups if the owner asks.

## 5. Work packages

1. **Client runtime split** (`web/src/platform/`): one small interface for what differs between browser and desktop: base URL, token storage, notifications, "open external link". A browser implementation (same origin, cookie session) and a Tauri implementation. The rest of the SPA uses only this interface.
2. **Desktop shell** (`desktop/`): Tauri 2 project that loads the `web/` build, with a minimal capability set (only the plugins used), window state persistence, app icons.
3. **Onboarding and auth**: first-run screen asking for the server URL (validated against `/healthz` and the API version), login that obtains a token (`POST /auth/login` with a `client: "desktop"` flag returning the token in the body instead of a cookie), stored through the keychain plugin; logout revokes it. Server side: CORS allow-list for the Tauri origin, configured by `SALDO_ALLOWED_ORIGINS`.
4. **Desktop notifications**: the desktop app listens to the SSE stream and shows native notifications for `bill.due_soon` and `bill.overdue`, with a click opening the bill.
5. **PWA polish**: install prompt with instructions for iOS ("Add to Home Screen"), push permission asked only after an explicit user action, subscription registered as a `webpush` notification channel, app badge for overdue bills where supported, mobile layout pass on the transaction editor.
6. **Backup command**: `saldo backup <path>` using `VACUUM INTO`, plus a documented cron/compose example that also copies `attachments/`.
7. **Release builds**: CI job building the desktop app for the owner's OS as an artifact; a README section on installing the desktop app and the PWA.

## 6. Contracts

- No API URL or `fetch` call outside `web/src/api/`; no direct `window.__TAURI__` access outside `web/src/platform/`.
- The desktop app uses bearer tokens only; cookies and the CSRF header remain the browser's mechanism.
- CORS is off unless `SALDO_ALLOWED_ORIGINS` is set.
- The Tauri capability file lists only what is used; no shell or filesystem access beyond what the features need.

## 7. Concepts to explain

| Concept | Where | Analogy |
|---|---|---|
| Tauri commands (`#[tauri::command]`) | keychain, notifications | A controller method callable from the frontend; an RPC handler in Go |
| Capabilities and permissions | `desktop/src-tauri/capabilities/` | An allow-list of what the web code may call, like OAuth scopes for the local machine |
| System webview vs bundled Chromium | choosing Tauri | Tauri reuses the OS browser engine, like a PHP app relying on the system's nginx instead of shipping its own |
| Service worker lifecycle | PWA | A background script the browser keeps for your origin; closest to a tiny queue worker living in the browser |
| `cfg` attributes / platform-specific code | desktop Rust code | Go build tags (`//go:build darwin`) |

## 8. Acceptance criteria

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
SQLX_OFFLINE=true cargo test --workspace
cd web && npm run lint && npm run typecheck && npm test && npx playwright test
cd ../desktop && npm run tauri build
```

Manual, recorded in the hand-off:

- Desktop app: fresh install → enter server URL → log in → create a transaction → quit and reopen (still logged in) → a bill reminder appears as a native notification.
- Android and iOS: install from the browser, enable notifications, receive a test push with the app closed.
- `saldo backup` produces a file that a fresh server starts from with all data present.
- Desktop app idle memory is recorded in `STATUS.md` (expected well under 200 MB).

## 9. Hand-off

In `docs/STATUS.md`: Phase 7 done with commit hash; platforms tested; known platform limits (for example iOS push requirements); follow-ups the owner may want (auto-update, Tauri mobile, OIDC).
