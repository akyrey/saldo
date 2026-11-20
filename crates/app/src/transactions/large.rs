//! `transaction.large` (Phase 10): a split at or above the household's
//! large-transaction amount, entered by a person or changed by a rule.

use std::collections::BTreeSet;

use serde::Serialize;
use sqlx::SqliteConnection;

use super::TransactionGroupView;
use crate::clock::Clock;
use crate::ctx::{HouseholdCtx, Origin};
use crate::currencies;
use crate::error::AppError;
use crate::events::{NewEvent, append_event};

/// The payload's `data`: the whole group, the large split, and the amount
/// the household set (in its primary currency).
#[derive(Debug, Serialize)]
struct LargePayload<'a> {
    group: &'a TransactionGroupView,
    split_id: &'a str,
    threshold: &'a str,
}

/// Only what people enter or rules change: imports and recurrences are
/// expected, and alerting on them would be noise.
fn alerts_for(origin: Origin) -> bool {
    matches!(origin, Origin::Api | Origin::Rule)
}

/// The group's splits at or above the threshold in the primary currency,
/// plus the threshold as text; empty when the household set none.
pub(super) async fn large_splits(
    conn: &mut SqliteConnection,
    household_id: &str,
    group_id: &str,
) -> Result<(BTreeSet<String>, Option<String>), AppError> {
    let household = sqlx::query!(
        "SELECT primary_currency, large_transaction_minor FROM households WHERE id = ?",
        household_id
    )
    .fetch_one(&mut *conn)
    .await?;
    let Some(threshold) = household.large_transaction_minor else {
        return Ok((BTreeSet::new(), None));
    };
    let ids = sqlx::query_scalar!(
        "SELECT id FROM splits WHERE household_id = ? AND group_id = ?
           AND currency_code = ? AND amount_minor >= ?",
        household_id,
        group_id,
        household.primary_currency,
        threshold
    )
    .fetch_all(&mut *conn)
    .await?;
    let currency = currencies::find(&mut *conn, &household.primary_currency, "currency").await?;
    let text = saldo_core::Money::new(threshold, currency.code()).to_decimal_string(&currency);
    Ok((ids.into_iter().collect(), Some(text)))
}

/// Appends `transaction.large` for each split that is large now and was not
/// `before` (empty for a new group), so editing a large split's notes does
/// not alert again.
pub(super) async fn emit(
    conn: &mut SqliteConnection,
    clock: &dyn Clock,
    ctx: &HouseholdCtx,
    origin: Origin,
    view: &TransactionGroupView,
    before: &BTreeSet<String>,
) -> Result<(), AppError> {
    if !alerts_for(origin) {
        return Ok(());
    }
    let household_id = ctx.household_id.to_string();
    let (now, threshold) = large_splits(&mut *conn, &household_id, &view.id).await?;
    let Some(threshold) = threshold else {
        return Ok(());
    };
    for split_id in now.difference(before) {
        let payload = LargePayload {
            group: view,
            split_id,
            threshold: &threshold,
        };
        let event = NewEvent::new(clock, "transaction", "large", view.id.clone(), &payload)?;
        append_event(&mut *conn, ctx, origin, event).await?;
    }
    Ok(())
}
