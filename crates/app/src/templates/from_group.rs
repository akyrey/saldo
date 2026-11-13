//! "Save as template": a stored transaction group as a new template.

use saldo_core::GroupId;
use sqlx::SqliteConnection;

use super::{
    NewTemplate, TEMPLATE_KINDS, TemplateService, TemplateSplitInput, TransactionTemplateView,
};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::{AppError, parse_stored};
use crate::transactions::{SplitView, TransactionGroupView};

impl TemplateService {
    /// Saves the group as a template named `name`, without its dates and
    /// reconciled flags. Opening balances and reconciliations cannot be
    /// templates.
    pub async fn from_transaction(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        group_id: GroupId,
        name: String,
    ) -> Result<TransactionTemplateView, AppError> {
        let mut tx = self.pool.begin().await?;
        let view = self
            .from_transaction_in(&mut tx, ctx, origin, group_id, name)
            .await?;
        tx.commit().await?;
        Ok(view)
    }

    /// Like [`Self::from_transaction`], inside the caller's transaction.
    pub async fn from_transaction_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        group_id: GroupId,
        name: String,
    ) -> Result<TransactionTemplateView, AppError> {
        let group = self.transactions.get_in(conn, ctx, group_id).await?;
        let input = template_of(group, name)?;
        self.create_in(conn, ctx, origin, input).await
    }
}

/// The template input that copies `group`.
fn template_of(group: TransactionGroupView, name: String) -> Result<NewTemplate, AppError> {
    let kind = group
        .splits
        .first()
        .map(|s| s.kind.clone())
        .unwrap_or_default();
    if !TEMPLATE_KINDS.contains(&kind.as_str()) {
        return Err(AppError::invalid(
            "kind",
            "only withdrawals, deposits and transfers can be templates",
        ));
    }
    let splits = group
        .splits
        .iter()
        .map(split_of)
        .collect::<Result<_, _>>()?;
    Ok(NewTemplate {
        name,
        kind,
        title: group.title,
        sort_order: None,
        splits,
    })
}

fn split_of(split: &SplitView) -> Result<TemplateSplitInput, AppError> {
    Ok(TemplateSplitInput {
        description: Some(split.description.clone()),
        source_id: Some(parse_stored(&split.source_account_id)?),
        destination_id: Some(parse_stored(&split.destination_account_id)?),
        amount: Some(split.amount.clone()),
        currency: Some(split.currency.clone()),
        foreign_amount: split.foreign_amount.clone(),
        foreign_currency: split.foreign_currency.clone(),
        category_id: split.category_id.as_deref().map(parse_stored).transpose()?,
        budget_id: split.budget_id.as_deref().map(parse_stored).transpose()?,
        bill_id: split.bill_id.as_deref().map(parse_stored).transpose()?,
        tag_ids: split
            .tag_ids
            .iter()
            .map(|t| parse_stored(t))
            .collect::<Result<_, _>>()?,
        notes: split.notes.clone(),
    })
}
