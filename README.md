# Saldo

Self-hosted personal finance server in Rust, replacing Firefly III.
Splits with their own accounts, bill reminders until paid, signed webhooks and a shared household.
See `docs/ARCHITECTURE.md` for the design and `docs/STATUS.md` for progress.

## Commands

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --workspace
cargo run -p saldo-server        # binary `saldo`
cargo run -p saldo-importer      # binary `saldo-import`
```

## License

MIT, see `LICENSE`.
