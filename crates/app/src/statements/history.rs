//! Enriching transactions already in Saldo (imported from Firefly with bank
//! text) from stored Amazon and PayPal rows (Phase 13, Track E).

use chrono::NaiveDate;
use saldo_core::matching::split_by_item;
use saldo_core::{Currency, CurrencyCode, Money};
use serde::Serialize;
use serde_json::Value;

use super::StatementService;
use super::enrich::{Enrichment, Proposal};
use super::ledger_repo::{self, LedgerRow};
use super::match_repo;
use super::parse::StatementLine;
use super::review::statement_currency;
use super::source_lookup;
use crate::clock::format_timestamp;
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::{AppError, parse_stored};
use crate::transactions::{SplitInput, TransactionGroupView, TransactionInput, input_from_view};

/// The longest range searched at once: two years of an account's history.
const MAX_RANGE_DAYS: i64 = 731;
/// The most candidates accepted in one request (one write transaction).
const MAX_ACCEPT_ITEMS: usize = 200;

/// An existing transaction a stored source row explains.
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct EnrichmentCandidate {
    pub split_id: String,
    pub group_id: String,
    pub date: String,
    /// Signed for the account.
    pub amount: String,
    pub description: String,
    /// `{ source, description, counterparty, notes, items }`.
    #[schema(value_type = Object)]
    pub proposal: Value,
}

/// A candidate the owner accepts.
#[derive(Debug, Clone)]
pub struct EnrichmentAccept {
    pub split_id: String,
    /// Book a one-split Amazon purchase as one split per item.
    pub split_by_item: bool,
}

