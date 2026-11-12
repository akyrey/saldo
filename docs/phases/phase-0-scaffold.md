# Phase 0: scaffold

## 1. Goal

An empty but working Cargo workspace under git, with lints and CI, so every later phase starts from a green build. When finished, `cargo test --workspace` passes on four crates that contain nothing but a placeholder.

## 2. Read first

- `CLAUDE.md`
- `docs/ARCHITECTURE.md` section 3 (Crates)

## 3. Prerequisites

None. The repository contains only `CLAUDE.md` and `docs/`. Rust 1.99 is installed.

## 4. Scope

In scope: git, workspace, crate skeletons, lint configuration, CI workflow, `.gitignore`, `README.md` with a three-line description and the commands.

Out of scope: any domain code, any dependency beyond what the skeleton needs, Docker (Phase 3), `web/` (Phase 4).

## 5. Work packages

1. **Git**: `git init`, default branch `main`, `.gitignore` (`target/`, `*.db`, `*.db-wal`, `*.db-shm`, `.env`, `node_modules/`, `web/dist/`). First commit contains the existing `CLAUDE.md` and `docs/`.
2. **Workspace**: root `Cargo.toml` with `[workspace]`, `resolver = "3"`, `members = ["crates/*"]`, `[workspace.package]` (edition 2024, version 0.1.0, license of the owner's choice: ask), and an empty `[workspace.dependencies]`.
3. **Crates**: `crates/core` (lib, package `saldo-core`), `crates/app` (lib, `saldo-app`, depends on `saldo-core`), `crates/server` (bin, `saldo-server`, binary name `saldo`, depends on `saldo-app`), `crates/importer` (bin, `saldo-importer`, binary name `saldo-import`, depends on `saldo-app`). Each has one trivial test so the test runner has something to run.
4. **Lints**: `[workspace.lints]` with `unsafe_code = "forbid"` and clippy `unwrap_used = "warn"`, `expect_used = "warn"`; each crate has `[lints] workspace = true`. Add `rustfmt.toml` only if a non-default setting is needed (prefer defaults). Add `rust-toolchain.toml` pinning the stable channel with `rustfmt` and `clippy` components.
5. **CI**: `.github/workflows/ci.yml` running on push and pull request: `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, with cargo caching.

## 6. Contracts

- Package and binary names exactly as in ARCHITECTURE.md section 3.
- Dependency direction `core` ← `app` ← `server` / `importer`.

## 7. Rust concepts to explain

- **Cargo workspace**: several packages sharing one lock file and one `target/` directory. Like a Composer monorepo with path repositories, or a Go workspace (`go.work`) with several modules.
- **Crate vs package vs module**: a package is what `Cargo.toml` describes (a Composer package / Go module); a crate is one compiled unit, a library or a binary (a Go package compiled to one archive or one `main`); a module is a namespace inside a crate (a PHP namespace).
- **lib vs bin crates**: `src/lib.rs` is a library, `src/main.rs` a program; like a Go package versus `package main`.

## 8. Acceptance criteria

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
cargo run -p saldo-server      # prints a placeholder line and exits 0
git status                     # clean
```

## 9. Hand-off

In `docs/STATUS.md`: set Phase 0 to done with the commit hash, and record the chosen license and edition under "Decisions".
