//! What an import did: counts per entity, aggregated notes, messages, and the
//! balance check. Printed at the end and stored in `import_runs.summary`.

use std::collections::BTreeMap;
use std::fmt::Write;

use serde::Serialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub created: u64,
    pub updated: u64,
    pub unchanged: u64,
    pub skipped: u64,
}

/// One row of the balance check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BalanceLine {
    pub account: String,
    pub currency: String,
    pub firefly: String,
    pub saldo: String,
}

impl BalanceLine {
    pub fn matches(&self) -> bool {
        self.firefly == self.saldo
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportReport {
    pub dry_run: bool,
    /// Entity name → counts. A `BTreeMap` keeps keys sorted, so output is stable.
    pub entities: BTreeMap<String, Counts>,
    /// Repeated situations counted instead of listed one by one.
    pub notes: BTreeMap<String, u64>,
    /// Individual problems (skipped items and why).
    pub messages: Vec<String>,
    pub balances: Vec<BalanceLine>,
}

impl ImportReport {
    fn counts(&mut self, entity: &str) -> &mut Counts {
        self.entities.entry(entity.to_string()).or_default()
    }

    pub fn created(&mut self, entity: &str) {
        self.counts(entity).created += 1;
    }

    pub fn updated(&mut self, entity: &str) {
        self.counts(entity).updated += 1;
    }

    pub fn unchanged(&mut self, entity: &str) {
        self.counts(entity).unchanged += 1;
    }

    pub fn skipped(&mut self, entity: &str, why: String) {
        self.counts(entity).skipped += 1;
        self.messages.push(format!("{entity}: {why}"));
    }

    /// A message about an item that was still imported (for example, a rule
    /// imported inactive).
    pub fn message(&mut self, entity: &str, what: String) {
        self.messages.push(format!("{entity}: {what}"));
    }

    pub fn note(&mut self, what: &str) {
        *self.notes.entry(what.to_string()).or_default() += 1;
    }

    /// True if anything was created or updated.
    pub fn has_changes(&self) -> bool {
        self.entities
            .values()
            .any(|c| c.created > 0 || c.updated > 0)
    }

    pub fn balance_mismatches(&self) -> usize {
        self.balances.iter().filter(|b| !b.matches()).count()
    }

    /// A plain-text summary for the terminal.
    pub fn render(&self) -> String {
        let mut out = String::new();
        let mode = if self.dry_run {
            " (dry run: nothing was saved)"
        } else {
            ""
        };
        let _ = writeln!(out, "Import summary{mode}");
        let _ = writeln!(
            out,
            "{:<14} {:>8} {:>8} {:>10} {:>8}",
            "entity", "created", "updated", "unchanged", "skipped"
        );
        for (entity, c) in &self.entities {
            let _ = writeln!(
                out,
                "{entity:<14} {:>8} {:>8} {:>10} {:>8}",
                c.created, c.updated, c.unchanged, c.skipped
            );
        }
        if !self.notes.is_empty() {
            let _ = writeln!(out, "\nNotes");
            for (note, n) in &self.notes {
                let _ = writeln!(out, "  {n:>6}  {note}");
            }
        }
        if !self.messages.is_empty() {
            let _ = writeln!(out, "\nSkipped or changed");
            for message in &self.messages {
                let _ = writeln!(out, "  {message}");
            }
        }
        if !self.balances.is_empty() {
            let mismatches = self.balance_mismatches();
            let _ = writeln!(
                out,
                "\nBalance check: {} accounts, {mismatches} mismatches",
                self.balances.len()
            );
            for line in self.balances.iter().filter(|b| !b.matches()) {
                let _ = writeln!(
                    out,
                    "  MISMATCH {} ({}): Firefly {} / Saldo {}",
                    line.account, line.currency, line.firefly, line.saldo
                );
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_notes_and_renders() {
        let mut report = ImportReport::default();
        assert!(!report.has_changes());
        report.unchanged("tags");
        assert!(!report.has_changes());
        report.created("accounts");
        report.skipped("accounts", "import account \"x\"".into());
        report.note("kind differs");
        report.note("kind differs");
        report.balances.push(BalanceLine {
            account: "A".into(),
            currency: "EUR".into(),
            firefly: "1.00".into(),
            saldo: "2.00".into(),
        });
        assert!(report.has_changes());
        assert_eq!(
            report.entities["accounts"],
            Counts {
                created: 1,
                updated: 0,
                unchanged: 0,
                skipped: 1
            }
        );
        assert_eq!(report.notes["kind differs"], 2);
        assert_eq!(report.balance_mismatches(), 1);
        let text = report.render();
        assert!(
            text.contains("MISMATCH A (EUR)")
                && text.contains("kind differs")
                && text.contains("import account")
        );
    }
}
