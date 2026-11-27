//! How a profile reads a spreadsheet or CSV: which columns hold what, and
//! the presets for the owner's banks (Phase 13, Package 0).

use serde::{Deserialize, Serialize};

/// The header names a column may have, compared ignoring case and
/// surrounding spaces; the first one present wins (L5).
pub type ColumnNames = Vec<String>;

/// Where the amount is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AmountColumns {
    /// One signed column: negative when money left the account.
    Signed { column: ColumnNames },
    /// Money in and money out in separate columns. A debit is always made
    /// negative, whatever sign the bank wrote (L4).
    Split {
        credit: ColumnNames,
        debit: ColumnNames,
    },
}

/// A line whose `column` contains `contains` (ignoring case) is left out
/// with `reason`, for example Satispay's cancelled payments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkipRule {
    pub column: ColumnNames,
    pub contains: String,
    pub reason: String,
}

/// Where a date rule moves a line (L8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BookOn {
    /// The last day of the month before the accounting date: a fee charged
    /// on the 16th for the previous month.
    PreviousMonthEnd,
}

/// A line whose description contains `contains` (ignoring case) is booked
/// on another date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateRule {
    pub contains: String,
    pub book_on: BookOn,
}

/// How to read one bank's export. Stored as JSON on a statement profile;
/// an unknown field is refused, so a typo fails instead of being ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    /// The xlsx sheet; the first sheet when absent.
    #[serde(default)]
    pub sheet: Option<String>,
    /// The accounting date. Its header also finds the header row (L1).
    pub date_column: ColumnNames,
    /// The value date; the accounting date when absent.
    #[serde(default)]
    pub value_date_column: Option<ColumnNames>,
    /// chrono formats for dates written as text, tried in order (spreadsheet
    /// date cells need none).
    #[serde(default)]
    pub date_formats: Vec<String>,
    pub amount: AmountColumns,
    /// For amounts written as text: `,` for "1.234,56", `.` for "1,234.56".
    pub decimal_separator: char,
    /// Joined with " - " into the description, empty cells skipped.
    pub description_columns: Vec<ColumnNames>,
    /// Kept apart for transfer rules and display (Widiba's CAUSALE,
    /// Satispay's Type).
    #[serde(default)]
    pub category_column: Option<ColumnNames>,
    #[serde(default)]
    pub counterparty_column: Option<ColumnNames>,
    #[serde(default)]
    pub counterparty_iban_column: Option<ColumnNames>,
    #[serde(default)]
    pub external_id_column: Option<ColumnNames>,
    /// The line's currency; the account's when absent.
    #[serde(default)]
    pub currency_column: Option<ColumnNames>,
    /// Non-empty cells become "Header: value" notes (Satispay's vouchers).
    #[serde(default)]
    pub note_columns: Vec<ColumnNames>,
    /// Labelled cells anywhere in the sheet; the value is the first
    /// non-empty cell to the right of the label (L1).
    #[serde(default)]
    pub opening_balance_label: Option<String>,
    #[serde(default)]
    pub closing_balance_label: Option<String>,
    /// The balance after each line (Satispay), which gives the opening and
    /// closing balances when there are no labels.
    #[serde(default)]
    pub running_balance_column: Option<ColumnNames>,
    /// Lines whose description contains one of these (ignoring case) are
    /// pending and never imported (decision 12).
    #[serde(default)]
    pub pending_when: Vec<String>,
    #[serde(default)]
    pub skip_when: Vec<SkipRule>,
    #[serde(default)]
    pub date_rules: Vec<DateRule>,
}

/// Most rules, column names or date formats of one kind in a mapping.
const MAX_RULES: usize = 50;
/// Longest name, format or rule text in a mapping.
const MAX_TEXT_CHARS: usize = 200;

/// Why a mapping cannot be saved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MappingError {
    #[error("a column needs at least one name")]
    NoColumnNames,
    #[error("a rule or name is empty")]
    EmptyText,
    #[error("a rule or name is longer than {MAX_TEXT_CHARS} characters")]
    TextTooLong,
    #[error("more than {MAX_RULES} rules, names or formats of one kind")]
    TooManyRules,
    #[error("the decimal separator must be ',' or '.', not {0:?}")]
    DecimalSeparator(char),
}

impl Mapping {
    /// Checks a mapping the owner wrote before it is saved: an empty rule
    /// text would match every line, and the limits keep a parse cheap.
    pub fn validate(&self) -> Result<(), MappingError> {
        if !matches!(self.decimal_separator, ',' | '.') {
            return Err(MappingError::DecimalSeparator(self.decimal_separator));
        }
        let columns = self.columns();
        if columns.iter().any(|names| names.is_empty()) {
            return Err(MappingError::NoColumnNames);
        }
        let lists = [
            self.date_formats.len(),
            self.description_columns.len(),
            self.note_columns.len(),
            self.pending_when.len(),
            self.skip_when.len(),
            self.date_rules.len(),
        ];
        let too_many = lists.iter().any(|len| *len > MAX_RULES)
            || columns.iter().any(|names| names.len() > MAX_RULES);
        if too_many {
            return Err(MappingError::TooManyRules);
        }
        check_texts(&self.texts(&columns))
    }

