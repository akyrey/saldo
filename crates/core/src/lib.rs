//! Saldo domain: pure types and rules with no I/O.
//!
//! `lib.rs` makes this a library crate, like a Go package that is not `main`.

/// Name of the application, shared by every crate.
pub const APP_NAME: &str = "Saldo";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_name_is_saldo() {
        assert_eq!(APP_NAME, "Saldo");
    }
}
