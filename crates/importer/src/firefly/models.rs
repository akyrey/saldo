//! Firefly III API shapes, with only the fields the import needs. Unknown
//! fields are ignored, so new Firefly versions do not break the import.
//!
//! `#[derive(Deserialize)]` plus `#[serde(...)]` attributes play the role of
//! Go struct tags for `encoding/json`. A JSON field that may be `null` or
//! missing becomes an `Option`: the compiler then makes the code handle it.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

/// One object of a JSON:API response: `{ "id": "...", "attributes": {...} }`.
#[derive(Debug, Clone, Deserialize)]
pub struct Resource<T> {
    pub id: String,
    pub attributes: T,
}

/// A page of a list endpoint.
#[derive(Debug, Deserialize)]
pub struct Page<T> {
    pub data: Vec<Resource<T>>,
    pub meta: Meta,
}

#[derive(Debug, Deserialize)]
pub struct Meta {
    pub pagination: Pagination,
}

#[derive(Debug, Deserialize)]
pub struct Pagination {
    pub total: u64,
    pub total_pages: u32,
}

/// `GET /v1/about`.
#[derive(Debug, Deserialize)]
pub struct About {
    pub version: String,
    pub api_version: String,
}

#[derive(Debug, Deserialize)]
pub struct AboutResponse {
    pub data: About,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Currency {
    pub code: String,
    pub name: String,
    pub symbol: String,
    pub decimal_places: u8,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Account {
    pub name: String,
    /// `asset`, `expense`, `revenue`, `liabilities`, `cash`,
    /// `initial-balance`, `reconciliation`, `import`.
    #[serde(rename = "type")]
    pub account_type: String,
    pub account_role: Option<String>,
    pub currency_code: String,
    pub active: bool,
    pub current_balance: Option<String>,
    pub current_balance_date: Option<String>,
    pub virtual_balance: Option<String>,
    pub iban: Option<String>,
    pub bic: Option<String>,
    pub account_number: Option<String>,
    pub liability_type: Option<String>,
    pub liability_direction: Option<String>,
    pub interest: Option<String>,
    pub interest_period: Option<String>,
    pub include_net_worth: Option<bool>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Category {
    pub name: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Tag {
    pub tag: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransactionGroup {
    pub group_title: Option<String>,
    pub transactions: Vec<Split>,
}

/// One split ("transaction journal" in Firefly).
#[derive(Debug, Clone, Deserialize)]
pub struct Split {
    pub transaction_journal_id: String,
    /// `withdrawal`, `deposit`, `transfer`, `opening balance`, `reconciliation`.
    #[serde(rename = "type")]
    pub split_type: String,
    /// Local date-time with offset; the first 10 characters are the date.
    pub date: String,
    pub amount: String,
    pub currency_code: String,
    pub foreign_amount: Option<String>,
    pub foreign_currency_code: Option<String>,
    pub description: String,
    pub source_id: String,
    pub destination_id: String,
    pub category_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub reconciled: bool,
    pub notes: Option<String>,
    /// Every other field. The SEPA fields, extra dates and references are
    /// picked from here into `meta.firefly`.
    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Attachment {
    /// `TransactionJournal`, `Bill`, ...
    pub attachable_type: String,
    pub attachable_id: String,
    pub filename: String,
    pub title: Option<String>,
    pub notes: Option<String>,
    pub mime: Option<String>,
}

/// A subscription (formerly "bill") from `/v1/subscriptions`.
#[derive(Debug, Clone, Deserialize)]
pub struct Subscription {
    pub name: String,
    pub amount_min: String,
    pub amount_max: String,
    pub currency_code: String,
    /// The first due date, as a local date-time.
    pub date: String,
    pub end_date: Option<String>,
    /// `weekly`, `monthly`, `quarterly`, `half-year` or `yearly`.
    pub repeat_freq: String,
    #[serde(default)]
    pub skip: u32,
    pub active: bool,
    pub order: Option<i64>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuleGroup {
    pub title: String,
    pub description: Option<String>,
    pub order: Option<i64>,
    pub active: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub rule_group_id: String,
    pub title: String,
    pub description: Option<String>,
    /// `store-journal` (on create), `update-journal`, or manual.
    pub trigger: String,
    pub active: bool,
    pub strict: bool,
    pub stop_processing: bool,
    pub order: Option<i64>,
    #[serde(default)]
    pub triggers: Vec<RulePart>,
    #[serde(default)]
    pub actions: Vec<RulePart>,
}

/// A trigger (`prohibited` = negated) or an action of a rule.
#[derive(Debug, Clone, Deserialize)]
pub struct RulePart {
    #[serde(rename = "type")]
    pub kind: String,
    pub value: Option<String>,
    #[serde(default)]
    pub prohibited: bool,
    #[serde(default)]
    pub stop_processing: bool,
    #[serde(default = "yes")]
    pub active: bool,
    pub order: Option<i64>,
}

fn yes() -> bool {
    true
}
