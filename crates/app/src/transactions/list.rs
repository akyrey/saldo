//! Listing transaction groups with filters (`GET /transactions`).

use chrono::NaiveDate;
use saldo_core::{AccountId, BillId, BudgetId, CategoryId, Money, TagId};
use sqlx::SqliteConnection;

use super::{KINDS, TransactionGroupView, TransactionService, view};
use crate::ctx::HouseholdCtx;
use crate::error::AppError;
use crate::page::{Page, PageRequest};
use crate::scope::Scope;
use crate::{currencies, identity};

/// Filters for listing groups; a group matches if any split matches.
#[derive(Debug, Clone, Default)]
pub struct TransactionFilter {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub account_id: Option<AccountId>,
    pub category_id: Option<CategoryId>,
    /// Withdrawals and deposits without a category (transfers rarely have one).
    pub no_category: bool,
    pub tag_id: Option<TagId>,
    pub budget_id: Option<BudgetId>,
    /// Withdrawals without a budget (only withdrawals take one).
    pub no_budget: bool,
    pub bill_id: Option<BillId>,
    pub kind: Option<String>,
    /// Text searched in split descriptions and the group title.
    pub q: Option<String>,
    /// Only splits the bank confirmed (`true`) or not yet (`false`).
    pub reconciled: Option<bool>,
    /// Smallest split amount, a decimal string in `currency`.
    pub amount_min: Option<String>,
    /// Largest split amount, a decimal string in `currency`.
    pub amount_max: Option<String>,
    /// The currency of the amount range; the household's primary currency
    /// when empty. Only splits in this currency match a range.
    pub currency: Option<String>,
    /// Splits with (`true`) or without (`false`) an attachment.
    pub has_attachments: Option<bool>,
    pub scope: Scope,
}

/// An amount range in minor units of one currency.
#[derive(Debug, Default)]
struct AmountRange {
    currency: Option<String>,
    min: Option<i64>,
    max: Option<i64>,
}

/// Filters for [`list_ids`] in their stored form; `None` means "any".
#[derive(Debug, Default)]
struct ListFilter {
    from: Option<String>,
    to: Option<String>,
    account_id: Option<String>,
    category_id: Option<String>,
    no_category: bool,
    tag_id: Option<String>,
    budget_id: Option<String>,
    no_budget: bool,
    bill_id: Option<String>,
    kind: Option<String>,
    /// A `LIKE` pattern from `crate::search::contains_pattern`.
    q: Option<String>,
    owner_user_id: Option<String>,
    reconciled: Option<bool>,
    amounts: AmountRange,
    has_attachments: Option<bool>,
    limit: i64,
    offset: i64,
}

impl TransactionService {
    /// Groups with a matching split, newest first.
    pub async fn list(
        &self,
        ctx: &HouseholdCtx,
        filter: &TransactionFilter,
        page: PageRequest,
    ) -> Result<Page<TransactionGroupView>, AppError> {
        check_kind(filter.kind.as_deref())?;
        let mut conn = self.pool.acquire().await?;
        let amounts = amount_range(&mut conn, ctx, filter).await?;
        let repo_filter = ListFilter {
            from: filter.from.map(|d| d.to_string()),
            to: filter.to.map(|d| d.to_string()),
            account_id: filter.account_id.map(|id| id.to_string()),
            category_id: filter.category_id.map(|id| id.to_string()),
            no_category: filter.no_category,
            tag_id: filter.tag_id.map(|id| id.to_string()),
            budget_id: filter.budget_id.map(|id| id.to_string()),
            no_budget: filter.no_budget,
            bill_id: filter.bill_id.map(|id| id.to_string()),
            kind: filter.kind.clone(),
            q: crate::search::contains_pattern(filter.q.as_deref()),
            owner_user_id: filter.scope.owner(ctx),
            reconciled: filter.reconciled,
            amounts,
            has_attachments: filter.has_attachments,
            limit: page.limit(),
            offset: page.offset(),
        };
        let household_id = ctx.household_id.to_string();
        let (ids, total) = list_ids(&mut conn, &household_id, &repo_filter).await?;
        let items = view::load(&mut conn, &household_id, &ids).await?;
        Ok(Page::new(items, page, total))
    }
}

fn check_kind(kind: Option<&str>) -> Result<(), AppError> {
    match kind {
        Some(k) if !KINDS.contains(&k) => Err(AppError::invalid(
            "kind",
            format!("must be one of {}", KINDS.join(", ")),
        )),
        _ => Ok(()),
    }
}

/// Parses the amount range in its currency; an empty range needs no lookup.
async fn amount_range(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    filter: &TransactionFilter,
) -> Result<AmountRange, AppError> {
    let blank = |text: &Option<String>| text.as_deref().is_none_or(|t| t.trim().is_empty());
    if blank(&filter.amount_min) && blank(&filter.amount_max) {
        return Ok(AmountRange::default());
    }
    let code = match filter.currency.as_deref().map(str::trim) {
        Some(code) if !code.is_empty() => code.to_string(),
        _ => identity::primary_currency(&mut *conn, ctx).await?,
    };
    let currency = currencies::find(&mut *conn, &code, "currency").await?;
    let parse = |field: &str, text: &Option<String>| -> Result<Option<i64>, AppError> {
        let Some(text) = text.as_deref().map(str::trim).filter(|t| !t.is_empty()) else {
            return Ok(None);
        };
        let money = Money::parse_decimal(text, &currency)
            .map_err(|e| AppError::invalid(field, e.to_string()))?;
        if money.minor() < 0 {
            return Err(AppError::invalid(field, "must not be negative"));
        }
        Ok(Some(money.minor()))
    };
    let min = parse("amount_min", &filter.amount_min)?;
    let max = parse("amount_max", &filter.amount_max)?;
    if let (Some(low), Some(high)) = (min, max)
        && low > high
    {
        return Err(AppError::invalid(
            "amount_min",
            "must not be greater than amount_max",
        ));
    }
    Ok(AmountRange {
        currency: Some(code),
        min,
        max,
    })
}

