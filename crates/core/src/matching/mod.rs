//! Matching statement lines with what Saldo already knows (Phase 13,
//! ARCHITECTURE.md 4.3): existing splits, Amazon orders and PayPal rows.
//!
//! Every matcher builds the pairs that could be the same movement, scores
//! them (lower is better), and hands them to [`assign`], which takes the best
//! pairs first and never uses a line or a split twice. The rules come from
//! the owner's earlier Go tool, firefly-iii-reconcile; the lessons they
//! encode are numbered L1… in `docs/phases/phase-13-statement-import.md`.

pub mod amazon;
pub mod assign;
pub mod fingerprint;
pub mod lag;
pub mod ledger;
pub mod paypal;
pub mod statement;
pub mod text;

pub use amazon::{
    AmazonItem, AmazonMatch, AmazonMatchKind, AmazonOrder, AmazonResult, amazon_orders,
    find_order_ids, match_amazon,
};
pub use assign::{Pair, assign};
pub use fingerprint::{fingerprint, occurrences};
pub use lag::LagWindow;
pub use ledger::{
    Candidate, Finding, LedgerMatch, LedgerSplit, Line, ledger_candidates, match_ledger,
};
pub use paypal::{
    PayPalKind, PayPalMatch, PayPalMovement, PayPalRow, classify_paypal, match_paypal,
    pair_paypal_funding,
};
pub use statement::{balances_at, split_by_item, statement_gap};
pub use text::{PER_MILLE, coverage, jaccard, normalise, tokens};
