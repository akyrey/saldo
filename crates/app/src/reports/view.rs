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
    /// Income minus expenses: what was saved.
    pub net: String,
    /// `net / income` in basis points (2500 = 25%); negative when spending
    /// was larger, `null` without income.
    pub savings_rate_bp: Option<i64>,
    pub intervals: Vec<CashFlowInterval>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct CashFlowInterval {
    pub start: String,
    pub end: String,
    pub income: String,
    pub expenses: String,
    pub net: String,
    pub savings_rate_bp: Option<i64>,
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

/// One amount per month for several years, to compare like with like.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct YearOverYearReport {
    /// `expenses`, `income` or `savings` (income minus expenses).
    pub metric: String,
    /// The category it is limited to (`none`: without a category).
    pub category_id: Option<String>,
    pub currencies: Vec<YearOverYearCurrency>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct YearOverYearCurrency {
    pub currency: String,
    /// Oldest year first.
    pub years: Vec<YearOverYearYear>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct YearOverYearYear {
    pub year: i32,
    /// January to December; `null` for months still to come.
    pub months: Vec<Option<String>>,
    /// Each month against the same month a year earlier, in basis points;
    /// `null` for months to come, the first year, or nothing a year earlier.
    pub month_change_bp: Vec<Option<i64>>,
    /// The sum of the months so far.
    pub total: String,
    /// `total` minus the previous year's total over the same months.
    pub change: Option<String>,
    pub change_bp: Option<i64>,
}

/// Average monthly spending per category over complete months, against this
/// month so far.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct CategoryAveragesReport {
    /// How many complete months the averages cover.
    pub months: u32,
    /// The first and last day of those months.
    pub from: String,
    pub to: String,
    pub currencies: Vec<CategoryAveragesCurrency>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct CategoryAveragesCurrency {
    pub currency: String,
    /// Largest average first.
    pub items: Vec<CategoryAverageItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct CategoryAverageItem {
    /// `null`: withdrawals without a category.
    pub id: Option<String>,
    pub name: Option<String>,
    /// Months without spending count as zero.
    pub average: String,
    pub min: String,
    pub max: String,
    pub this_month: String,
    /// `(this_month - average) / average`; `null` when the average is zero.
    pub this_month_vs_average_bp: Option<i64>,
}
