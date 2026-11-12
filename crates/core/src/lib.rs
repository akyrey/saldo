//! Saldo domain: pure types and rules with no I/O.

// `mod x;` pulls in `x.rs` as a module (a namespace, like a PHP namespace).
// `pub` makes it visible outside the crate: Go's capitalised names.
pub mod account;
pub mod currency;
pub mod error;
pub mod ids;
pub mod kind;
pub mod lookup;
pub mod money;
pub mod transaction;
pub mod validation;

pub mod balance;
#[cfg(test)]
mod test_support;
