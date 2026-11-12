//! Saldo application layer: services (use cases), repositories and migrations.

/// Returns the application name from `saldo-core`, proving the dependency is wired up.
pub fn app_name() -> &'static str {
    saldo_core::APP_NAME
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_name_comes_from_core() {
        assert_eq!(app_name(), "Saldo");
    }
}