impl StatementService {
    /// The account's transactions in the range that no source row explains
    /// yet but a stored Amazon or PayPal row would. Computed on each request.
    pub async fn enrichment_candidates(
        &self,
        ctx: &HouseholdCtx,
        account_id: &str,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<EnrichmentCandidate>, AppError> {
        check_range(from, to)?;
        let mut conn = self.pool.acquire().await?;
        let household_id = ctx.household_id.to_string();
        let currency = statement_currency(&mut conn, &household_id, account_id).await?;
        let splits = ledger_repo::unexplained_splits(
            &mut conn,
            &household_id,
            account_id,
            &from.to_string(),
            &to.to_string(),
        )
        .await?;
        let lines = splits
            .iter()
            .map(|s| line_of(s, account_id))
            .collect::<Result<Vec<_>, _>>()?;
        let found = source_lookup::enrichments(&mut conn, &household_id, &lines, &currency).await?;
        let mut candidates = Vec::new();
        for ((split, line), enrichment) in splits.iter().zip(&lines).zip(found) {
            let Some(enrichment) = enrichment else {
                continue;
            };
            candidates.push(EnrichmentCandidate {
                split_id: split.id.clone(),
                group_id: split.group_id.clone(),
                date: split.date.clone(),
                amount: line.amount.to_decimal_string(&currency),
                description: split.description.clone(),
                proposal: serde_json::to_value(&enrichment.proposal)
                    .map_err(|e| AppError::Internal(e.to_string()))?,
            });
        }
        Ok(candidates)
    }

    /// Books the accepted candidates: the proposal's description, the old
    /// text and the source's notes in the notes, and with `split_by_item`
    /// one split per item. The proposal is computed again here, never taken
    /// from the client. One database transaction.
    pub async fn accept_enrichment(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        account_id: &str,
        items: Vec<EnrichmentAccept>,
    ) -> Result<Vec<TransactionGroupView>, AppError> {
        if items.len() > MAX_ACCEPT_ITEMS {
            return Err(AppError::invalid(
                "items",
                format!("at most {MAX_ACCEPT_ITEMS} at once"),
            ));
        }
        let mut tx = crate::db::begin(&self.pool).await?;
        let household_id = ctx.household_id.to_string();
        let currency = statement_currency(&mut tx, &household_id, account_id).await?;
        let now = format_timestamp(self.clock.now());
        let mut changed = Vec::with_capacity(items.len());
        for item in items {
            let split =
                ledger_repo::unexplained_split(&mut tx, &household_id, account_id, &item.split_id)
                    .await?
                    .ok_or_else(|| {
                        AppError::invalid(
                            "split_id",
                            format!(
                                "{} is not an unexplained transaction of this account",
                                item.split_id
                            ),
                        )
                    })?;
            let line = line_of(&split, account_id)?;
            let enrichment = source_lookup::enrichments(
                &mut tx,
                &household_id,
                std::slice::from_ref(&line),
                &currency,
            )
            .await?
            .into_iter()
            .next()
            .flatten()
            .ok_or_else(|| {
                AppError::Conflict(
                    "no stored Amazon or PayPal row explains this transaction".to_string(),
                )
            })?;
            let view = self
                .transactions
                .get_in(&mut tx, ctx, parse_stored(&split.group_id)?)
                .await?;
            let input = rewrite(
                &view,
                &split.id,
                &enrichment.proposal,
                item.split_by_item,
                &currency,
            )?;
            let saved = self
                .transactions
                .replace_in(&mut tx, ctx, origin, parse_stored(&split.group_id)?, input)
                .await?;
            record_matches(&mut tx, &household_id, &split.id, &enrichment, &now).await?;
            changed.push(saved);
        }
        tx.commit().await?;
        Ok(changed)
    }
}

fn check_range(from: NaiveDate, to: NaiveDate) -> Result<(), AppError> {
    if from > to {
        return Err(AppError::invalid("from", "must not be after to"));
    }
    if (to - from).num_days() > MAX_RANGE_DAYS {
        return Err(AppError::invalid(
            "to",
            format!("the range is at most {MAX_RANGE_DAYS} days"),
        ));
    }
    Ok(())
}

async fn record_matches(
    conn: &mut sqlx::SqliteConnection,
    household_id: &str,
    split_id: &str,
    enrichment: &Enrichment,
    now: &str,
) -> Result<(), AppError> {
    for found in &enrichment.matches {
        match_repo::insert_split_match(
            conn,
            household_id,
            &found.row_id,
            split_id,
            found.match_type,
            found.amount_diff_minor,
            now,
        )
        .await?;
    }
    Ok(())
}

/// A stored split as a statement line of the account.
fn line_of(split: &LedgerRow, account_id: &str) -> Result<StatementLine, AppError> {
    let date: NaiveDate = parse_stored(&split.date)?;
    let code =
        CurrencyCode::new(&split.currency_code).map_err(|e| AppError::Internal(e.to_string()))?;
    let minor = if split.source_account_id == account_id {
        -split.amount_minor
    } else {
        split.amount_minor
    };
    Ok(StatementLine {
        row: 0,
        accounting_date: date,
        value_date: date,
        booked_date: date,
        amount: Money::new(minor, code),
        description: split.description.clone(),
        category: None,
        counterparty: None,
        counterparty_iban: None,
        external_id: None,
        notes: vec![],
        parts: vec![],
    })
}

/// The group with the split rewritten from the proposal.
fn rewrite(
    view: &TransactionGroupView,
    split_id: &str,
    proposal: &Proposal,
    by_item: bool,
    currency: &Currency,
) -> Result<TransactionInput, AppError> {
    let mut input = input_from_view(view)?;
    let index = view
        .splits
        .iter()
        .position(|s| s.id == split_id)
        .ok_or_else(|| AppError::Internal("split not in its group".to_string()))?;
    let original = view.splits[index].clone();
    let mut notes = original.notes.clone().unwrap_or_default();
    let mut lines = vec![format!("Bank: {}", original.description)];
    lines.extend(proposal.notes.iter().cloned());
    let added = lines
        .iter()
        .map(|n| format!("- {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    notes = if notes.trim().is_empty() {
        added
    } else {
        format!("{notes}\n{added}")
    };
    let mut first = input.splits[index].clone();
    first.description = proposal.description.clone();
    first.notes = Some(notes);
    if !by_item {
        input.splits[index] = first;
        return Ok(input);
    }
    // A foreign amount cannot be shared out between the items faithfully.
    let foreign = original.foreign_amount.is_some();
    if view.splits.len() != 1 || original.reconciled || foreign || proposal.items.len() < 2 {
        return Err(AppError::invalid(
            "split_by_item",
            "only an unreconciled one-split purchase of several Amazon items, \
             without a foreign amount, can be split",
        ));
    }
    let charge = Money::parse_decimal(&original.amount, currency)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let amounts: Vec<Money> = proposal
        .items
        .iter()
        .map(|i| Money::new(i.amount_minor, currency.code()))
        .collect();
    let parts = split_by_item(charge, &amounts).ok_or_else(|| {
        AppError::invalid("split_by_item", "the items do not add up to this amount")
    })?;
    let splits: Vec<SplitInput> = parts
        .iter()
        .zip(&proposal.items)
        .enumerate()
        .map(|(n, (part, item))| SplitInput {
            // The first keeps the split's ID, and so its attachments.
            id: if n == 0 { first.id } else { None },
            description: item.product.clone(),
            amount: part.to_decimal_string(currency),
            notes: if n == 0 { first.notes.clone() } else { None },
            ..first.clone()
        })
        .collect();
    Ok(TransactionInput {
        title: Some(proposal.description.clone()),
        splits,
    })
}
