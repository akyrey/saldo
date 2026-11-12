//! The `saldo` binary. `main.rs` makes this a program, like Go's `package main`.

fn main() {
    println!("{}", banner());
}

fn banner() -> String {
    format!(
        "{} server {} (placeholder)",
        saldo_app::app_name(),
        env!("CARGO_PKG_VERSION")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_names_the_app() {
        assert!(banner().starts_with("Saldo server"));
    }
}
