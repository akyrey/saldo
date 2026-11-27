//! What the source service returns and accepts.

use chrono::NaiveDate;
use serde::Serialize;

/// An Amazon or PayPal export to store.
#[derive(Debug, Clone)]
pub struct SourceUpload {
    /// `amazon_orders` or `paypal`.
    pub kind: String,
    pub filename: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct DocumentView {
    pub id: String,
    pub kind: String,
    pub filename: String,
    /// The earliest and latest row dates in the file.
    pub first_date: Option<String>,
    pub last_date: Option<String>,
    pub row_count: i64,
    /// Rows this upload added, and known rows it changed.
    pub rows_added: i64,
    pub rows_updated: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct SourceRowView {
    pub id: String,
    pub kind: String,
    pub external_id: String,
    pub date: String,
    pub time: Option<String>,
    /// Signed as the source writes it.
    pub amount: String,
    pub currency: String,
    /// Amazon `item`; PayPal `funding`, `withdrawal`, `payment`, `refund` or `other`.
    pub row_kind: String,
    /// Amazon: the order ID.
    pub group_key: Option<String>,
    pub counterparty: Option<String>,
    pub text: Option<String>,
    pub status: Option<String>,
    /// Never reaches a bank (cancelled, or paid by gift card alone).
    pub excluded: bool,
    /// Explains a statement line or a transaction.
    pub matched: bool,
}

/// Filters for listing rows; `None` means any.
#[derive(Debug, Clone, Default)]
pub struct RowFilter {
    pub kind: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    /// Only rows that explain nothing yet.
    pub unmatched_only: bool,
}
