//! Runs the entity importers in order, in one database transaction.

use chrono::NaiveDate;
use saldo_app::clock::format_timestamp;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::events::{NewEvent, append_event};
use saldo_app::external_refs::{ExternalRefs, FIREFLY};
use saldo_app::{App, import_runs};
use sqlx::{Sqlite, Transaction};
use thiserror::Error;
use tokio::sync::Mutex;

use crate::balance;
use crate::entities;
use crate::firefly::{FireflyClient, FireflyError};
use crate::report::ImportReport;

/// Entities in import order (ARCHITECTURE.md 9.1); later phases add theirs.
pub const ENTITIES: [&str; 12] = [
    "currencies",
    "accounts",
    "categories",
    "tags",
    "budgets",
    "bills",
    "piggy_banks",
    "rules",
    "recurrences",
    "transactions",
    "attachments",
    // After transactions, so an event's transfer journal resolves to its split.
    "piggy_bank_events",
];

/// Everything that can stop an import. One enum, with `#[from]` so `?`
/// converts each source error, like Go's wrapped errors or a PHP exception
/// hierarchy collapsed into one type.
#[derive(Debug, Error)]
pub enum ImportError {
    #[error(transparent)]
    Firefly(#[from] FireflyError),
    #[error(transparent)]
    App(#[from] AppError),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("{0}")]
    Invalid(String),
}

/// The mutable part of a run: its transaction and the reference map.
pub struct State {
    /// `'static`: the transaction owns its pooled connection.
    pub tx: Transaction<'static, Sqlite>,
    pub refs: ExternalRefs,
}

/// What every entity importer receives:
/// `pub async fn import_<entity>(cx: &ImportCx, report: &mut ImportReport) -> Result<(), ImportError>`.
pub struct ImportCx {
    pub firefly: FireflyClient,
    pub app: App,
    pub ctx: HouseholdCtx,
    /// A `tokio` mutex can be held across `.await`, unlike `std`'s.
    pub state: Mutex<State>,
    pub dry_run: bool,
    /// Only transactions on or after this date.
    pub since: Option<NaiveDate>,
}

impl ImportCx {
    pub fn now(&self) -> String {
        format_timestamp(self.app.clock.now())
    }
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub dry_run: bool,
    /// Entity names to import; `None` means all.
    pub only: Option<Vec<String>>,
    pub since: Option<NaiveDate>,
}

/// Runs an import and records it in `import_runs` (also when it fails).
pub async fn run(
    app: App,
    firefly: FireflyClient,
    ctx: HouseholdCtx,
    options: Options,
) -> Result<ImportReport, ImportError> {
    let selected = selected_entities(options.only.as_deref())?;
    let run_id = import_runs::start(&app.pool, &*app.clock, &ctx, FIREFLY).await?;
    let pool = app.pool.clone();
    let clock = app.clock.clone();
    let result = run_entities(app, firefly, ctx, &options, &selected).await;
    let summary = match &result {
        Ok(report) => serde_json::to_value(report).unwrap_or_default(),
        Err(error) => serde_json::json!({ "error": error.to_string(), "dry_run": options.dry_run }),
    };
    import_runs::finish(&pool, &*clock, &run_id, result.is_ok(), &summary).await?;
    result
}

fn selected_entities(only: Option<&[String]>) -> Result<Vec<&'static str>, ImportError> {
    let Some(only) = only else {
        return Ok(ENTITIES.to_vec());
    };
    if let Some(unknown) = only.iter().find(|name| !ENTITIES.contains(&name.as_str())) {
        return Err(ImportError::Invalid(format!(
            "unknown entity {unknown:?}; choose from {}",
            ENTITIES.join(", ")
        )));
    }
    Ok(ENTITIES
        .iter()
        .copied()
        .filter(|e| only.iter().any(|o| o == e))
        .collect())
}

async fn run_entities(
    app: App,
    firefly: FireflyClient,
    ctx: HouseholdCtx,
    options: &Options,
    selected: &[&str],
) -> Result<ImportReport, ImportError> {
    let mut tx = app.pool.begin().await?;
    let refs = ExternalRefs::load(&mut tx, &ctx, FIREFLY).await?;
    let cx = ImportCx {
        firefly,
        app,
        ctx,
        state: Mutex::new(State { tx, refs }),
        dry_run: options.dry_run,
        since: options.since,
    };
    let mut report = ImportReport {
        dry_run: options.dry_run,
        ..ImportReport::default()
    };
    for entity in selected {
        match *entity {
            "currencies" => entities::currencies::import_currencies(&cx, &mut report).await?,
            "accounts" => entities::accounts::import_accounts(&cx, &mut report).await?,
            "categories" => entities::categories::import_categories(&cx, &mut report).await?,
            "tags" => entities::tags::import_tags(&cx, &mut report).await?,
            "budgets" => entities::budgets::import_budgets(&cx, &mut report).await?,
            "bills" => entities::bills::import_bills(&cx, &mut report).await?,
            "rules" => entities::rules::import_rules(&cx, &mut report).await?,
            "piggy_banks" => entities::piggy_banks::import_piggy_banks(&cx, &mut report).await?,
            "recurrences" => entities::recurrences::import_recurrences(&cx, &mut report).await?,
            "transactions" => entities::transactions::import_transactions(&cx, &mut report).await?,
            "attachments" => entities::attachments::import_attachments(&cx, &mut report).await?,
            "piggy_bank_events" => {
                entities::piggy_banks::import_piggy_bank_events(&cx, &mut report).await?
            }
            _ => {}
        }
    }
    balance::check(&cx, &mut report).await?;

    // `into_inner` takes the state back out of the mutex: the run is over.
    let mut state = cx.state.into_inner();
    if cx.dry_run {
        state.tx.rollback().await?;
    } else {
        let summary = serde_json::to_value(&report).unwrap_or_default();
        let event = NewEvent::new(
            &*cx.app.clock,
            "import",
            "completed",
            FIREFLY.to_string(),
            &summary,
        )?;
        append_event(&mut state.tx, &cx.ctx, Origin::Import, event).await?;
        state.tx.commit().await?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_selects_known_entities_in_import_order() {
        assert_eq!(selected_entities(None).unwrap(), ENTITIES.to_vec());
        let only = vec!["transactions".to_string(), "accounts".to_string()];
        assert_eq!(
            selected_entities(Some(&only)).unwrap(),
            ["accounts", "transactions"]
        );
        assert!(selected_entities(Some(&["piggy".to_string()])).is_err());
    }
}
