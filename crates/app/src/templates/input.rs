//! What a client sends for a template, and the checks that turn it into the
//! stored JSON. A template is checked only for what it contains; the full
//! ledger rules run when a transaction made from it is saved.

use std::collections::BTreeSet;
use std::str::FromStr;

use saldo_core::{AccountId, BillId, BudgetId, CategoryId, Money, SplitKind, TagId};
use sqlx::SqliteConnection;

use super::view::{Known, TransactionTemplateSplitView};
use crate::ctx::HouseholdCtx;
use crate::error::{AppError, parse_stored};
use crate::{currencies, identity};

/// The kinds a template can have (no opening balances or reconciliations).
pub const TEMPLATE_KINDS: [&str; 3] = ["withdrawal", "deposit", "transfer"];

/// A new template. `sort_order` defaults to after the last template.
#[derive(Debug, Clone, Default)]
pub struct NewTemplate {
    pub name: String,
    pub kind: String,
    pub title: Option<String>,
    pub sort_order: Option<i64>,
    pub splits: Vec<TemplateSplitInput>,
}

/// `None` keeps a value; `title: Some(None)` clears the title and
/// `splits: Some(...)` replaces all splits.
#[derive(Debug, Clone, Default)]
pub struct TemplateUpdate {
    pub name: Option<String>,
    pub kind: Option<String>,
    pub title: Option<Option<String>>,
    pub sort_order: Option<i64>,
    pub splits: Option<Vec<TemplateSplitInput>>,
}

/// One partial split; every field is optional.
#[derive(Debug, Clone, Default)]
pub struct TemplateSplitInput {
    pub description: Option<String>,
    pub source_id: Option<AccountId>,
    pub destination_id: Option<AccountId>,
    pub amount: Option<String>,
    pub currency: Option<String>,
    pub foreign_amount: Option<String>,
    pub foreign_currency: Option<String>,
    pub category_id: Option<CategoryId>,
    pub budget_id: Option<BudgetId>,
    pub bill_id: Option<BillId>,
    pub tag_ids: Vec<TagId>,
    pub notes: Option<String>,
}

/// Trims text; empty text means "not set".
pub(super) fn text(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

impl TemplateSplitInput {
    /// The stored form, before checks. A set drops duplicate tags.
    fn into_view(self) -> TransactionTemplateSplitView {
        let tags: BTreeSet<String> = self.tag_ids.iter().map(ToString::to_string).collect();
        TransactionTemplateSplitView {
            description: text(self.description),
            source_id: self.source_id.map(|id| id.to_string()),
            destination_id: self.destination_id.map(|id| id.to_string()),
            amount: text(self.amount),
            currency: text(self.currency),
            foreign_amount: text(self.foreign_amount),
            foreign_currency: text(self.foreign_currency),
            category_id: self.category_id.map(|id| id.to_string()),
            budget_id: self.budget_id.map(|id| id.to_string()),
            bill_id: self.bill_id.map(|id| id.to_string()),
            tag_ids: tags.into_iter().collect(),
            notes: text(self.notes),
        }
    }
}

/// The stored name of a template kind.
pub(super) fn kind_name(kind: SplitKind) -> &'static str {
    match kind {
        SplitKind::Withdrawal => "withdrawal",
        SplitKind::Deposit => "deposit",
        SplitKind::Transfer => "transfer",
        SplitKind::OpeningBalance => "opening_balance",
        SplitKind::Reconciliation => "reconciliation",
    }
}

/// Checks the name and kind; returns them trimmed.
pub(super) fn name_and_kind(name: &str, kind: &str) -> Result<(String, SplitKind), AppError> {
    let name = identity::required("name", name)?;
    let kind = match kind.trim() {
        "withdrawal" => SplitKind::Withdrawal,
        "deposit" => SplitKind::Deposit,
        "transfer" => SplitKind::Transfer,
        _ => {
            return Err(AppError::invalid(
                "kind",
                "must be withdrawal, deposit or transfer",
            ));
        }
    };
    Ok((name, kind))
}

/// Checks every split and returns the stored JSON. Errors name the field,
/// for example `splits[1].amount`.
pub(super) async fn check_splits(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    kind: SplitKind,
    splits: Vec<TemplateSplitInput>,
) -> Result<String, AppError> {
    let splits: Vec<TransactionTemplateSplitView> =
        splits.into_iter().map(|s| s.into_view()).collect();
    let known = Known::load(conn, ctx, splits.iter()).await?;
    let primary = identity::primary_currency(conn, ctx).await?;
    let mut checked = Vec::with_capacity(splits.len());
    for (index, split) in splits.into_iter().enumerate() {
        let field = |name: &str| format!("splits[{index}].{name}");
        if let Some((name, message)) = known.first_unknown(&split) {
            return Err(AppError::invalid(field(name), message));
        }
        check_kind(&known, &split, kind)
            .map_err(|(name, message)| AppError::invalid(field(name), message))?;
        checked.push(check_amounts(conn, index, &known, &primary, split).await?);
    }
    serde_json::to_string(&checked)
        .map_err(|e| AppError::Internal(format!("cannot encode template splits: {e}")))
}

