# Saldo status

Every agent reads this file before starting and updates it before stopping. Keep entries short and factual. Newest entries go at the top of each section.

## Phase state

| Phase | State | Branch / last commit | Notes |
|---|---|---|---|
| 0 Scaffold | not started | | |
| 1 Core | not started | | Needs the owner's explicit go-ahead |
| 2 Server | not started | | |
| 3 Importer + export | not started | | |
| 4 Web | not started | | |
| 5 Planning features | not started | | |
| 6 Events | not started | | |
| 7 Clients | not started | | |

States: not started, in progress, blocked (say on what), done.

## Deviations from ARCHITECTURE.md

Record every place where the code differs from the architecture document, why, and whether the owner approved it. Update `ARCHITECTURE.md` in the same commit.

- none yet

## Decisions made during implementation

Choices the briefs left open (crate picked, naming, a trade-off), with one line of reasoning each, so later phases follow them.

- 2026-11-11 Planning: app named Saldo; UUIDv7 over ULID; React + TypeScript SPA with PWA and Tauri 2; SQLite only; everything in a household is visible to every member.

## Open questions for the owner

- Phase 3 will need the URL and a personal access token of the Firefly III instance, and its version number.

## Findings about Firefly III

Facts checked against the owner's instance (Phase 3 fills this in; see ARCHITECTURE.md section 9.1 for the list of unconfirmed points).

- none yet
