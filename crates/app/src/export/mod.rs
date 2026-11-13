//! Export: everything of a household as one JSON document or a zip of CSV
//! files (ARCHITECTURE.md 9.2). Owner only.

mod csv;
mod planning_csv;

use serde::Serialize;
use sqlx::{SqliteConnection, SqlitePool};

use crate::accounts::{AccountFilter, AccountService, AccountView};
use crate::attachments::{AttachmentService, AttachmentView};
use crate::bills::{self, BillPeriodView, BillView};
use crate::categories::{self, CategoryView};
use crate::clock::{SharedClock, format_timestamp};
use crate::ctx::HouseholdCtx;
use crate::currencies::{self, CurrencyView};
use crate::error::AppError;
use crate::identity::{HouseholdView, MemberView, household_view, members_in};
use crate::page::{MAX_PER_PAGE, PageRequest};
use crate::rules::{self, RuleGroupView, RuleView};
use crate::tags::{self, TagView};
use crate::transactions::{self, TransactionGroupView};

pub const FORMAT: &str = "saldo-export";
pub const VERSION: u32 = 1;

/// One import mapping (`external_refs`), so re-imports keep working after a restore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct ExternalRefView {
    pub source: String,
    pub entity_type: String,
    pub external_id: String,
    pub local_id: String,
}

/// The whole household in the API's own representations. Attachment files
/// are not embedded (only their metadata and SHA-256); sessions, the event
/// log and import bookkeeping are not exported.
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct ExportDocument {
    /// Always `saldo-export`.
    pub format: String,
    pub version: u32,
    pub exported_at: String,
    pub household: HouseholdView,
    pub members: Vec<MemberView>,
    pub currencies: Vec<CurrencyView>,
    pub accounts: Vec<AccountView>,
    pub categories: Vec<CategoryView>,
    pub tags: Vec<TagView>,
    pub transactions: Vec<TransactionGroupView>,
    pub attachments: Vec<AttachmentView>,
    pub external_refs: Vec<ExternalRefView>,
    pub bills: Vec<BillView>,
    pub bill_periods: Vec<BillPeriodView>,
    pub rule_groups: Vec<RuleGroupView>,
    pub rules: Vec<RuleView>,
}

#[derive(Clone)]
pub struct ExportService {
    pool: SqlitePool,
    clock: SharedClock,
    accounts: AccountService,
    attachments: AttachmentService,
}

impl ExportService {
    pub fn new(
        pool: SqlitePool,
        clock: SharedClock,
        accounts: AccountService,
        attachments: AttachmentService,
    ) -> Self {
        Self {
            pool,
            clock,
            accounts,
            attachments,
        }
    }

    /// The JSON document, read in one transaction so it is consistent.
    pub async fn json(&self, ctx: &HouseholdCtx) -> Result<ExportDocument, AppError> {
        ctx.require_owner()?;
        let mut tx = self.pool.begin().await?;
        let (rule_groups, rules) = rules::all_in(&mut tx, ctx).await?;
        let document = ExportDocument {
            format: FORMAT.to_string(),
            version: VERSION,
            exported_at: format_timestamp(self.clock.now()),
            household: household_view(&mut tx, ctx).await?,
            members: members_in(&mut tx, ctx).await?,
            currencies: currencies::all_in(&mut tx).await?,
            accounts: self.all_accounts(&mut tx, ctx).await?,
            categories: categories::all_in(&mut tx, ctx).await?,
            tags: tags::all_in(&mut tx, ctx).await?,
            transactions: transactions::all_in(&mut tx, ctx).await?,
            attachments: self.attachments.list_in(&mut tx, ctx).await?,
            external_refs: external_refs(&mut tx, ctx).await?,
            bills: bills::all_in(&mut tx, ctx).await?,
            bill_periods: bills::all_periods_in(&mut tx, ctx).await?,
            rule_groups,
            rules,
        };
        tx.rollback().await?;
        Ok(document)
    }

    /// A zip with one CSV per entity; `transactions.csv` has one row per split.
    pub async fn csv_zip(&self, ctx: &HouseholdCtx) -> Result<Vec<u8>, AppError> {
        let document = self.json(ctx).await?;
        csv::zip(&document)
    }

    async fn all_accounts(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
    ) -> Result<Vec<AccountView>, AppError> {
        let mut all = Vec::new();
        let mut number = 1;
        loop {
            let page = PageRequest::new(Some(number), Some(MAX_PER_PAGE))?;
            let result = self
                .accounts
                .list_in(&mut *conn, ctx, &AccountFilter::default(), page)
                .await?;
            let done = result.items.len() < MAX_PER_PAGE as usize;
            all.extend(result.items);
            if done {
                return Ok(all);
            }
            number += 1;
        }
    }
}

async fn external_refs(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
) -> Result<Vec<ExternalRefView>, AppError> {
    let household_id = ctx.household_id.to_string();
    let refs = sqlx::query_as!(
        ExternalRefView,
        "SELECT source, entity_type, external_id, local_id FROM external_refs
         WHERE household_id = ? ORDER BY source, entity_type, external_id",
        household_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(refs)
}
