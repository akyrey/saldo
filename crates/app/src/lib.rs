//! Saldo application layer: services (use cases), repositories and migrations.
//!
//! Every state-changing service method opens a database transaction,
//! validates through `saldo-core`, writes the rows, appends one event, and
//! commits. Every method takes a [`ctx::HouseholdCtx`] first.

pub mod accounts;
pub mod alerts;
pub mod attachments;
pub mod bills;
pub mod budgets;
pub mod categories;
pub mod clock;
pub mod credit_cards;
pub mod ctx;
pub mod currencies;
pub mod db;
pub mod error;
pub mod events;
pub mod export;
pub mod external_refs;
pub mod goals;
pub mod identity;
pub mod import_runs;
pub mod notifications;
pub mod page;
pub mod piggy_banks;
pub mod reconciliations;
pub mod recurrences;
pub mod reports;
pub mod rules;
pub mod safe_to_spend;
pub mod saved_views;
pub mod scope;
mod search;
pub mod snapshot;
pub mod sources;
pub mod statements;
pub mod subscriptions;
pub mod tags;
pub mod templates;
pub mod transactions;
pub mod webhooks;

use std::path::PathBuf;

use sqlx::SqlitePool;

use crate::accounts::AccountService;
use crate::alerts::AlertService;
use crate::attachments::AttachmentService;
use crate::bills::BillService;
use crate::budgets::BudgetService;
use crate::categories::CategoryService;
use crate::clock::SharedClock;
use crate::credit_cards::CreditCardService;
use crate::currencies::CurrencyService;
use crate::events::EventService;
use crate::export::ExportService;
use crate::goals::GoalService;
use crate::identity::IdentityService;
use crate::notifications::NotificationService;
use crate::piggy_banks::PiggyBankService;
use crate::reconciliations::ReconciliationService;
use crate::recurrences::RecurrenceService;
use crate::reports::ReportService;
use crate::rules::RuleService;
use crate::safe_to_spend::SafeToSpendService;
use crate::saved_views::SavedViewService;
use crate::snapshot::SnapshotService;
use crate::statements::StatementService;
use crate::subscriptions::SubscriptionService;
use crate::tags::TagService;
use crate::templates::TemplateService;
use crate::transactions::TransactionService;
use crate::webhooks::WebhookService;

/// The entry point: one value holding every service. Built once at startup
/// and cloned cheaply (the pool and the clock are shared pointers).
/// New feature modules register their service here, in one place.
#[derive(Clone)]
pub struct App {
    pub pool: SqlitePool,
    pub clock: SharedClock,
    pub accounts: AccountService,
    pub attachments: AttachmentService,
    pub bills: BillService,
    pub categories: CategoryService,
    pub currencies: CurrencyService,
    pub events: EventService,
    pub export: ExportService,
    pub identity: IdentityService,
    pub notifications: NotificationService,
    pub rules: RuleService,
    pub tags: TagService,
    pub transactions: TransactionService,
    pub webhooks: WebhookService,
    pub budgets: BudgetService,
    pub reports: ReportService,
    pub piggy_banks: PiggyBankService,
    pub recurrences: RecurrenceService,
    pub templates: TemplateService,
    pub subscriptions: SubscriptionService,
    pub reconciliations: ReconciliationService,
    pub saved_views: SavedViewService,
    pub alerts: AlertService,
    pub goals: GoalService,
    pub safe_to_spend: SafeToSpendService,
    pub credit_cards: CreditCardService,
    pub snapshots: SnapshotService,
    pub statements: StatementService,
}

impl App {
    pub fn new(pool: SqlitePool, clock: SharedClock) -> Self {
        let accounts = AccountService::new(pool.clone(), clock.clone());
        let goals = GoalService::new(pool.clone(), clock.clone());
        let transactions = TransactionService::new(pool.clone(), clock.clone());
        let bills = BillService::new(pool.clone(), clock.clone());
        let attachments =
            AttachmentService::new(pool.clone(), clock.clone(), PathBuf::from(DEFAULT_DATA_DIR));
        Self {
            snapshots: SnapshotService::new(pool.clone(), PathBuf::from(DEFAULT_DATA_DIR)),
            export: ExportService::new(
                pool.clone(),
                clock.clone(),
                accounts.clone(),
                attachments.clone(),
            ),
            accounts,
            attachments,
            credit_cards: CreditCardService::new(pool.clone(), clock.clone(), transactions.clone()),
            reconciliations: ReconciliationService::new(
                pool.clone(),
                clock.clone(),
                transactions.clone(),
            ),
            statements: StatementService::new(pool.clone(), clock.clone(), transactions.clone()),
            subscriptions: SubscriptionService::new(
                pool.clone(),
                clock.clone(),
                bills.clone(),
                transactions.clone(),
            ),
            bills,
            categories: CategoryService::new(pool.clone(), clock.clone()),
            currencies: CurrencyService::new(pool.clone()),
            events: EventService::new(pool.clone()),
            identity: IdentityService::new(pool.clone(), clock.clone()),
            notifications: NotificationService::new(pool.clone(), clock.clone()),
            tags: TagService::new(pool.clone(), clock.clone()),
            saved_views: SavedViewService::new(pool.clone(), clock.clone()),
            goals: goals.clone(),
            safe_to_spend: SafeToSpendService::new(
                pool.clone(),
                clock.clone(),
                BudgetService::new(pool.clone(), clock.clone()),
                ReportService::new(pool.clone()),
            ),
            alerts: AlertService::new(
                pool.clone(),
                clock.clone(),
                BudgetService::new(pool.clone(), clock.clone()),
                ReportService::new(pool.clone()),
                goals,
            ),
            rules: RuleService::new(pool.clone(), clock.clone(), transactions.clone()),
            webhooks: WebhookService::new(pool.clone(), clock.clone()),
            budgets: BudgetService::new(pool.clone(), clock.clone()),
            reports: ReportService::new(pool.clone()),
            piggy_banks: PiggyBankService::new(pool.clone(), clock.clone()),
            recurrences: RecurrenceService::new(pool.clone(), clock.clone(), transactions.clone()),
            templates: TemplateService::new(pool.clone(), clock.clone(), transactions.clone()),
            transactions,
            pool,
            clock,
        }
    }
}

/// Where files live unless [`App::with_data_dir`] says otherwise.
pub const DEFAULT_DATA_DIR: &str = "/data";

impl App {
    /// Stores attachment files (and snapshot temporaries) under `data_dir`
    /// (`SALDO_DATA_DIR`).
    pub fn with_data_dir(self, data_dir: PathBuf) -> Self {
        let attachments =
            AttachmentService::new(self.pool.clone(), self.clock.clone(), data_dir.clone());
        let snapshots = SnapshotService::new(self.pool.clone(), data_dir);
        let export = ExportService::new(
            self.pool.clone(),
            self.clock.clone(),
            self.accounts.clone(),
            attachments.clone(),
        );
        Self {
            attachments,
            export,
            snapshots,
            ..self
        }
    }

    /// Checks the database answers (for `/healthz`).
    pub async fn ping(&self) -> Result<(), error::AppError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }
}

/// Returns the application name from `saldo-core`.
pub fn app_name() -> &'static str {
    saldo_core::APP_NAME
}
