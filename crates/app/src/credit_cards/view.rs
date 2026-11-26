//! The JSON shapes of credit card plans and months. Amounts are decimal
//! strings in the card's currency; months are `YYYY-MM`.

use serde::Serialize;

/// A card's plan, as the API returns it and as plan events carry it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct CreditCardPlanView {
    pub account_id: String,
    pub currency: String,
    /// The asset account that pays the card.
    pub settlement_account_id: String,
    /// Day of the following month the card is paid (1 to 31; the last day
    /// of a shorter month).
    pub settlement_day: u8,
    /// The first month the plan covers.
    pub starts_cycle: String,
    /// The budgets, oldest first: each applies from its month until the next.
    pub limits: Vec<CardLimitView>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct CardLimitView {
    pub from_cycle: String,
    pub amount: String,
}

/// One month of a card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct CardCycleView {
    pub cycle: String,
    pub from: String,
    pub to: String,
    pub currency: String,
    /// The month's budget.
    pub limit: String,
    /// Purchases minus refunds counted in this month (after moves).
    pub spent: String,
    /// `limit - spent`; negative when over budget.
    pub available: String,
    /// Spending moved here from the month before.
    pub moved_in: String,
    /// Spending of this month's dates moved to the next month.
    pub moved_out: String,
    pub settlement: SettlementView,
    /// For a frozen or hand-managed month: `spent` minus what the transfer
    /// paid. `None` while the server keeps the transfer in sync.
    pub difference_after_settlement: Option<String>,
}

/// The month's settlement transfer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct SettlementView {
    /// `synced` (follows the month), `frozen` (booked) or `detached` (managed
    /// by hand).
    pub state: String,
    /// The payment day of the following month.
    pub date: String,
    /// The transfer, when there is one.
    pub group_id: Option<String>,
    pub amount: Option<String>,
}

/// One month with the transactions that count toward it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct CardCycleDetailView {
    #[serde(flatten)]
    pub cycle: CardCycleView,
    /// Oldest first: the month's own transactions (also those moved out,
    /// which do not count here) and those moved in from the month before.
    pub splits: Vec<CardSplitView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct CardSplitView {
    pub split_id: String,
    pub group_id: String,
    pub date: String,
    pub description: String,
    pub counterparty: CounterpartyView,
    /// Positive for a purchase, negative for a refund.
    pub amount: String,
    /// Dated in the month before and counted here.
    pub moved_in: bool,
    /// Dated in this month and counted in the next.
    pub moved_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct CounterpartyView {
    pub id: String,
    pub name: String,
}

/// The data of `credit_card_move.created` and `.deleted`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct CardMoveView {
    pub account_id: String,
    pub split_id: String,
    pub from_cycle: String,
    pub to_cycle: String,
}

/// A card's current month, for the accounts list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct CardSummaryView {
    pub account_id: String,
    pub account_name: String,
    pub current: CardCycleView,
}

/// A move as stored, for the export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct CardMoveRecordView {
    pub split_id: String,
    pub account_id: String,
    pub created_at: String,
}
