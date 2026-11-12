//! The `saldo-import` binary: imports data from Firefly III.

fn main() {
    println!("{}", banner());
}

fn banner() -> String {
    format!(
        "{} importer {} (placeholder)",
        saldo_app::app_name(),
        env!("CARGO_PKG_VERSION")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_names_the_app() {
        assert!(banner().starts_with("Saldo importer"));
    }
}
