//! The JSON shape of "safe to spend this month".

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct SafeToSpendView {
    /// The first and last day of the current month (household time).
    pub from: String,
    pub to: String,
    /// The household's primary currency; other currencies are left out.
    pub currency: String,
    /// Income (received and planned) minus everything spent or committed.
    pub safe_to_spend: String,
    /// `safe_to_spend` over the days left, today included, rounded down.
    pub per_day: String,
    pub days_left: i64,
    pub parts: SafeToSpendParts,
    /// What each part is made of (not `income_received` and `spent`, which
    /// are this month's cash flow).
    pub items: Vec<SafeToSpendItem>,
}

/// Every part is zero or more; the formula adds the first two and
/// subtracts the rest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct SafeToSpendParts {
    /// Deposits from the 1st of the month to today.
    pub income_received: String,
    /// Deposits planned for the rest of the month (recurrences, transactions
    /// dated later).
    pub income_planned: String,
    /// Withdrawals from the 1st of the month to today.
    pub spent: String,
    /// Bills still due this month (open and overdue), at their highest amount.
    pub bills: String,
    /// Other withdrawals planned for the rest of the month.
    pub planned: String,
    /// What budgets have left for the rest of the month (prorated by days
    /// for limits running past it).
    pub budgets: String,
    /// What piggy banks with a target date need this month.
    pub piggy_banks: String,
    /// Planned transfers into savings accounts.
    pub savings_transfers: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct SafeToSpendItem {
    /// The part it belongs to, as named in `parts`.
    pub part: String,
    /// `transaction`, `recurrence`, `bill`, `budget` or `piggy_bank`.
    pub source: String,
    pub source_id: String,
    pub title: String,
    pub date: Option<String>,
    /// Always positive: the formula gives the sign.
    pub amount: String,
}
