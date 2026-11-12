# Phase 1: `saldo-core`

## 1. Goal

The pure domain model of the ledger: money, currencies, typed IDs, accounts, transaction groups with splits, split-kind derivation, validation and balance calculation, all heavily tested. Nothing in this phase touches a database, the network, the clock or a random source.

When finished, a test can build a group with three splits that each have a different source and destination, validate it, and compute the resulting balance of every account.

## 2. Read first

- `CLAUDE.md`, `docs/STATUS.md`
- `docs/ARCHITECTURE.md` sections 2 (Decisions), 3 (Crates, the rules for `core`), 4 (Domain model, all of it)
- Section 5.2 (the `accounts` and `splits` tables) to see how the types will be stored; do not write SQL.

## 3. Prerequisites

Phase 0 is done: `cargo test --workspace` passes and `crates/core` exists.

## 4. Scope

In scope: `Money`, `Currency`, `CurrencyCode`, the ID newtypes, `AccountType`, `EquityRole`, `Account`, `SplitKind`, `NewSplit`, `NewTransactionGroup`, `ValidatedGroup`, `ValidationError`, `validate_group`, balance calculation, serde support for all of them.

Out of scope: schedules, rules, bill reminders, budget maths (Phase 5); anything async; SQLx; HTTP; generating IDs or reading the time inside `core`.

## 5. Work packages

Each package: write the tests first, see them fail, implement, commit.

1. **Errors and IDs** (`error.rs`, `ids.rs`): a small macro-free set of newtypes `HouseholdId`, `UserId`, `AccountId`, `GroupId`, `SplitId`, `CategoryId`, `TagId`, `BudgetId`, `BillId`, each wrapping `uuid::Uuid` with `Display`, `FromStr`, serde as a plain string. Tests: round-trip through string and JSON; an invalid string is an error. Writing nine near-identical newtypes by hand is acceptable; if a `macro_rules!` is used, explain it.
2. **Currency** (`currency.rs`): `CurrencyCode` (exactly three ASCII uppercase letters) and `Currency { code, decimal_places }` (0 to 8). Tests: valid and invalid codes.
3. **Money** (`money.rs`): `Money { minor: i64, currency }`. `parse_decimal(&str, &Currency)`, `to_decimal_string(&Currency)`, `checked_add`, `checked_sub`, `is_positive`, `zero(currency)`. Tests: `"123.45"` → 12345; `"0.5"` → 50; `"7"` → 700; `"1.234"` with two places is an error; `"-3.10"` → −310; JPY with zero places; overflow and currency mismatch are errors; empty string, `"abc"`, `"1e3"`, `"1,5"` are errors. Properties (`proptest`): parse(format(x)) == x for any `i64` except extremes; `a + b − b == a`.
4. **Accounts** (`account.rs`): `AccountType`, `EquityRole`, `Account`, serde names matching the SQL values (`asset`, `expense`, `revenue`, `liability`, `equity`).
5. **Split kind** (`kind.rs`): `SplitKind::derive(source: AccountType, destination: AccountType) -> Result<SplitKind, ValidationError>`. Tests: one test per row of the table in ARCHITECTURE.md 4.1, and a table-driven test over all 36 type pairs asserting exactly which are valid.
6. **Groups and validation** (`transaction.rs`, `validation.rs`): the input types, `AccountLookup` (a map from `AccountId` to `&Account` supplied by the caller), `validate_group` returning either a `ValidatedGroup` (splits with their derived kind) or all errors found, each carrying the index of the split. Tests: one per invariant in ARCHITECTURE.md 4.2 (numbers 1 to 9), plus the headline case: a group with three splits with different sources, destinations and kinds validates.
7. **Balances** (`balance.rs`): `fn balance_deltas(group: &ValidatedGroup, accounts: &AccountLookup) -> Vec<(AccountId, Money)>` and a helper that folds deltas into balances. Tests: withdrawal lowers the asset and raises the expense; a foreign-currency transfer moves the amount out of the source and the foreign amount into the destination. Property: for any valid group in one currency, the deltas sum to zero.
8. **Public API pass** (`lib.rs`): re-export the public types, add crate-level docs with one worked example as a doc test, run `cargo doc` and fix warnings.

