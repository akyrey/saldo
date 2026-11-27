//! Committing and reverting statement imports (Phase 13, Track B; brief
//! decisions 8 and 13).

use std::collections::BTreeMap;

use saldo_core::{AccountId, Currency, Money};
use serde_json::{Map, Value, json};
use sqlx::SqliteConnection;

use super::StatementService;
use super::classify::money_out;
use super::import_repo::{self, ImportRow, LineRow};
use super::imports::draft;
use super::views::{ImportDetail, ImportView, detail_of, import_view};
use crate::accounts::NewAccount;
use crate::clock::format_timestamp;
use crate::ctx::{HouseholdCtx, Origin};
use crate::currencies;
use crate::error::{AppError, parse_stored};
use crate::events::{NewEvent, append_event};
use crate::external_refs::{BANK_STATEMENT, ExternalRefs};
use crate::transactions::{SplitInput, TransactionInput};

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
            import_repo::update_line(&mut tx, id, line).await?;
        }
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
        let (moves, notes) = moves_of(import, line, currency, part_accounts)?;
        let first_part = moves.first().map(|(_, minor)| *minor);
        let money_out = money_out(line.amount_minor, first_part);
        let counterparty = self
            .counterparty(conn, ctx, origin, line, currency, money_out)
            .await?;
        let notes = notes
            .iter()
            .map(|n| format!("- {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let meta = statement_meta(import, line);
        let splits = moves
            .iter()
            .enumerate()
            .map(|(index, (account, minor))| {
                let (source, destination) = if *minor < 0 {
                    (account.as_str(), counterparty.as_str())
                } else {
                    (counterparty.as_str(), account.as_str())
                };
                split_input(
                    line,
                    currency,
                    source,
                    destination,
                    *minor,
                    (index == 0).then(|| notes.clone()),
                    meta.clone(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let title = (splits.len() > 1).then(|| line.description.clone());
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

/// The part column → account map the draft was reviewed with, kept in its
/// summary at upload (decision 13).
fn part_accounts(row: &ImportRow) -> Result<BTreeMap<String, Option<String>>, AppError> {
    let summary: Value = serde_json::from_str(&row.summary)
        .map_err(|e| AppError::Internal(format!("stored summary: {e}")))?;
    match summary.get("part_accounts") {
        Some(value) if !value.is_null() => serde_json::from_value(value.clone())
            .map_err(|e| AppError::Internal(format!("stored summary: {e}"))),
        _ => Ok(BTreeMap::new()),
    }
}

/// Refuses a line another import booked since this draft was made (two
/// drafts of one file, or a duplicate forced to create or link), and a link
/// to a split already linked from this account's statements: either would
/// book a line twice or overwrite a link a later revert would then lose.
fn still_unbooked(refs: &ExternalRefs, account_id: &str, line: &LineRow) -> Result<(), AppError> {
    if refs.resolve("split", &line.ref_key).is_some() {
        return Err(AppError::Conflict(format!(
            "row {} is already imported; upload the file again to review it",
            line.row_number
        )));
    }
    if line.action == "link"
        && let Some(split) = &line.split_id
    {
        let prefix = format!("{account_id}:");
        let taken = refs
            .entries("split")
            .iter()
            .any(|(key, local)| local == split && key.starts_with(&prefix));
        if taken {
            return Err(AppError::Conflict(format!(
                "row {} links to a transaction already linked to another line of this account",
                line.row_number
            )));
        }
    }
    Ok(())
}

/// `(account ID, signed minor units)` for each account a line moves.
type Moves = Vec<(String, i64)>;

/// Who pays what for one line: `(account, signed minor)` for the line's
/// own amount and each mapped part, and the notes (the bank text, the
/// parser's notes, unmapped parts). One line is one movement, so all moves
/// must go the same way.
fn moves_of(
    import: &ImportRow,
    line: &LineRow,
    currency: &Currency,
    part_accounts: &BTreeMap<String, Option<String>>,
) -> Result<(Moves, Vec<String>), AppError> {
    let bad = |e: serde_json::Error| AppError::Internal(format!("stored line: {e}"));
    let parts: Vec<Value> = serde_json::from_str(&line.parts).map_err(bad)?;
    let mut notes = vec![format!("Bank: {}", line.description)];
    notes.extend(serde_json::from_str::<Vec<String>>(&line.notes).map_err(bad)?);
    let mut moves: Moves = Vec::new();
    if line.amount_minor != 0 {
        moves.push((import.account_id.clone(), line.amount_minor));
    }
    for part in &parts {
        let column = part["column"].as_str().unwrap_or_default();
        let minor = part["amount_minor"].as_i64().unwrap_or_default();
        let mapped = part_accounts
            .iter()
            .find(|(name, _)| name.trim().eq_ignore_ascii_case(column.trim()))
            .and_then(|(_, account)| account.clone());
        match mapped {
            Some(account) => moves.push((account, minor)),
            None => notes.push(format!(
                "{column}: {}",
                Money::new(minor, currency.code()).to_decimal_string(currency)
            )),
        }
    }
    let row = line.row_number;
    let Some((_, first)) = moves.first() else {
        return Err(AppError::invalid(
            "lines",
            format!("row {row} has nothing to book: map its part columns to accounts"),
        ));
    };
    if moves.iter().any(|(_, minor)| (*minor < 0) != (*first < 0)) {
        return Err(AppError::invalid(
            "lines",
            format!("row {row} mixes money in and out; skip it and enter it by hand"),
        ));
    }
    Ok((moves, notes))
}

/// `meta.statement` of a created split (decision 8): where it came from.
fn statement_meta(import: &ImportRow, line: &LineRow) -> Map<String, Value> {
    let mut meta = Map::new();
    meta.insert(
        "statement".to_string(),
        json!({
            "import_id": import.id,
            "position": line.position,
            "ref_key": line.ref_key,
            "accounting_date": line.accounting_date,
            "value_date": line.value_date,
        }),
    );
    meta
}

fn split_input(
    line: &LineRow,
    currency: &Currency,
    source: &str,
    destination: &str,
    minor: i64,
    notes: Option<String>,
    meta: Map<String, Value>,
) -> Result<SplitInput, AppError> {
    let source: AccountId = parse_stored(source)?;
    let destination: AccountId = parse_stored(destination)?;
    Ok(SplitInput {
        id: None,
        date: line
            .booked_date
            .parse()
            .map_err(|_| AppError::Internal(format!("stored date {}", line.booked_date)))?,
        description: line.description.clone(),
        source_id: source,
        destination_id: destination,
        amount: Money::new(minor.saturating_abs(), currency.code()).to_decimal_string(currency),
        currency: None,
        foreign_amount: None,
        foreign_currency: None,
        category_id: line.category_id.as_deref().map(parse_stored).transpose()?,
        budget_id: None,
        bill_id: None,
        tag_ids: Vec::new(),
        reconciled: false,
        notes,
        meta,
    })
}
