# Saldo

Self-hosted personal finance server in Rust, replacing Firefly III.
Splits with their own accounts, bill reminders until paid, signed webhooks and a shared household.
See `docs/ARCHITECTURE.md` for the design and `docs/STATUS.md` for progress.

## Run with Docker

```sh
cp .env.example .env            # set SALDO_PUBLIC_URL to the address you open in the browser
docker compose up -d --build
curl -f http://localhost:8080/healthz
```

Everything Saldo stores lives in the `saldo-data` volume: the database `saldo.db` (with its WAL files) and `attachments/`.

Create the first user and household, either with `POST /api/v1/setup` or in the container (the password is read from standard input):

```sh
echo 'a long password' | docker compose exec -T saldo \
  saldo user create --email you@example.com --name You --household Home --currency EUR
```

## Import from Firefly III

The importer only reads from Firefly; nothing there is changed. It can run while the server runs, and running it again updates what changed and leaves the rest alone. Pass the token for one command only:

```sh
read -rs FIREFLY_TOKEN && export FIREFLY_TOKEN     # paste the personal access token
docker compose exec -T -e FIREFLY_TOKEN saldo \
  saldo-import firefly --url https://firefly.example.com --household Home --dry-run
docker compose exec -T -e FIREFLY_TOKEN saldo \
  saldo-import firefly --url https://firefly.example.com --household Home
unset FIREFLY_TOKEN
```

Options: `--only accounts,transactions` (any of currencies, accounts, categories, tags, budgets, bills, piggy_banks, rules, recurrences, transactions, attachments, piggy_bank_events), `--since YYYY-MM-DD`, `--allow-mismatch` (exit 0 even if a balance differs from Firefly). Every run ends with a summary and a balance check of every asset and liability account; it is also stored in the `import_runs` table.

## Export and backup

```sh
docker compose exec saldo saldo export json --household Home --out /data/export.json
docker compose exec saldo saldo export csv  --household Home --out /data/export.zip
docker compose exec saldo saldo backup /data/backup-$(date +%F).db     # consistent copy, safe while running
docker compose cp saldo:/data/backup-$(date +%F).db .
```

The same exports are available to the household owner at `GET /api/v1/export/json` and `GET /api/v1/export/csv`.

For regular backups, `scripts/backup.sh <folder>` (run from the compose folder) writes a consistent copy of the database and a tarball of `attachments/` to a folder on the host and removes copies older than `KEEP_DAYS` (default 14). A nightly cron entry:

```sh
15 3 * * * cd /opt/saldo && scripts/backup.sh /srv/backups/saldo >> /var/log/saldo-backup.log 2>&1
```

### Copy an instance into a fresh one (snapshot)

Without shell access to the server, the owner can download a snapshot of the whole instance (database and attachment files) and restore it into a fresh Saldo, for example to try a new version against real data. Set `SALDO_SNAPSHOT_DOWNLOAD=true` on the server (it is off by default; the snapshot holds everything, password hashes and secrets included, and is offered only while the instance has a single household), then:

```sh
curl -fH "Authorization: Bearer $SALDO_TOKEN" -o snapshot.zip https://money.example.com/api/v1/export/snapshot
```

On the new instance, before its first start (or after removing its volume):

```sh
docker compose run --rm -v "$PWD/snapshot.zip:/tmp/snapshot.zip:ro" saldo restore /tmp/snapshot.zip
docker compose up -d
```

`saldo restore` refuses to overwrite an existing database. It removes sessions and API tokens, webhooks and notification channels from the copy, so it never calls the original's webhooks or sends reminders to your phone; log in again with your password.

To restore a backup: stop the container, put the database copy in the volume as `saldo.db` (delete any `saldo.db-wal` and `saldo.db-shm` beside it), unpack the attachments tarball into the volume, and start it again.

## Log in with Authentik

