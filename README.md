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

Options: `--only accounts,transactions` (any of currencies, accounts, categories, tags, transactions, attachments), `--since YYYY-MM-DD`, `--allow-mismatch` (exit 0 even if a balance differs from Firefly). Every run ends with a summary and a balance check of every asset and liability account; it is also stored in the `import_runs` table.

## Export and backup

```sh
docker compose exec saldo saldo export json --household Home --out /data/export.json
docker compose exec saldo saldo export csv  --household Home --out /data/export.zip
docker compose exec saldo saldo backup /data/backup-$(date +%F).db     # consistent copy, safe while running
docker compose cp saldo:/data/backup-$(date +%F).db .
```

The same exports are available to the household owner at `GET /api/v1/export/json` and `GET /api/v1/export/csv`. To restore a backup, stop the container, put the file in the volume as `saldo.db`, and start it again.

## Development

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --workspace
cargo run -p saldo-server -- serve       # binary `saldo`; settings in docs/STATUS.md
cargo run -p saldo-importer -- --help    # binary `saldo-import`
```

## License

MIT, see `LICENSE`.