## 6. Contracts

Provided to later phases (names are binding, see ARCHITECTURE.md 4):

```rust
pub fn validate_group(input: NewTransactionGroup, accounts: &AccountLookup)
    -> Result<ValidatedGroup, Vec<ValidationError>>;
impl SplitKind { pub fn derive(source: AccountType, destination: AccountType) -> Result<SplitKind, ValidationError>; }
impl Money { pub fn parse_decimal(s: &str, currency: &Currency) -> Result<Money, MoneyError>; }
```

- `ValidationError` is an enum with one variant per invariant and a stable machine-readable `code()` such as `amount_not_positive`, `same_account`, `invalid_account_pair`, `currency_mismatch`, `foreign_amount_required`, `account_inactive`, `empty_group`, `unknown_account`. Phase 2 puts these codes in API error responses.
- `ValidatedGroup` has no public constructor other than `validate_group`.
- Dependencies allowed: `uuid`, `chrono` (dates only), `serde`, `thiserror`; dev: `proptest`, `serde_json`.

## 7. Rust concepts to explain

| Concept | Where it first appears | Analogy to use |
|---|---|---|
| Structs and `impl` blocks | `Money` | A PHP final class with methods, or a Go struct with methods; no inheritance |
| Newtype pattern | IDs | Go's `type AccountID string`, but with no implicit conversion; PHP has no equivalent short of a value-object class |
| Ownership and moves | `validate_group(input, ...)` takes the group by value | In PHP and Go a value can be used after passing it on; in Rust passing by value hands it over, and the compiler stops you using it afterwards. Like handing someone the only copy of a document |
| Borrowing (`&T`) | `&AccountLookup`, `&Currency` | Lending the document to be read and getting it back; close to passing a Go pointer you promise not to modify, checked by the compiler |
| `Option<T>` | `foreign_amount` | PHP's `?Money` or a Go pointer that may be nil, except the compiler forces the null check |
| `Result<T, E>` and `?` | `parse_decimal` | Go's `value, err :=` with `if err != nil { return err }` shortened to one character; unlike PHP exceptions, the error is in the function signature |
| Enums and `match` | `AccountType`, `SplitKind::derive` | A PHP 8.1 enum plus `match`, but the compiler rejects a `match` that misses a case; Go has no real equivalent (iota constants + switch without the exhaustiveness check) |
| Enums carrying data | `AccountType::Equity(EquityRole)`, `ValidationError` | A tagged union: a PHP enum case that also holds fields, or a Go interface with one struct per case |
| Traits and `derive` | `Display`, `FromStr`, `Debug`, `Clone`, `PartialEq`, `Serialize` | Traits are Go interfaces (PHP interfaces) that you can also implement for types you did not write; `#[derive]` generates the implementation, like Go's `stringer` built into the compiler |
| Modules and visibility | `lib.rs`, `pub` | PHP namespaces; `pub` versus private is Go's capitalised versus lower-case names |
| Unit tests in the same file | `#[cfg(test)] mod tests` | Go's `_test.go` next to the code; PHPUnit tests live elsewhere |
| Property tests | `proptest` | Go's `testing/quick` or fuzzing: the framework generates inputs and checks a rule holds for all of them |

Lifetimes: if `AccountLookup` needs an explicit lifetime (`AccountLookup<'a>`), explain it as "the lookup borrows the accounts, and `'a` tells the compiler the lookup must not outlive them"; if it makes the code hard to read, let the lookup own cloned accounts instead.

## 8. Acceptance criteria

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test -p saldo-core
cargo doc -p saldo-core --no-deps            # no warnings
cargo llvm-cov -p saldo-core --summary-only  # line coverage >= 95%
cargo tree -p saldo-core                     # no tokio, sqlx, reqwest, axum
```

- Every invariant in ARCHITECTURE.md 4.2 has at least one failing-case test and one passing-case test.
- No `unwrap()` / `expect()` outside tests, no floats anywhere.

## 9. Hand-off

In `docs/STATUS.md`: Phase 1 done with commit hash; list the public types and any naming that differs from ARCHITECTURE.md section 4 (and update that section); note the coverage figure.