/// Two given accounts must form the template's kind; only withdrawals have
/// a budget.
fn check_kind(
    known: &Known,
    split: &TransactionTemplateSplitView,
    kind: SplitKind,
) -> Result<(), (&'static str, String)> {
    if split.budget_id.is_some() && kind != SplitKind::Withdrawal {
        return Err(("budget_id", "only withdrawals can have a budget".into()));
    }
    let account = |id: &Option<String>| id.as_ref().and_then(|i| known.accounts.get(i));
    if let (Some(source), Some(destination)) =
        (account(&split.source_id), account(&split.destination_id))
    {
        let derived = SplitKind::derive(source.account_type, destination.account_type).ok();
        if derived != Some(kind) {
            return Err((
                "destination_id",
                "the two accounts do not form the template's kind".into(),
            ));
        }
    }
    Ok(())
}

/// Parses the amounts in their currencies and stores them normalised
/// (`"12.5"` becomes `"12.50"` in EUR).
async fn check_amounts(
    conn: &mut SqliteConnection,
    index: usize,
    known: &Known,
    primary: &str,
    split: TransactionTemplateSplitView,
) -> Result<TransactionTemplateSplitView, AppError> {
    let field = |name: &str| format!("splits[{index}].{name}");
    let code = split
        .currency
        .clone()
        .or_else(|| home_currency(known, &split))
        .unwrap_or_else(|| primary.to_string());
    let currency = currencies::find(conn, &code, &field("currency")).await?;
    let amount = match &split.amount {
        Some(text) => Some(positive(text, &currency, &field("amount"))?),
        None => None,
    };
    let foreign_amount = match (&split.foreign_amount, &split.foreign_currency) {
        (Some(text), Some(code)) => {
            let currency = currencies::find(conn, code, &field("foreign_currency")).await?;
            Some(positive(text, &currency, &field("foreign_amount"))?)
        }
        (Some(_), None) => {
            return Err(AppError::invalid(
                field("foreign_currency"),
                "is required with a foreign amount",
            ));
        }
        (None, Some(code)) => {
            currencies::find(conn, code, &field("foreign_currency")).await?;
            None
        }
        (None, None) => None,
    };
    Ok(TransactionTemplateSplitView {
        amount,
        foreign_amount,
        ..split
    })
}

fn positive(text: &str, currency: &saldo_core::Currency, field: &str) -> Result<String, AppError> {
    let money = Money::parse_decimal(text, currency)
        .map_err(|e| AppError::invalid(field, e.to_string()))?;
    if !money.is_positive() {
        return Err(AppError::invalid(field, "must be positive"));
    }
    Ok(money.to_decimal_string(currency))
}

/// The currency of the asset or liability side, as for a transaction
/// (ARCHITECTURE.md 4.2, rule 6).
fn home_currency(known: &Known, split: &TransactionTemplateSplitView) -> Option<String> {
    let find = |id: &Option<String>| id.as_ref().and_then(|i| known.accounts.get(i));
    let home = match find(&split.source_id) {
        Some(source) if source.account_type.is_asset_or_liability() => Some(source),
        _ => find(&split.destination_id),
    };
    home.map(|account| account.currency.to_string())
}

/// The input that would store `split` again (for an update that keeps the
/// splits but changes the kind).
pub(super) fn input_of(
    split: &TransactionTemplateSplitView,
) -> Result<TemplateSplitInput, AppError> {
    Ok(TemplateSplitInput {
        description: split.description.clone(),
        source_id: stored_id(&split.source_id)?,
        destination_id: stored_id(&split.destination_id)?,
        amount: split.amount.clone(),
        currency: split.currency.clone(),
        foreign_amount: split.foreign_amount.clone(),
        foreign_currency: split.foreign_currency.clone(),
        category_id: stored_id(&split.category_id)?,
        budget_id: stored_id(&split.budget_id)?,
        bill_id: stored_id(&split.bill_id)?,
        tag_ids: split
            .tag_ids
            .iter()
            .map(|t| parse_stored(t))
            .collect::<Result<_, _>>()?,
        notes: split.notes.clone(),
    })
}

/// A stored optional ID back as its type. `T: FromStr` accepts any ID type
/// that can be parsed from text (like a Go generic with a constraint).
fn stored_id<T: FromStr>(value: &Option<String>) -> Result<Option<T>, AppError> {
    value.as_deref().map(parse_stored).transpose()
}
