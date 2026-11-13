//! The JSON shapes of the reports. Amounts are decimal strings; each
//! currency is reported on its own (no conversion), primary currency first.

use serde::Serialize;

/// What every report echoes back: the span and the interval it was cut into.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct ReportRange {
    pub from: String,
    pub to: String,
    /// `day`, `week`, `month` or `year`.
    pub interval: String,
}

/// Income (deposits) and expenses (withdrawals) per interval. Transfers,
/// opening balances and reconciliations are neither.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct CashFlowReport {
    pub range: ReportRange,
    pub currencies: Vec<CashFlowCurrency>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct CashFlowCurrency {
    pub currency: String,
    pub income: String,
    pub expenses: String,
    /// Income minus expenses.
    pub net: String,
    pub intervals: Vec<CashFlowInterval>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct CashFlowInterval {
    pub start: String,
    pub end: String,
    pub income: String,
    pub expenses: String,
    pub net: String,
}

/// Income and expenses per category or per tag. A split with several tags
/// counts once for each; `id: null` collects splits without one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct BreakdownReport {
    pub range: ReportRange,
    pub currencies: Vec<BreakdownCurrency>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct BreakdownCurrency {
    pub currency: String,
    /// Largest expenses first.
    pub items: Vec<BreakdownItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct BreakdownItem {
    /// The category or tag; `null` for splits without one.
    pub id: Option<String>,
    pub name: Option<String>,
    pub income: String,
    pub expenses: String,
    pub intervals: Vec<BreakdownInterval>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct BreakdownInterval {
    pub start: String,
    pub end: String,
    pub income: String,
    pub expenses: String,
}

/// Per budget: the limits starting in each interval (budgeted) and the
/// withdrawals booked on it (spent). `budget_id: null` is spending without
/// a budget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct BudgetReport {
    pub range: ReportRange,
    pub currencies: Vec<BudgetReportCurrency>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct BudgetReportCurrency {
    pub currency: String,
    pub items: Vec<BudgetReportItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct BudgetReportItem {
    pub budget_id: Option<String>,
    pub name: Option<String>,
    pub budgeted: String,
    pub spent: String,
    pub intervals: Vec<BudgetReportInterval>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct BudgetReportInterval {
    pub start: String,
    pub end: String,
    pub budgeted: String,
    pub spent: String,
}

/// The sum of the asset and liability balances counted in net worth, at the
/// end of each interval, per account currency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct NetWorthReport {
    pub range: ReportRange,
    pub currencies: Vec<NetWorthCurrency>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct NetWorthCurrency {
    pub currency: String,
    /// Net worth at the end of the day before `from`.
    pub opening: String,
    pub intervals: Vec<NetWorthInterval>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct NetWorthInterval {
    pub start: String,
    pub end: String,
    /// Net worth at the end of the interval.
    pub value: String,
    /// The change during the interval.
    pub change: String,
}
