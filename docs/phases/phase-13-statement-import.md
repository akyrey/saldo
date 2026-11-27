# Phase 13: bank and Satispay statement import with Amazon and PayPal matching

Requested by the owner on 2026-11-27. This brief replaces Track E of Phase 9 (`phase-9-statements-and-forecast.md`), which was never started. It keeps that track's design where it still holds and adds what the owner learned writing [firefly-iii-reconcile](https://github.com/akyrey/firefly-iii-reconcile) (Go, 2026-03 to 2026-11), a tool that matched Firefly III against Widiba and Mediobanca exports, Amazon orders, PayPal and Satispay. Section 3 lists those lessons; every rule in the work packages that comes from one names it (L1, L2, …).

Before starting a track, add its tables, endpoints and events to `docs/ARCHITECTURE.md` (sections 5, 6 and 8.1), since that file is the contract.

## 1. Goal

Less typing, better descriptions, and numbers the owner can trust. When finished, the owner can:

- **Import a bank or card statement** (xlsx, CSV, CAMT.053 or OFX), review every line with its proposed match, and commit it. Importing the same or an overlapping file again creates nothing new. A whole import can be undone.
- **Import Satispay exports as statements** of the Satispay asset accounts (one per person), like any other bank. A top-up imported from both the bank and the Satispay file ends up as one transfer.
- **Upload Amazon and PayPal exports** and upload newer ones whenever wanted. Saldo keeps their rows, adds only what is new and updates what changed, so later bank statements are matched against them without uploading them again.
- **See what a bank line paid for**: "AMAZON EU SARL 405-1234567-1234567" turns into the order's products and "PAYPAL *EUROPE" turns into the merchant. The new transaction takes the better description, keeps the bank's text in the notes, and runs the rules on the better text.
- **Split an Amazon charge by item**, so products in different categories land in different splits.
- **Check that a statement is complete** (opening balance plus lines equals closing balance) and compare month-end balances with Saldo's before reconciling.
- **Enrich existing history** (optional track): match the stored Amazon and PayPal rows against transactions already in Saldo (imported from Firefly) and accept better descriptions in bulk.

## 2. Read first

- `CLAUDE.md`, `docs/STATUS.md`
- `docs/ARCHITECTURE.md` sections 4.2 (balances and signs), 5.2 (`splits.reconciled`, `splits.meta`, `notes`), 5.4 (rules), 5.4c (reconciliations), 5.4f (credit card plans: imported card purchases count toward a plan), 5.6 (`external_refs`, `import_runs`), 6, 8.1, 9.1, 12
- `docs/phases/phase-9-statements-and-forecast.md` Track D (reconciliation, done) and Track E (replaced by this brief)
- Existing code: `crates/app/src/external_refs.rs`, `crates/app/src/transactions/`, `crates/app/src/reconciliations/`, the rule engine and its dry run (`POST /rules/{id}/test`), `crates/importer/src/entities/transactions.rs` (how the Firefly importer uses refs and `Origin::Import`), `web/src/features/{accounts,transactions}/`
- The reconcile tool, as a reference for the matching rules (not for its code style: it uses floats and skips bad rows silently, both forbidden here): `internal/reconciler/{bank,splits,paypal,satispay,reconciler}.go`, `internal/parser/*.go`, `internal/balance/balance.go`, `internal/models/memory.go`

## 3. Lessons from firefly-iii-reconcile

Each was a bug or a surprise in the Go tool. The brief turns them into rules; the tests in section 9 pin the important ones.

**Files**

- **L1. The owner's banks export xlsx, not CSV.** Widiba and Mediobanca give `.xlsx`/`.xlsm`. The header is not on row 1 (a prelude of account details comes first), so the header row is found by looking for the configured date column. Balances are labelled cells above the table ("Saldo iniziale (€)", "Saldo finale (€)", "Saldo contabile:"), read from the first non-empty cell to the right of the label.
- **L2. Read xlsx cells raw.** A date-formatted cell rendered as text came back as `mm-dd-yy` in the tool; reading the raw value gives an Excel serial (days since 1899-12-30; workbooks with the 1904 date system start on 1904-01-01). Accept both a serial and the profile's text format.
- **L3. Every source writes numbers differently.** Widiba: `1,300.00` (comma groups, dot decimal). PayPal (Italian locale): `1.234,56`. Amazon: `12.34` or `12,34`. Amounts may carry `€` and spaces. The separators belong to the profile, never guessed per value, and the result is integer minor units (the Go tool used floats with a 0.01 epsilon; Saldo must not).
- **L4. Split debit/credit columns disagree on sign.** Mediobanca's "Uscite" may hold `50,00` or `-50,00`; a debit is always made negative. A row with both columns empty is not a transaction.
- **L5. Column names change between exports.** PayPal renamed "Lordo" to "Importo"; Amazon renamed "Total Owed" to "Total Amount"; the delimiter changed between Amazon/PayPal exports (comma vs tab). PayPal headers carry a UTF-8 BOM and stray quotes. Satispay's ID header is "ID (Share this with Customer Support in case of issues)". Parsers take a list of accepted names per field, strip BOM and quotes, compare ignoring case, and sniff the delimiter.
- **L6. The exports are localised.** PayPal's columns and type names are Italian (`Data`, `Orario`, `Fuso orario`, `Nome`, `Tipo`, `Stato`, `Valuta`, `Importo`, `Codice transazione`) and dates are `d/M/yy` with two-digit years. An English export would break the parser. Keep the names in one table per locale; Italian first, English when a sample exists.
- **L7. Bad rows were skipped silently.** That hid real problems. Here every row that is not imported is kept as a line with status `unsupported` and a reason (no date, footer row, pending line, zero amount, other currency).