    /// Every column the mapping names.
    fn columns(&self) -> Vec<&ColumnNames> {
        let mut columns: Vec<&ColumnNames> = vec![&self.date_column];
        columns.extend(self.description_columns.iter());
        columns.extend(self.note_columns.iter());
        columns.extend(self.skip_when.iter().map(|rule| &rule.column));
        match &self.amount {
            AmountColumns::Signed { column } => columns.push(column),
            AmountColumns::Split { credit, debit } => columns.extend([credit, debit]),
        }
        let optional = [
            &self.value_date_column,
            &self.category_column,
            &self.counterparty_column,
            &self.counterparty_iban_column,
            &self.external_id_column,
            &self.currency_column,
            &self.running_balance_column,
        ];
        columns.extend(optional.into_iter().flatten());
        columns
    }

    /// Every piece of text the owner typed: names, formats, rules, labels.
    // `'a` is a lifetime: the returned `&str`s point into `self` and
    // `columns`, so they may not outlive them. Like a Go slice of strings
    // that borrows from a struct, except the compiler checks it.
    fn texts<'a>(&'a self, columns: &[&'a ColumnNames]) -> Vec<&'a str> {
        let mut texts: Vec<&str> = columns
            .iter()
            .flat_map(|names| names.iter().map(String::as_str))
            .collect();
        texts.extend(self.date_formats.iter().map(String::as_str));
        texts.extend(self.pending_when.iter().map(String::as_str));
        texts.extend(
            self.skip_when
                .iter()
                .flat_map(|rule| [rule.contains.as_str(), rule.reason.as_str()]),
        );
        texts.extend(self.date_rules.iter().map(|rule| rule.contains.as_str()));
        let labels = [
            &self.sheet,
            &self.opening_balance_label,
            &self.closing_balance_label,
        ];
        texts.extend(labels.into_iter().flatten().map(String::as_str));
        texts
    }
}

fn check_texts(texts: &[&str]) -> Result<(), MappingError> {
    for text in texts {
        if text.trim().is_empty() {
            return Err(MappingError::EmptyText);
        }
        if text.chars().count() > MAX_TEXT_CHARS {
            return Err(MappingError::TextTooLong);
        }
    }
    Ok(())
}

fn names(list: &[&str]) -> ColumnNames {
    list.iter().map(|name| name.to_string()).collect()
}

impl Mapping {
    /// Widiba's "Movimenti conto" xlsx.
    pub fn widiba() -> Self {
        Mapping {
            sheet: None,
            date_column: names(&["DATA CONT."]),
            value_date_column: Some(names(&["DATA VAL."])),
            date_formats: vec!["%d/%m/%Y".to_string()],
            amount: AmountColumns::Signed {
                column: names(&["IMPORTO (€)(€)", "IMPORTO (€)", "IMPORTO"]),
            },
            decimal_separator: ',',
            description_columns: vec![names(&["CAUSALE"]), names(&["DESCRIZIONE"])],
            category_column: Some(names(&["CAUSALE"])),
            counterparty_column: None,
            counterparty_iban_column: None,
            external_id_column: None,
            currency_column: None,
            note_columns: vec![],
            opening_balance_label: Some("Saldo iniziale (€)".to_string()),
            closing_balance_label: Some("Saldo finale (€)".to_string()),
            running_balance_column: None,
            pending_when: vec![],
            skip_when: vec![],
            date_rules: vec![DateRule {
                contains: "Addebito Pacchetto".to_string(),
                book_on: BookOn::PreviousMonthEnd,
            }],
        }
    }

    /// Mediobanca's xlsx: only a value date, written as text.
    pub fn mediobanca() -> Self {
        Mapping {
            sheet: None,
            date_column: names(&["Data contabile", "Data valuta"]),
            value_date_column: Some(names(&["Data valuta"])),
            date_formats: vec!["%d/%m/%Y".to_string()],
            amount: AmountColumns::Split {
                credit: names(&["Entrate"]),
                debit: names(&["Uscite"]),
            },
            decimal_separator: ',',
            description_columns: vec![names(&["Tipologia"])],
            category_column: None,
            counterparty_column: None,
            counterparty_iban_column: None,
            external_id_column: None,
            currency_column: Some(names(&["Divisa"])),
            note_columns: vec![],
            opening_balance_label: None,
            closing_balance_label: None,
            running_balance_column: None,
            pending_when: vec![],
            skip_when: vec![],
            date_rules: vec![],
        }
    }

    /// The Satispay app's xlsx. `Balance` is the part paid from the Satispay
    /// balance; vouchers paid the rest (Package 0).
    pub fn satispay() -> Self {
        Mapping {
            sheet: None,
            date_column: names(&["Date"]),
            value_date_column: None,
            date_formats: vec!["%d/%m/%Y %H:%M".to_string(), "%d/%m/%Y".to_string()],
            amount: AmountColumns::Signed {
                column: names(&["Balance"]),
            },
            decimal_separator: '.',
            description_columns: vec![names(&["Name"])],
            category_column: Some(names(&["Type"])),
            counterparty_column: Some(names(&["Name"])),
            counterparty_iban_column: None,
            external_id_column: Some(names(&[
                "ID (Share this with Customer Support in case of issues)",
                "ID",
            ])),
            currency_column: None,
            note_columns: vec![names(&["Meal Vouchers"]), names(&["Shopping Vouchers"])],
            opening_balance_label: None,
            closing_balance_label: None,
            running_balance_column: Some(names(&["Balance after transaction"])),
            pending_when: vec![],
            skip_when: vec![SkipRule {
                column: names(&["Status"]),
                contains: "cancel".to_string(),
                reason: "cancelled".to_string(),
            }],
            date_rules: vec![],
        }
    }
}
