//! Firefly III shapes for piggy banks and recurrences (API v1, as in the
//! public specification 6.7.8; the owner's instance has no piggy banks).
//! Only the fields the import needs; unknown fields are ignored.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PiggyBank {
    pub name: String,
    pub currency_code: Option<String>,
    /// `null` (or zero) means no target.
    pub target_amount: Option<String>,
    /// Date-times such as `2026-10-01T00:00:00+00:00`.
    pub start_date: Option<String>,
    pub target_date: Option<String>,
    pub order: Option<i64>,
    pub active: Option<bool>,
    pub notes: Option<String>,
    #[serde(default)]
    pub accounts: Vec<PiggyBankAccount>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PiggyBankAccount {
    pub account_id: String,
    pub current_amount: Option<String>,
}

/// `GET /v1/piggy-banks/{id}/events`. An event names no account; the
/// journal (a transfer) or the piggy bank's accounts decide.
#[derive(Debug, Clone, Deserialize)]
pub struct PiggyBankEvent {
    pub created_at: String,
    /// Signed: negative removes money.
    pub amount: String,
    pub currency_code: Option<String>,
    pub transaction_journal_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Recurrence {
    pub title: String,
    pub description: Option<String>,
    pub first_date: String,
    /// The last date Firefly fired it.
    pub latest_date: Option<String>,
    pub repeat_until: Option<String>,
    pub nr_of_repetitions: Option<u32>,
    pub apply_rules: Option<bool>,
    pub active: Option<bool>,
    pub notes: Option<String>,
    #[serde(default)]
    pub repetitions: Vec<RecurrenceRepetition>,
    #[serde(default)]
    pub transactions: Vec<RecurrenceTransaction>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecurrenceRepetition {
    /// `daily`, `weekly`, `monthly`, `ndom`, `yearly`.
    #[serde(rename = "type")]
    pub kind: String,
    pub moment: String,
    #[serde(default)]
    pub skip: u32,
    /// 1 keep, 2 skip, 3 previous Friday, 4 next Monday.
    pub weekend: Option<u8>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecurrenceTransaction {
    pub description: String,
    pub amount: String,
    pub currency_code: String,
    pub foreign_amount: Option<String>,
    pub foreign_currency_code: Option<String>,
    pub source_id: String,
    pub source_type: Option<String>,
    pub destination_id: String,
    pub destination_type: Option<String>,
    pub budget_id: Option<String>,
    pub category_id: Option<String>,
    pub piggy_bank_id: Option<String>,
    pub subscription_id: Option<String>,
    /// Deprecated name of `subscription_id`.
    pub bill_id: Option<String>,
    pub tags: Option<Vec<String>>,
}