Saldo can log people in through Authentik (or any OpenID Connect provider). In Authentik create an OAuth2/OpenID provider (confidential client, any signing certificate: RSA, or Authentik's default ECDSA P-384 one, whose `ES256` label Saldo handles, redirect URI `<SALDO_PUBLIC_URL>/api/v1/auth/oidc/callback`) and an application for it, then set `SALDO_OIDC_ISSUER` (for example `https://sso.example.com/application/o/saldo/`), `SALDO_OIDC_CLIENT_ID`, `SALDO_OIDC_CLIENT_SECRET` and usually `SALDO_OIDC_TRUST_EMAIL=true`. People log in when their email at the provider is the email of a household member; the desktop app does the same through the system browser. `SALDO_PASSWORD_LOGIN=false` then turns password login off.

## Releases

Push a tag `vX.Y.Z` to release a version:

- `.github/workflows/docker.yml` publishes the server image `X.Y.Z`, `X.Y` and `latest` to Docker Hub;
- `.github/workflows/desktop-release.yml` builds the desktop app for macOS (Apple Silicon and Intel), Windows and Linux, with the version taken from the tag, and attaches the installers to a **draft** GitHub Release named `Saldo X.Y.Z`. Check it and press Publish.

Running "Desktop release" by hand builds the same installers as workflow artifacts, without a release. The installers are unsigned.

Installed desktop apps check for a newer published release at start (and from **More > Check for updates**) and install it after you confirm. That needs a public repository and two repository secrets made once with `cd desktop && npx tauri signer generate -w ~/.tauri/saldo-updater.key`: `TAURI_SIGNING_PRIVATE_KEY` (the contents of the key file) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Its public half is `plugins.updater.pubkey` in `desktop/src-tauri/tauri.conf.json`. Keep the private key safe: without it, installed apps cannot be updated.

## Images

`.github/workflows/docker.yml` publishes the image to Docker Hub for amd64 and arm64 on every push to `main` (tags `main`, `sha-…`) and on `vX.Y.Z` tags (`X.Y.Z`, `X.Y`, `latest`). Set the repository secrets `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN` (and optionally the variable `DOCKERHUB_IMAGE`, default `<username>/saldo`).

## Apps

**Phones (Android and iOS).** Open Saldo in the browser and install it: Chrome on Android offers "Install"; on iPhone and iPad use Safari, Share, "Add to Home Screen" (iOS 16.4 or later). Then open the installed app, go to Notifications (the bell) and enable notifications in this browser to receive bill reminders. Web Push needs Saldo on https.

**Desktop (macOS, Windows, Linux).** The desktop app is the same web app in a small native window, with the login kept in the system keychain and bill reminders as system notifications. Allow its origin on the server first, in `.env`:

```sh
SALDO_ALLOWED_ORIGINS=tauri://localhost,http://tauri.localhost
```

Download it from the repository's **Releases** page (macOS for Apple Silicon and Intel, Windows, Linux), or build it yourself (needs Rust, Node and the [Tauri prerequisites](https://tauri.app/start/prerequisites/)):

```sh
cd web && npm ci && cd ../desktop && npm ci && npm run tauri build
# macOS: desktop/src-tauri/target/release/bundle/macos/Saldo.app (and a .dmg)
```

On first start it asks for the server's address and checks it, then shows the login.

The builds have no Apple or Windows certificate. The macOS app is signed ad-hoc (without that, Apple Silicon Macs call a downloaded app "damaged"), so macOS blocks it once: open **System Settings > Privacy & Security** and click **Open Anyway**, or run `xattr -dr com.apple.quarantine /Applications/Saldo.app`. Windows SmartScreen asks for **More info > Run anyway**.

## Development

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --workspace
cargo run -p saldo-server -- serve       # binary `saldo`; settings in docs/STATUS.md
cargo run -p saldo-importer -- --help    # binary `saldo-import`
cd desktop && npm ci && npm run dev      # desktop app against the Vite dev server
```

## License

MIT, see `LICENSE`.