/// IDs of the groups with at least one matching split, newest split date
/// first, plus the total number of matches. The two queries share their
/// `WHERE` clause word for word; keep them in step.
async fn list_ids(
    conn: &mut SqliteConnection,
    household_id: &str,
    f: &ListFilter,
) -> Result<(Vec<String>, i64), AppError> {
    let a = &f.amounts;
    // `?n IS NULL OR ...` makes every filter optional in one checked query.
    let ids = sqlx::query_scalar!(
        r#"SELECT g.id AS "id!" FROM transaction_groups g
           WHERE g.household_id = ?1 AND EXISTS (
             SELECT 1 FROM splits s WHERE s.group_id = g.id
               AND (?2 IS NULL OR s.date >= ?2) AND (?3 IS NULL OR s.date <= ?3)
               AND (?4 IS NULL OR s.source_account_id = ?4 OR s.destination_account_id = ?4)
               AND (?5 IS NULL OR s.category_id = ?5)
               AND (?6 IS NULL OR EXISTS (SELECT 1 FROM split_tags st WHERE st.split_id = s.id AND st.tag_id = ?6))
               AND (?7 IS NULL OR s.budget_id = ?7) AND (?8 IS NULL OR s.bill_id = ?8)
               AND (?9 IS NULL OR s.kind = ?9)
               AND (?10 IS NULL OR s.description LIKE ?10 ESCAPE '\' OR g.title LIKE ?10 ESCAPE '\')
               AND (?11 IS NULL OR EXISTS (SELECT 1 FROM accounts a WHERE a.household_id = g.household_id
                     AND a.id IN (s.source_account_id, s.destination_account_id) AND a.owner_user_id = ?11))
               AND (?12 IS NULL OR s.reconciled = ?12)
               AND (?13 = 0 OR (s.category_id IS NULL AND s.kind IN ('withdrawal', 'deposit')))
               AND (?14 = 0 OR (s.budget_id IS NULL AND s.kind = 'withdrawal'))
               AND (?15 IS NULL OR s.currency_code = ?15)
               AND (?16 IS NULL OR s.amount_minor >= ?16) AND (?17 IS NULL OR s.amount_minor <= ?17)
               AND (?18 IS NULL OR ?18 = EXISTS (SELECT 1 FROM attachments at WHERE at.household_id = s.household_id
                     AND at.attachable_type = 'split' AND at.attachable_id = s.id)))
           ORDER BY (SELECT max(s2.date) FROM splits s2 WHERE s2.group_id = g.id) DESC, g.id DESC
           LIMIT ?19 OFFSET ?20"#,
        household_id, f.from, f.to, f.account_id, f.category_id, f.tag_id, f.budget_id, f.bill_id,
        f.kind, f.q, f.owner_user_id, f.reconciled, f.no_category, f.no_budget, a.currency, a.min,
        a.max, f.has_attachments, f.limit, f.offset,
    )
    .fetch_all(&mut *conn)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) FROM transaction_groups g
           WHERE g.household_id = ?1 AND EXISTS (
             SELECT 1 FROM splits s WHERE s.group_id = g.id
               AND (?2 IS NULL OR s.date >= ?2) AND (?3 IS NULL OR s.date <= ?3)
               AND (?4 IS NULL OR s.source_account_id = ?4 OR s.destination_account_id = ?4)
               AND (?5 IS NULL OR s.category_id = ?5)
               AND (?6 IS NULL OR EXISTS (SELECT 1 FROM split_tags st WHERE st.split_id = s.id AND st.tag_id = ?6))
               AND (?7 IS NULL OR s.budget_id = ?7) AND (?8 IS NULL OR s.bill_id = ?8)
               AND (?9 IS NULL OR s.kind = ?9)
               AND (?10 IS NULL OR s.description LIKE ?10 ESCAPE '\' OR g.title LIKE ?10 ESCAPE '\')
               AND (?11 IS NULL OR EXISTS (SELECT 1 FROM accounts a WHERE a.household_id = g.household_id
                     AND a.id IN (s.source_account_id, s.destination_account_id) AND a.owner_user_id = ?11))
               AND (?12 IS NULL OR s.reconciled = ?12)
               AND (?13 = 0 OR (s.category_id IS NULL AND s.kind IN ('withdrawal', 'deposit')))
               AND (?14 = 0 OR (s.budget_id IS NULL AND s.kind = 'withdrawal'))
               AND (?15 IS NULL OR s.currency_code = ?15)
               AND (?16 IS NULL OR s.amount_minor >= ?16) AND (?17 IS NULL OR s.amount_minor <= ?17)
               AND (?18 IS NULL OR ?18 = EXISTS (SELECT 1 FROM attachments at WHERE at.household_id = s.household_id
                     AND at.attachable_type = 'split' AND at.attachable_id = s.id)))"#,
        household_id, f.from, f.to, f.account_id, f.category_id, f.tag_id, f.budget_id, f.bill_id,
        f.kind, f.q, f.owner_user_id, f.reconciled, f.no_category, f.no_budget, a.currency, a.min,
        a.max, f.has_attachments,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok((ids, total))
}
