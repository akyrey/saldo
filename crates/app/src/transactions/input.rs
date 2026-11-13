//! A stored group back as input, for re-saving it with changes (rules).

use serde_json::Value;

use super::{SplitInput, TransactionGroupView, TransactionInput};
use crate::error::{AppError, parse_stored};

/// The input that would store `view` again unchanged.
pub(crate) fn input_from_view(view: &TransactionGroupView) -> Result<TransactionInput, AppError> {
    let splits = view
        .splits
        .iter()
        .map(|s| {
            Ok(SplitInput {
                date: parse_stored(&s.date)?,
                description: s.description.clone(),
                source_id: parse_stored(&s.source_account_id)?,
                destination_id: parse_stored(&s.destination_account_id)?,
                amount: s.amount.clone(),
                currency: Some(s.currency.clone()),
                foreign_amount: s.foreign_amount.clone(),
                foreign_currency: s.foreign_currency.clone(),
                category_id: s.category_id.as_deref().map(parse_stored).transpose()?,
                budget_id: s.budget_id.as_deref().map(parse_stored).transpose()?,
                bill_id: s.bill_id.as_deref().map(parse_stored).transpose()?,
                tag_ids: s
                    .tag_ids
                    .iter()
                    .map(|t| parse_stored(t))
                    .collect::<Result<_, _>>()?,
                reconciled: s.reconciled,
                notes: s.notes.clone(),
                meta: match &s.meta {
                    Value::Object(map) => map.clone(),
                    _ => serde_json::Map::new(),
                },
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(TransactionInput {
        title: view.title.clone(),
        splits,
    })
}
