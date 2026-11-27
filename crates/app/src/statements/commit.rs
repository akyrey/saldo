//! Committing and reverting statement imports (Phase 13, Track B; brief
//! decisions 8 and 13).

use std::collections::BTreeMap;

use saldo_core::Currency;
use serde_json::json;
use sqlx::SqliteConnection;

use super::StatementService;
use super::booking::{
    by_item, moves_of, part_accounts, proposal_of, split_input, statement_meta, still_unbooked,
};
use super::classify::money_out;
use super::import_repo::{self, ImportRow, LineRow};
use super::match_repo;
use super::review::draft;
use super::views::{ImportDetail, ImportView, detail_of, import_view};
use crate::accounts::NewAccount;
use crate::clock::format_timestamp;
use crate::ctx::{HouseholdCtx, Origin};
use crate::currencies;
use crate::error::{AppError, parse_stored};
use crate::events::{NewEvent, append_event};
use crate::external_refs::{BANK_STATEMENT, ExternalRefs};
use crate::transactions::TransactionInput;

/// Longest name given to a counterparty account created from a line.
const MAX_COUNTERPARTY_CHARS: usize = 100;

impl StatementService {
    /// Books a draft: `create` lines become transactions (rules run, since
    /// they carry the caller's origin), `link` lines only record the link,
    /// `skip` lines are remembered for the next file. One database
    /// transaction; appends `statement_import.committed` and
    /// `import.completed`.
    pub async fn commit(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
    ) -> Result<ImportDetail, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let household_id = ctx.household_id.to_string();
        let mut row = draft(&mut tx, &household_id, id).await?;
        let currency = currencies::find(&mut tx, &row.currency_code, "currency").await?;
        let part_accounts = part_accounts(&row)?;
        let mut refs = ExternalRefs::load(&mut tx, ctx, BANK_STATEMENT).await?;
        let now = format_timestamp(self.clock.now());
        let mut lines = import_repo::lines(&mut tx, id).await?;
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for line in &mut lines {
            *counts.entry(line.action.clone()).or_insert(0) += 1;
            if line.action != "skip" {
                still_unbooked(&refs, &row.account_id, line)?;
            }
            // The rows behind a proposal may have gone since the draft (a
            // deleted or newer export): then the bank's text is booked.
            if line.proposal.is_some()
                && !match_repo::line_has_matches(&mut tx, id, line.position).await?
            {
                line.proposal = None;
            }
            match line.action.as_str() {
                "create" => {
                    self.create_line(&mut tx, ctx, origin, &row, &currency, &part_accounts, line)
                        .await?;
                    let split = line.split_id.clone().ok_or_else(|| {
                        AppError::Internal("a created group without splits".to_string())
                    })?;
                    refs.upsert(&mut tx, "split", &line.ref_key, &split, &now)
                        .await?;
                }
                "link" => {
                    let split = line.split_id.clone().ok_or_else(|| {
                        AppError::invalid(
                            "lines",
                            format!("row {} links to nothing", line.row_number),
                        )
                    })?;
                    if !import_repo::split_on_account(
                        &mut tx,
                        &household_id,
                        &row.account_id,
                        &split,
                    )
                    .await?
                    {
                        return Err(AppError::Conflict(format!(
                            "row {} links to a split that is no longer on the account",
                            line.row_number
                        )));
                    }
                    refs.upsert(&mut tx, "split", &line.ref_key, &split, &now)
                        .await?;
                }
                _ => {}
            }
            if let (true, Some(split)) = (line.action != "skip", &line.split_id) {
                match_repo::attach_matches(&mut tx, id, line.position, split).await?;
            }
            import_repo::update_line(&mut tx, id, line).await?;
        }
        // Matches of lines that booked nothing (skipped) explain nothing.
        match_repo::drop_unattached_matches(&mut tx, id).await?;
        import_repo::set_status(&mut tx, id, "committed", Some(&now)).await?;
        row.status = "committed".to_string();
        row.committed_at = Some(now);
        let detail = detail_of(row, lines, &currency)?;
        self.emit_import(&mut tx, ctx, origin, &detail.import, "committed")
            .await?;
        let completed = json!({ "source": BANK_STATEMENT, "import_id": id, "lines": counts });
        let event = NewEvent::new(
            &*self.clock,
            "import",
            "completed",
            id.to_string(),
            &completed,
        )?;
        append_event(&mut tx, ctx, origin, event).await?;
        tx.commit().await?;
        Ok(detail)
    }

    /// Undoes a committed import: deletes the transactions it created and
    /// forgets its links. Refused (409) when one of those transactions is
    /// reconciled; undo that reconciliation first.
    pub async fn revert(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
    ) -> Result<ImportView, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let household_id = ctx.household_id.to_string();
        let mut row = import_repo::find(&mut tx, &household_id, id).await?;
        if row.status != "committed" {
            return Err(AppError::Conflict(format!(
                "the import is {}, not committed",
                row.status
            )));
        }
        let lines = import_repo::lines(&mut tx, id).await?;
        let groups: Vec<String> = lines
            .iter()
            .filter_map(|l| l.created_group_id.clone())
            .collect();
        if import_repo::reconciled_in_groups(&mut tx, &household_id, &groups).await? {
            return Err(AppError::Conflict(
                "a transaction this import created is reconciled; undo that reconciliation first"
                    .to_string(),
            ));
        }
        for group in import_repo::existing_groups(&mut tx, &household_id, &groups).await? {
            // Deleting a split also forgets its bank-statement link (decision 11).
            self.transactions
                .delete_in(&mut tx, ctx, origin, parse_stored(&group)?)
                .await?;
        }
        let mut refs = ExternalRefs::load(&mut tx, ctx, BANK_STATEMENT).await?;
        for line in lines.iter().filter(|l| l.action == "link") {
            refs.remove(&mut tx, "split", &line.ref_key).await?;
        }
        match_repo::drop_matches(&mut tx, id).await?;
        import_repo::set_status(&mut tx, id, "reverted", None).await?;
        row.status = "reverted".to_string();
        let currency = currencies::find(&mut tx, &row.currency_code, "currency").await?;
        let view = import_view(row, &currency)?;
        self.emit_import(&mut tx, ctx, origin, &view, "reverted")
            .await?;
        tx.commit().await?;
        Ok(view)
    }

    /// One `create` line as a transaction group: the line's own split on the
    /// statement account and one split per mapped part (decision 13).
    #[allow(clippy::too_many_arguments)] // all needed; a struct would only rename them
    async fn create_line(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        import: &ImportRow,
        currency: &Currency,
        part_accounts: &BTreeMap<String, Option<String>>,
        line: &mut LineRow,
    ) -> Result<(), AppError> {
        let (moves, mut notes) = moves_of(import, line, currency, part_accounts)?;
        let proposal = proposal_of(line)?;
        // The line as booked: the source's description and counterparty
        // when the owner keeps them (decision 4); the bank text stays in
        // the notes.
        let mut booked = line.clone();
        if let Some(proposal) = &proposal {
            booked.description = proposal.description.clone();
            booked.counterparty_name = Some(proposal.counterparty.clone());
            notes.extend(proposal.notes.iter().cloned());
        }
        let first_part = moves.first().map(|(_, minor)| *minor);
        let money_out = money_out(line.amount_minor, first_part);
        let counterparty = self
            .counterparty(conn, ctx, origin, &booked, currency, money_out)
            .await?;
        let moves = by_item(&booked, proposal.as_ref(), moves, currency);
        let notes = notes
            .iter()
            .map(|n| format!("- {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let meta = statement_meta(import, line);
        let splits = moves
            .iter()
            .enumerate()
            .map(|(index, (account, minor, description))| {
                let (source, destination) = if *minor < 0 {
                    (account.as_str(), counterparty.as_str())
                } else {
                    (counterparty.as_str(), account.as_str())
                };
                let mut split_line = booked.clone();
                split_line.description = description.clone();
                split_input(
                    &split_line,
                    currency,
                    source,
                    destination,
                    *minor,
                    (index == 0).then(|| notes.clone()),
                    meta.clone(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let title = (splits.len() > 1).then(|| booked.description.clone());
        let view = self
            .transactions
            .create_in(conn, ctx, origin, TransactionInput { title, splits })
            .await?;
        line.split_id = view.splits.first().map(|s| s.id.clone());
        line.created_group_id = Some(view.id);
        Ok(())
    }

    /// The line's counterparty account: the chosen one, or an expense
    /// (money out) or revenue account named after the counterparty, found
    /// by name or created.
    async fn counterparty(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        line: &LineRow,
        currency: &Currency,
        money_out: bool,
    ) -> Result<String, AppError> {
        if let Some(chosen) = &line.counterparty_account_id {
            return Ok(chosen.clone());
        }
        let name: String = line
            .counterparty_name
            .as_deref()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or(&line.description)
            .trim()
            .chars()
            .take(MAX_COUNTERPARTY_CHARS)
            .collect();
        let kind = if money_out { "expense" } else { "revenue" };
        let household_id = ctx.household_id.to_string();
        if let Some(existing) = import_repo::named_account(conn, &household_id, kind, &name).await?
        {
            return Ok(existing);
        }
        let input = NewAccount {
            name,
            account_type: kind.to_string(),
            currency: Some(currency.code().to_string()),
            ..NewAccount::default()
        };
        Ok(self.accounts.create_in(conn, ctx, origin, input).await?.id)
    }
}