**Dates**

- **L8. A bank line has several dates.** Accounting date ("data contabile"), value date ("data valuta"), and sometimes the date the owner wants it booked: Widiba's "Addebito Pacchetto" fee is charged on the 16th for the previous month and belongs on that month's last day. The profile has `date_rules` (`contains` text → `book_on: previous_month_end`). The booked date is the transaction's date and the month the line counts in; the accounting date stays a matching candidate.
- **L9. Ledger dates drift from bank dates.** Usually one or two days (weekends, value vs accounting date); PayPal and Satispay up to a week after; Amazon direct debits can arrive weeks after the order. Matching windows are per source, asymmetric (a bank line rarely precedes its PayPal row by more than 2 days), and the ledger entry may sit on the Amazon *order* date rather than the charge date (the owner often entered Amazon purchases on the order date).
- **L10. Time zones move dates.** Amazon's `Order Date` is ISO 8601 in UTC: `2024-01-05T23:30:00Z` is 6 January in Italy. PayPal gives a local time with a `Fuso orario` column. Convert every timestamp to the household's time zone before taking its date (the Go tool took the UTC date).

**Matching**

- **L11. Match one-to-one, best pair first, never first fit.** Two monthly payments of the same amount must match two ledger entries, not the same one twice. The tool builds every eligible pair (same amount, same direction, within the window), ranks it by `date distance − 3 × description similarity`, and takes pairs greedily in rank order, skipping any side already used. That was simple and good enough; an optimal assignment algorithm was never needed.
- **L12. Similarity measure depends on the texts.** Description against description: Jaccard over lowercase words of 3+ letters. A short merchant name against a long bank description: *coverage* (share of the name's words found in the description); Jaccard understated it.
- **L13. Transfers must match the right side.** A bank debit can only match a transfer *out of* the account, a credit only one *into* it.
- **L14. Split groups usually pay one bank line, but not always.** An Amazon order the owner split by category is one bank debit. Splits of one group that share kind, currency and date and all touch the account on the same side are one candidate with the summed amount; otherwise each split stands alone. The owner also has groups whose splits are *separate* bank lines (for example one group for several payments on the same day), so the whole group is never the only candidate: each of its splits is one too, and the assignment uses either the group or its splits, never both (Track A). A split that pays only *part* of a bank line, or a bank line paid by splits of several groups, is left to a manual link.
- **L15. Leftovers explain mistakes.** An existing entry left unmatched that has the same amount and kind as a matched one within the window is most likely a *duplicate* (typed by hand and imported). Identical unmatched entries (same date, amount, description) are duplicates of each other. The rest are *not on the statement*. A matched entry on another account is *on the wrong account*. All four were the tool's most useful findings.
- **L16. Decisions must be remembered.** "This line is that entry", "ignore this line", "this month is done" were kept in a state file; without that, every run re-proposed the same things. A remembered link whose entry was deleted must fall back to normal matching, not point at nothing. In Saldo, nothing removes a split's `external_refs` row when the split is deleted today (`external_refs::remove` is only called by the Firefly importer), so this case needs handling.
- **L17. Fingerprints need an occurrence counter.** `account | date | amount | description` collides for two identical coffees on one day. The tool hashed `base | n`, where `n` counts identical lines before it in the file. It is stable across overlapping exports that cover whole days. It changes when a bank rewrites a description between "pending" and "booked", so pending lines are never imported (L7) and an unknown fingerprint still gets a `possible_match` by amount and date.
- **L18. Detecting the source by "contains paypal" is too loose and too strict.** Banks write `PAYPAL *NETFLIX`, `PayPal Europe S.a.r.l.`, `AMZN Mktp IT`, `Amazon.it*AB12CD34`, `AMAZON PRIME`. `contains("amazon")` misses `AMZN`. The patterns are a list of case-insensitive regular expressions per source, editable in settings, with these as defaults.

**Amazon**

- **L19. The order ID is authoritative.** `\d{3}-\d{7}-\d{7}` in the bank description links the line to that order whatever the amount and date: gift cards and promotions lower the charge, and an order is charged once per shipment, so several bank lines can name one order. Refunds naming the order reduce the charge. Keep the difference (`amount_diff`) and show it.
- **L20. Marketplace references are not order IDs.** `Mkt*AB1CD2EF3G`, `Ufg*AU8U062M5` cannot be resolved against the order file. Keep them on the line for display only.
- **L21. Without an ID, match by amount and date, then approximately.** Exact amount within the window first; then the closest amount within a configurable tolerance, marked *approximate*. Combining 2–3 orders into one charge, or 2–3 charges into one order, found real cases but also false ones; the tool kept both off by default.
- **L22. The order file has one row per item.** Group by `Order ID`. Skip cancelled rows. An order paid entirely by gift card (`Payment Instrument Type`) never reaches a bank; the instrument (for example "Visa - 1234") also tells which account was charged.

**PayPal**

- **L23. Classify every row before matching.** By `Tipo` and `Stato`: *funding* ("Bonifico bancario sul conto PayPal"), *withdrawal* ("Trasferimento avviato dall'utente"), *refund* ("Rimborso…"), *other* (authorisation, cancellation, hold, hold release, currency conversion, and any row with `Stato` negata/annullata/stornata), and *payment* (any other row where money left PayPal). Before this, holds and conversions competed with real payments and produced wrong matches.
- **L24. A funding row and its payment share date and time.** Pair them (amount breaks ties; a single payment at that exact time is accepted even with another amount, which is a foreign-currency payment funded in euro). The funding row's amount is what the bank shows; the payment row names the merchant.
- **L25. Not every PayPal payment reaches the bank.** Payments and refunds settled on the PayPal balance have no bank line and are not findings. Only funding rows and withdrawals must appear on a statement; if they do not, that is a finding.

**Satispay**

- **L26. Only bank top-ups and withdrawals reach the bank.** "🏦 From Bank" (a bank debit) and "🏦 To Bank" (a bank credit), not cancelled. The `Type` column contains an emoji. Payments from the Satispay balance never reach the bank. The app exports one xlsx per year. In Saldo, Satispay is an asset account (the owner and the owner's wife have one each, each topped up from a bank account), so the Satispay file is a statement of that account (decision 3): top-ups and withdrawals are transfers with the bank account, balance payments are withdrawals to the merchant in `Name`, and cancelled rows are `unsupported`.

**Balances**

- **L27. Check the statement before trusting it.** `opening + sum(lines) − closing` must be zero; when it is not, the export is truncated or contains pending lines. Then compare the bank's balance at each month end (walking back from the closing balance, using booked dates) with Saldo's balance at the same date: the first month that differs is where to look.

## 4. Decisions

Carried over from Phase 9 (owner, 2026-11-17): statement import runs in the web app (upload, review, commit), there is no CLI command; parsers live in `saldo-app` as pure functions over `&[u8]`; drafts and per-line edits append no events, while creating, committing, discarding and reverting an import do.

Proposed in this brief (confirm or change them when reviewing it; record the confirmation in `STATUS.md`):

1. **Formats**: xlsx/xlsm (new, L1), CSV, CAMT.053 and OFX for statements. xlsx and CSV are driven by a saved profile; CAMT and OFX need none. Add `calamine` for xlsx (maintained, pure Rust, reads raw values). If the owner's banks offer CAMT.053, that becomes the preferred format, since it carries IDs, IBANs and balances.
2. **Companion sources (Amazon and PayPal) are stored and can be updated.** Amazon (the "Your Orders" data export) and PayPal (activity CSV) files are uploaded on a "Sources" page, their rows are kept in `source_rows`, and every statement draft matches against all stored rows in its window. The Amazon export is a data request covering years, while bank statements arrive monthly, so the rows must outlive the upload. The owner can upload a newer or overlapping export at any time: each row has a stable ID (Amazon `Order ID` + item position, PayPal `Codice transazione`), so only new rows are added and known rows get their latest status (an item shipped, a payment completed). The uploaded file itself is kept on disk by SHA-256 (like attachments), so a parser fix can re-read every stored document ("Re-read" on the Sources page) without asking for the files again.
3. **Satispay is a bank, PayPal is a source** *(owner, 2026-11-27: Satispay is an asset account, one for the owner and one for the owner's wife, each topped up from a bank account)*. Satispay files are imported through Track B with an xlsx profile, into the chosen Satispay account. A top-up is a transfer between the Satispay account and its funding bank account, whichever statement is imported first; the second import finds that transfer and links to it (L13), so there is one transfer. To know the other side, an asset account gets an optional **funding account** (Satispay → its bank account), and a bank profile gets **transfer rules**: a description pattern (default `satispay`) names the own account a matching line is a transfer with. When both Satispay accounts are funded from the same bank account, the rule offers both and the line's match against the already imported Satispay lines decides; failing that, the review asks. A bank line paying PayPal becomes a withdrawal to the *merchant* (from the paired payment row), not a transfer to a PayPal account; importing PayPal as an account of its own is a later phase.
4. **Enrichment changes the proposal, never silently the ledger.** A matched source row proposes description, counterparty, notes and (Amazon) a split by item. The review shows the bank text and the proposal side by side; the owner can revert per line. Committed transactions keep the original bank text in `notes` and the source references in `splits.meta.statement` (decision 8), so nothing is lost.
5. **Rules run after enrichment.** The dry run and the commit see the enriched description and counterparty, so a rule like "description contains Netflix → Subscriptions" works for PayPal payments too. The rule engine must run for statement imports: check what it does for `Origin::Import` and, if it skips it (as the Firefly importer needs), add an explicit option rather than change the Firefly importer.
6. **The matcher lives in `saldo-core::matching`.** It is pure: integer amounts, dates, token sets, and a ranked one-to-one assignment (L11). Every source matcher (bank ↔ ledger, bank ↔ Amazon, bank ↔ PayPal, bank ↔ Satispay) uses the same assignment function with its own eligibility and score. Tie-breaks are deterministic (score, then the candidate using more splits, then line and candidate position), so a re-run proposes the same thing.
7. **Remembered decisions use existing rows, with one new table.** A link is an `external_refs` row (source `bank-statement`, entity `split`) as in Phase 9. A *skip* is remembered as the line's fingerprint on a committed import with action `skip`, so the next overlapping file marks it `skipped_before`. Source ↔ split matches go in a new `source_matches` table, because one Amazon order can pay several bank lines and one bank line several orders, which `external_refs` (one local row per external ID) cannot hold. "This month is done" is the account's reconciliation (Phase 9 Track D): lines dated on or before the account's `reconciled_through` that match nothing are flagged, not proposed for creation.
8. **What a created split stores**: description = the proposal (decision 4); `notes` = a short Markdown block with the bank text and, when matched, the order ID and products or the PayPal merchant and transaction code; `meta.statement = { import_id, position, fingerprint, accounting_date, value_date, sources: [...] }`. The bank text is never thrown away.
9. **Amazon split by item** is offered, not default: one split per item at its `Total Amount`; the difference between the charge and the sum of items (shipping, gift card, promotion, partial shipment) goes to the largest item, so the splits always add up to the bank amount exactly. Each split's description is the product name cut to 120 characters.
10. **Matching windows** (days, bank date minus source date; profile and source settings can change them): ledger ±3 (also against the Amazon order dates of a matched line, L9); a transfer found through a transfer rule −2 to +7 (Satispay top-ups, L26); PayPal −2 to +7; Amazon without order ID −2 to +14; Amazon with order ID: no window (L19). A day before the source date costs 2, a day after costs 1 (L9). Description similarity weighs 3 days.
11. **Deleting a split removes its `bank-statement` refs and `source_matches`** in the same transaction (L16), so a re-import proposes the line again instead of calling it a duplicate. Refs from other sources (`firefly-iii`) keep their current behaviour.
12. **Pending lines are never imported** (L17): a row without an accounting date, or marked pending by the profile (`pending_when` text), is `unsupported` with reason "pending".

## 5. Scope

In scope: the work packages below, including migrations, `core` logic, services, events, endpoints, OpenAPI and TS schema regeneration, web screens, export of new tables, and tests.

Out of scope: direct bank connections (PSD2/GoCardless); exchange-rate conversion (lines in another currency than the account's are `unsupported`; a PayPal foreign-currency payment is matched through its euro funding row, L24); PayPal as an account of its own (decision 3); automatic matching of a split that pays part of a bank line, or of one bank line paid by splits of several groups (L14: linked by hand); other merchants' order exports (eBay, Zalando, …: the source design must allow adding one as a new `source_kind` without schema changes); the combination matchers of L21 (two orders in one charge, one order in two charges), unless the owner asks after using the exact and approximate passes; a CLI.

## 6. Work packages

Package 0 comes first. Track A is next, because every other track uses it. B and C can then run in parallel. D needs B and C. E is optional and last.

### Package 0: samples and fixtures

- The owner provides anonymised samples: Widiba xlsx, Mediobanca xlsx, one credit card statement if it is a separate export, Amazon order history CSV, PayPal CSV (Italian), Satispay xlsx; and CAMT.053 or OFX if a bank offers them. Until then use the ISO 20022 and OFX specification examples and the column lists in section 3, and mark the gap in `STATUS.md`.
- Fixtures live in `crates/app/tests/fixtures/statements/` and must be synthetic or anonymised (names, IBANs, order IDs, amounts shifted). Real exports never enter the repository; add `*.xlsx`, `*.xlsm` and `*.csv` under `fixtures/private/` to `.gitignore` for local runs against real files. Small xlsx fixtures are generated by a test helper rather than committed as binaries, if `calamine`'s writing companion is not wanted; otherwise commit the few small ones.
- Write the fixture lines so that every lesson in section 3 has at least one line exercising it.

### Track A: matching core (`crates/core/src/matching/`)

Pure, no I/O, close to 100% coverage, property tests where noted.

- `normalise(text) -> String`: Unicode lowercase, collapse whitespace, trim. `tokens(text) -> BTreeSet<String>`: alphanumeric words of 3+ characters. `jaccard(a, b)` and `coverage(part, whole)` as integer per-mille (0–1000), not floats.
- `fingerprint(account_id, date, amount, normalised_text, occurrence) -> String` (SHA-256 hex) and `assign_occurrences(lines)` (L17). A property test: shuffling lines from different days does not change any fingerprint; two identical lines on one day get different ones.
- `assign(pairs: Vec<Pair>, pinned: &[(usize, usize)]) -> Vec<(usize, usize)>`: pinned pairs first, then pairs sorted by `(score, more splits used first, line, candidate)` and taken greedily (L11). The right side of a pair is a candidate that *uses a set of split IDs*; a pair is skipped when its line is taken or any of its split IDs is. That one rule gives one-to-one matching for single splits and lets a group and its own splits compete without a split being used twice (L14). A property test: no line appears twice, no split ID appears twice, every pinned pair is in the result.
- `ledger_candidates(splits, account_id) -> Vec<Candidate>`: one candidate per split touching the account, plus one per combinable group with the summed amount (L14). When a group and one of its splits score the same, the group wins (most groups are one bank line). Tests: a two-split group matches one bank line by its sum; a two-split group dated on one day matches two bank lines with the splits' amounts; a group never matches one line while its split matches another.
- `match_ledger(lines, candidates, window, pinned) -> LedgerMatch`: eligibility = same absolute amount, direction compatible (L13), within the window of any of the line's dates (booked, accounting, Amazon order dates). Score = `day_distance × 1000 − 3 × similarity` in per-mille units. Result: matched pairs, unmatched lines, and the unmatched candidates classified as `duplicate_of` a matched one, duplicates of each other, or `not_on_statement` (L15).
- `classify_paypal(row) -> PayPalKind` (L23), `pair_paypal_funding(rows) -> Vec<PayPalMovement>` (L24), `match_paypal(lines, eligible, rows, movements, window, range)`: eligible only for lines the PayPal patterns hit (L18); direction: debit ↔ funding or payment, credit ↔ withdrawal or refund; score uses the asymmetric lag (decision 10) minus coverage of the merchant name (L12). Leftover funding and withdrawals are `missing_from_bank`; leftover payments and refunds are `paypal_balance_only` (L25).
- `match_amazon(lines, eligible, orders, window, amount_tolerance, range)` (L19–L22): pass 1 by order ID (many lines to one order, keeps `amount_diff`, refunds reduce the charge), pass 2 exact amount in the window, pass 3 closest amount within tolerance (`approximate = true`). Orders are groups of items; cancelled items and gift-card-only orders are left out.
- `split_by_item(charge, items) -> Vec<Money>` (decision 9): sums to `charge` exactly; a property test checks that for any items and charge.
- `balances_at(closing, lines, dates) -> Vec<Money>` (the caller passes the month ends it wants) and `statement_gap(opening, amounts, closing) -> Option<Money>` (L27).

Explain the first appearance of `BTreeSet`, sorting with a key tuple, and `impl Ord` if used (see section 8).

### Track B: bank statement import (`crates/app/src/statements/`)

This is Phase 9 Track E with these changes. Keep everything there not changed here.

- **Dependencies** (`cargo add` in `saldo-app`): `csv`, `quick-xml`, `encoding_rs`, `calamine`, `regex` (if not already in the workspace).
- **Schema**:
  - `statement_profiles` as in Phase 9, with `format` also allowing `xlsx`, and the `csv` JSON column renamed `mapping` (used by both CSV and xlsx). `mapping` adds: `sheet` (xlsx; default the first), `header_match` (the date column's header, used to find the header row, L1), `opening_balance_label` and `closing_balance_label` (L1), `thousands_separator` next to `decimal_separator` (L3), `value_date_column`, `category_column` (Widiba's "CAUSALE", Mediobanca's "Tipologia"; joined into the description when present), `currency_column` (default the account's currency), `pending_when` (decision 12), `skip_when: [{ column, contains }]` (Satispay's cancelled rows, kept as `unsupported`), `date_rules: [{ contains, book_on }]` with `previous_month_end` as the only `book_on` (L8), and `transfer_rules: [{ pattern, account_ids[] }]` (decision 3). Column names are lists of accepted names (L5). Ship a built-in Satispay mapping (sheet `Transactions`, `Date` with time, `Name` as counterparty, `Amount`, `Type`, `Status`, the `ID…` column as external ID by prefix, L5) and a transfer rule from `Type` containing "From Bank"/"To Bank" to the account's funding account.
  - `accounts.funding_account_id` (nullable, an own asset account; new migration): the bank account a Satispay-like account is topped up from (decision 3). Editable on the account form for asset accounts; deleting the funding account clears it.
  - `statement_imports` as in Phase 9, plus `opening_balance_minor` (nullable) and `gap_minor` (nullable, L27).
  - `statement_lines` as in Phase 9, plus `accounting_date`, `value_date`, `booked_date` (the date used for the transaction), `raw_text` (the bank's description, kept verbatim), `occurrence`, `status` also allowing `skipped_before` and `not_matched_reconciled` (decision 7), and `proposal` (JSON: description, counterparty name, notes, split by item, filled by Track D).
- **Parsers** (`statements::parse`, one `match` on the format, no trait), as Phase 9 plus:
  - **xlsx**: `calamine`, raw cell values, serial or text dates (L2), header row found by `header_match` (L1), labelled balances (L1), and every non-imported row kept as `unsupported` with a reason (L7). Rows after the table (totals) are `unsupported` with reason "not a transaction".
  - **CSV**: BOM stripped, delimiter from the profile or sniffed from the header (L5), `encoding_rs` for `windows-1252`.
  - Amounts parsed from text with the profile's two separators straight into minor units, rejecting more decimals than the currency has (L3); debit columns always negative (L4).
- **Classification** (`StatementService::classify`, no writes): for each line, in order:
  1. `duplicate` when a `bank-statement` external ref already maps `account_id:external_id` or the fingerprint (Phase 9); `skipped_before` when a committed import of the account has the fingerprint with action `skip` (decision 7).
  2. Run Track D's source matching (when it exists) to get order dates and proposals.
  3. `match_ledger` against the account's splits in the statement's range ± the window. Only splits **without a `bank-statement` ref** are candidates; refs from other sources (the Firefly import) do not exclude them. Note for the implementer: the Phase 9 text said "no external ref", which would have excluded the owner's entire imported history. Matched lines are `possible_match` with the matched split (or group) and the reason ("same amount, 1 day apart, similar text").
  4. A line dated on or before the account's `reconciled_through` and matching nothing is `not_matched_reconciled` with action `skip`: that period was already confirmed, so a missing transaction there needs a look, not an automatic create.
  5. Otherwise `new`.
  The draft also stores the leftover findings from `match_ledger` (duplicates in Saldo, entries not on the statement, entries on another account found by a household-wide search for same amount and date on other asset accounts) in `summary.findings`, shown on the review page but never acted on automatically.
- **Counterparties** as in Phase 9 (own IBAN → transfer; then a transfer rule → transfer with that own account; IBAN or exact name → existing account; else a new expense or revenue account on commit). A transfer rule naming several accounts (both Satispay accounts funded from one bank) picks the one whose imported lines contain a transfer of that amount in the window; otherwise the line keeps all of them and the review asks. The name used is the proposal's counterparty when Track D enriched the line.
- **Transfers between own accounts** imported from both statements: the second import must find the transfer the first created. That works through `match_ledger` because a transfer touching the account is a candidate with the direction check (L13), with the transfer rule's wider window (decision 10). The acceptance tests in section 9 cover it for two banks and for a bank and Satispay, in both import orders.
- **Commit, discard, revert** as in Phase 9. Commit stores decision 8's `notes` and `meta.statement`. Revert also removes the import's `source_matches` rows. Deleting a split anywhere removes its `bank-statement` refs and `source_matches` (decision 11): add it to `TransactionService`'s delete and replace paths (a replaced split keeps its ID since 2026-11-15, so only splits removed from a group count).
- **Balance checks** (L27): with both balances, `gap_minor` is stored and a non-zero gap is a warning on the review page ("the file is missing lines or contains pending ones"), not an error. With a closing balance, the review shows bank vs Saldo balance at each month end the file covers, and the first month that differs.
- **Server**: as Phase 9 (`/statement-profiles`, `POST /statement-profiles/preview`, `/statement-imports…`). The preview returns the first 20 parsed lines *and* the unsupported rows with reasons, so the mapping editor shows why a row was dropped.
- **Web**: Phase 9's wizard, plus: the review table shows bank text and proposal side by side, the matched split for `possible_match` with its reason, status filters, bulk actions (skip all duplicates, link all possible matches above a score, create all new), and a findings panel (duplicates in Saldo, not on statement, on another account, statement gap, first month with a different balance). The first import of an account with years of history can set "import lines from" a date, defaulting to the day after `reconciled_through`.

### Track C: companion sources (`crates/app/src/sources/`)

- **Schema**:
  - `source_documents`: `id`, `household_id`, `kind` (`amazon_orders`, `paypal`), `filename`, `sha256` (the file is stored at `<data dir>/sources/<sha256>`, decision 2), `first_date`, `last_date`, `row_count`, `rows_added`, `rows_updated`, `created_by_user_id`, `created_at`. Uploading a file with a known `sha256` is a 409 ("already uploaded").
  - `source_rows`: `id`, `household_id`, `kind`, `external_id` (decision 2), `document_id` (the upload that first brought it), `last_document_id` (the latest upload that contained it), `date` (household date, L10), `time` (nullable), `amount_minor` (signed, from the owner's point of view: money out is negative), `currency_code`, `row_kind` (Amazon `item`; PayPal `funding`/`withdrawal`/`payment`/`refund`/`other`), `group_key` (Amazon order ID; PayPal: the funding row's paired payment ID), `counterparty` (PayPal `Nome`), `text` (Amazon product name, PayPal `Descrizione`), `instrument` (Amazon payment instrument), `status` (source status, for display), `raw` (JSON of the original columns), `UNIQUE (household_id, kind, external_id)`. A newer export of the same row updates `status` and `raw` (an Amazon item shipped, a PayPal payment completed) and keeps the ID.
  - `source_matches`: `household_id`, `source_row_id`, `split_id` (nullable while the line is in a draft), `statement_import_id` and `line_position` (nullable once committed), `match_type` (`order_id`, `amount_date`, `approximate`, `funding_pair`, `amount_lag`, `manual`), `amount_diff_minor`, `created_at`. Rows are bookkeeping for drafts and the record of what enriched a split once committed. Cascades on `source_rows` and `splits`.
  - `source_settings` per household (one JSON column on a small table, or rows per kind): detection patterns per source (L18), windows (decision 10), Amazon amount tolerance, and Amazon instrument → account mapping (L22).
- **Parsers** (`sources::parse`, pure, one `match` on the kind): Amazon CSV (L5, L10, L22), PayPal CSV (L5, L6, L23; pairing funding rows happens in core, not the parser). Every row not stored is reported with a reason (L7). Limits: 5 MB per upload, 50,000 rows.
- **Service** (`SourceService`): `upload(ctx, kind, file)` parses, upserts rows by `external_id` (new rows inserted; known rows get the newer `status`, `raw` and `last_document_id`, keeping their ID and matches), stores the file, writes the document and appends `source_document.created` with `{ kind, rows_added, rows_updated }`. A known row whose amount or date changed is updated too, and its draft matches are dropped so the next classification redoes them; committed matches stay and are flagged on the Sources page. `reread(ctx)` parses every stored file again in upload order with the same upsert (after a parser fix). `list`, `rows(ctx, filters)` (by kind, date range, matched or not); `delete_document(ctx, id)` deletes the document, its file and the rows no other document contains that are not matched to a committed split (409 otherwise), appending `source_document.deleted`.
- **Server**: `GET/POST /source-documents` (multipart: `kind`, file), `DELETE /source-documents/{id}`, `POST /source-documents/reread`, `GET /source-rows?kind=&from=&to=&matched=` (paginated), `GET/PUT /source-settings`.
- **Web**: a "Sources" page under More: "Upload a newer export" per kind (the result says how many rows were added and updated), the documents list with date ranges and row counts, the covered date range per kind (so gaps between exports are visible), a rows table with "unmatched only", and the settings form with the default patterns shown.

### Track D: matching sources into the review

- `StatementService::classify` step 2 (Track B): load the household's source rows in the statement's range widened by the largest window, run `match_amazon` and `match_paypal` in that order, then use their results:
  - Matched Amazon lines gain the order dates as extra ledger dates (L9).
  - The line's `proposal`: description from the PayPal merchant or the Amazon products ("Amazon: <first product>" plus "+ N more"); counterparty "Amazon" or the PayPal merchant; a notes block (decision 8); for Amazon, the item amounts for `split_by_item`. PayPal wins over Amazon when both match (a PayPal payment to Amazon names the order better through PayPal's text); otherwise one source per line.
  - Draft `source_matches` rows for the matches.
- The rule dry run runs on the proposal (decision 5).
- `update_line` gains: replace or clear the source match by hand (a picker listing unmatched source rows near the line's date and amount, `match_type = manual`), toggle "split by item", and revert to the bank text.
- Commit: `create` lines are created from the proposal; `link` lines keep the existing split's description unless the owner chose "use proposal" on that line (then it updates description and notes through `TransactionService::replace`, which appends its `transaction.updated` event). Draft `source_matches` get their `split_id`.
- The review shows, per source, the rows in the statement's window that matched nothing: Amazon orders and PayPal funding rows that should have a bank line (L25), and PayPal payments settled on the PayPal balance, listed separately as information.
- Unmatched source rows are not an error and never block a commit.

### Track E (optional): enrich existing transactions

The owner's history came from Firefly with bank-like descriptions. This track runs the source matchers against *existing* splits instead of statement lines.

- `SourceService::enrichment_candidates(ctx, account_id, from, to)`: the account's splits without a `source_matches` row, treated as bank lines (their description as the text, their date), matched with Track A's functions against unmatched source rows. Returns the proposals; computed on each request, nothing stored.
- `accept(ctx, [{ split_id, source_row_ids, use_description, split_by_item }])`: writes `source_matches` (`split_id` set) and, when asked, replaces the description and notes or splits the group, through `TransactionService` in one transaction. Reconciled splits keep `reconciled` (amount, date and accounts do not change; split by item keeps the total but changes the group's shape, so it is refused on reconciled groups).
- Server: `GET /accounts/{id}/enrichment?from=&to=`, `POST /enrichment/accept`. Web: an "Enrich from sources" action on the account page with the same side-by-side table and bulk accept.

### Last: export and docs

- The JSON and CSV export add `statement_profiles`, `source_documents` (metadata only), `source_rows` and `source_matches`, and `source_settings`. Statement imports and their lines are left out, like `import_runs`; the source rows are kept because they are the owner's data and cannot always be downloaded again (Amazon's export is a data request that takes days). Format version stays 1. Record this in ARCHITECTURE.md 9.2.
- Update ARCHITECTURE.md 8.5 if any background task changed. None should.

## 7. Contracts

Honour section 6's conventions: the error envelope, amounts as decimal strings, `scope` on lists, pagination on the imports, documents and rows lists. New event types: `statement_import.created`, `statement_import.committed`, `statement_import.discarded`, `statement_import.reverted`, `statement_profile.created`/`.updated`/`.deleted`, `source_document.created`, `source_document.deleted`, `source_settings.updated`. Draft edits and draft `source_matches` append none. Every new service function takes `HouseholdCtx` first and filters by `household_id`. Add `statement_import` and `source_document` to the live-event keys.

## 8. Rust concepts to explain

| Concept | Where | Analogy |
|---|---|---|
| Byte slices vs strings, UTF-8 and decoding (`&[u8]`, `String`, `encoding_rs`) | parsers | PHP strings are byte arrays with `mb_*` on top; Go's `[]byte` vs `string` is the same split, but Rust checks UTF-8 when converting |
| Streaming XML with an event loop (`quick-xml` `Reader`) | CAMT and OFX 2.x | Go's `xml.Decoder.Token()` loop / PHP's `XMLReader`, not `SimpleXML` loading the whole tree |
| A small state machine as an enum | the OFX 1.x tag reader | A Go `switch state` loop, with the compiler checking every state is handled |
| Enums with data for row kinds and match results | `PayPalKind`, `LineStatus`, `MatchType` | A Go interface with a type switch, or a PHP 8.1 backed enum that can also carry fields |
| `BTreeSet` and sorting by a key tuple (`sort_by_key(\|p\| (p.score, p.date, p.left))`) | matcher | PHP `usort` with a spaceship operator over arrays; Go `sort.Slice` with a multi-field less function |
| `Option` chains (`and_then`, `or_else`) | picking the proposal from PayPal or Amazon | PHP's `??` chain; Go's "if v, ok := …; ok" ladder |
| `regex::Regex` built once from settings | detection patterns | A compiled Go `regexp.MustCompile`, or a PHP pattern kept in a property instead of a literal in every `preg_match` |
| `TryFrom` for checked conversions | parsed row → `Money` and dates | A Laravel form request's validated DTO; a Go constructor returning `(T, error)` |

## 9. Acceptance criteria

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
SQLX_OFFLINE=true cargo test --workspace
cargo llvm-cov --workspace --summary-only     # >= 80%, saldo-core close to 100%
cd web && npm run lint && npm run typecheck && npm test && npx playwright test
```

- **Track A**:
  - Property tests for `assign` (one-to-one, pinned kept), `fingerprint` (order-independent across days, distinct within a day) and `split_by_item` (sums exactly).
  - Two monthly payments of the same amount match two ledger entries (L11). A debit never matches a transfer into the account (L13). A two-split group matches one bank line by its sum (L14).
  - Leftover classification: an extra hand-typed copy of a matched entry is `duplicate_of`; two identical unmatched entries are duplicates of each other; the rest is `not_on_statement` (L15).
  - PayPal: a funding row pairs with its payment by time; a foreign-currency payment pairs by time alone; holds, conversions and denied rows never match; a balance-only payment is not a finding (L23–L25).
  - Amazon: an order ID matches across amount and date; one order charged twice gives one match with two lines; an approximate match is marked; a cancelled item is ignored (L19–L22).
- **Track B**:
  - The parsers pass on the owner's samples (or the specification examples), including: header below a prelude, serial and text dates, both number styles, split debit/credit columns with either sign, a footer row and a pending row kept as `unsupported` with reasons (L1–L4, L7).
  - Importing the same file twice creates nothing; an overlapping file creates only the new lines; a skipped line stays skipped in the next file (decision 7).
  - A line matching a split imported from Firefly is `possible_match`, not `new`.
  - Deleting a split created by an import, then importing the file again, proposes that line as `new` (decision 11).
  - A transfer between two own accounts imported from both statements results in one transfer.
  - Satispay: the owner's xlsx imports into the owner's Satispay account with the built-in mapping (cancelled rows `unsupported`, balance payments as withdrawals to the merchant); a top-up imported from the bank first and the Satispay file second, and in the other order, results in one transfer each time; with both Satispay accounts funded from one bank account, a top-up goes to the account whose Satispay line matches it (decision 3).
  - A "previous month end" date rule books the fee on the last day of the previous month and still matches an entry dated there (L8).
  - A statement with a missing line shows a non-zero gap; month-end balances name the first month that differs (L27).
  - Reverting deletes exactly the created groups and their source matches, and is refused when one of them is reconciled.
  - A billion-laughs XML file, a file over 5 MB and a file over 10,000 lines are rejected.
- **Track C**: uploading the same Amazon export twice is a 409; a newer overlapping export adds only new rows and updates statuses, keeping committed matches; "Re-read" after a parser change gives the same rows as uploading the files again; an Amazon timestamp late in the UTC evening gets the next day's date in Europe/Rome (L10); deleting a document keeps rows matched to committed splits.
- **Track D**: a bank line with an Amazon order ID gets the products as its proposal and, with "split by item", splits that add up to the bank amount; a PayPal bank line gets the merchant, and a rule on the merchant name applies in the dry run and on commit; the original bank text is in the created split's notes (decisions 4, 5, 8, 9).
- **Track E** (if done): accepting an enrichment changes descriptions only, keeps `reconciled`, and refuses split by item on a reconciled group.
- **Playwright flows**: import an xlsx statement with a new profile and commit, then reconcile from the closing balance; upload an Amazon export, import a statement with an Amazon line, split it by item and commit.

## 10. Hand-off

In `docs/STATUS.md`: Phase 13 done (or which tracks are done) with commit hashes; the banks and source files tested with real samples (and which lessons of section 3 they confirmed or contradicted); the owner's confirmation of the decisions in section 4; and any deviation from this brief, approved by the owner.
