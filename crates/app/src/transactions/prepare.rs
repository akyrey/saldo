//! Turns API input (decimal strings, optional currencies) into `saldo-core`
//! types, loads what validation needs, and validates the group.

use std::collections::BTreeSet;

use saldo_core::{
    Account, AccountLookup, Money, NewSplit, NewTransactionGroup, SplitMeta, ValidatedGroup,
    validate_group,
};
use sqlx::SqliteConnection;

use super::{SplitInput, TransactionInput};
use crate::accounts::{core_account, repo as account_repo};
use crate::ctx::{HouseholdCtx, Origin};
use crate::db::json_list;
use crate::error::{AppError, parse_stored};
use crate::{categories, currencies, identity, tags};

/// Validates `input` against the ledger rules for the caller's household.
pub(crate) async fn validate(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    origin: Origin,
    input: TransactionInput,
) -> Result<ValidatedGroup, AppError> {
    let household_id = ctx.household_id.to_string();
    let account_ids: Vec<String> = input
        .splits
        .iter()
        .flat_map(|s| [s.source_id.to_string(), s.destination_id.to_string()])
        .collect();
    let rows = account_repo::get_many(conn, &household_id, &json_list(&account_ids)?).await?;
    let accounts = rows
        .iter()
        .map(core_account)
        .collect::<Result<Vec<_>, _>>()?;
    // Rule 8 (both accounts active) protects new entries. An import copies
    // history, which may sit on accounts archived since, so it is exempt.
    let accounts: Vec<Account> = if origin == Origin::Import {
        accounts
            .into_iter()
            .map(|a| Account { active: true, ..a })
            .collect()
    } else {
        accounts
    };
    let primary = identity::primary_currency(conn, ctx).await?;

    let mut splits = Vec::with_capacity(input.splits.len());
    for (index, split) in input.splits.into_iter().enumerate() {
        splits.push(to_core_split(conn, index, split, &accounts, &primary).await?);
    }
    let lookup = build_lookup(conn, ctx, accounts, &splits).await?;
    let title = input
        .title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    validate_group(NewTransactionGroup { title, splits }, &lookup).map_err(AppError::Validation)
}

/// One split, with amounts parsed in their currencies. Errors name the field,
/// for example `splits[1].amount`.
async fn to_core_split(
    conn: &mut SqliteConnection,
    index: usize,
    input: SplitInput,
    accounts: &[Account],
    primary: &str,
) -> Result<NewSplit, AppError> {
    let field = |name: &str| format!("splits[{index}].{name}");
    let description = identity::required(&field("description"), &input.description)?;
    let code = input
        .currency
        .clone()
        .or_else(|| default_currency(&input, accounts))
        .unwrap_or_else(|| primary.to_string());
    let currency = currencies::find(conn, &code, &field("currency")).await?;
    let amount = Money::parse_decimal(&input.amount, &currency)
        .map_err(|e| AppError::invalid(field("amount"), e.to_string()))?;
    let foreign_amount = match (&input.foreign_amount, &input.foreign_currency) {
        (None, None) => None,
        (Some(text), Some(code)) => {
            let currency = currencies::find(conn, code, &field("foreign_currency")).await?;
            let money = Money::parse_decimal(text, &currency)
                .map_err(|e| AppError::invalid(field("foreign_amount"), e.to_string()))?;
            Some(money)
        }
        (Some(_), None) => {
            return Err(AppError::invalid(
                field("foreign_currency"),
                "is required with a foreign amount",
            ));
        }
        (None, Some(_)) => {
            return Err(AppError::invalid(
                field("foreign_amount"),
                "is required with a foreign currency",
            ));
        }
    };
    // A set drops duplicate tags, which the database would reject.
    let tags: BTreeSet<_> = input.tag_ids.into_iter().collect();
    Ok(NewSplit {
        date: input.date,
        description,
        source: input.source_id,
        destination: input.destination_id,
        amount,
        foreign_amount,
        category: input.category_id,
        budget: input.budget_id,
        bill: input.bill_id,
        tags: tags.into_iter().collect(),
        reconciled: input.reconciled,
        notes: input.notes,
        meta: SplitMeta(input.meta),
    })
}

/// The currency the ledger rules expect: the source's if it is an asset or
/// liability, otherwise the destination's (ARCHITECTURE.md 4.2, rule 6).
fn default_currency(input: &SplitInput, accounts: &[Account]) -> Option<String> {
    let find = |id| accounts.iter().find(|a| a.id == id);
    let home = match find(input.source_id) {
        Some(source) if source.account_type.is_asset_or_liability() => Some(source),
        _ => find(input.destination_id),
    };
    home.map(|account| account.currency.to_string())
}

/// The accounts, categories and tags of the household that the splits use.
/// Budgets and bills do not exist before Phase 5, so any reference to one
/// fails validation as unknown.
async fn build_lookup(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    accounts: Vec<Account>,
    splits: &[NewSplit],
) -> Result<AccountLookup, AppError> {
    let category_ids: Vec<String> = splits
        .iter()
        .filter_map(|s| s.category)
        .map(|c| c.to_string())
        .collect();
    let tag_ids: Vec<String> = splits
        .iter()
        .flat_map(|s| &s.tags)
        .map(|t| t.to_string())
        .collect();

    let mut lookup = AccountLookup::new(ctx.household_id);
    for account in accounts {
        lookup = lookup.with_account(account);
    }
    for id in categories::existing(conn, ctx, &json_list(&category_ids)?).await? {
        lookup = lookup.with_category(parse_stored(&id)?);
    }
    for id in tags::existing(conn, ctx, &json_list(&tag_ids)?).await? {
        lookup = lookup.with_tag(parse_stored(&id)?);
    }
    Ok(lookup)
}
