//! The credit card settlement sync (Phase 12): keeps each card month's
//! payment transfer equal to what the month spent until its payment day.

use std::time::Duration;

use saldo_app::App;
use tracing::{info, warn};

use super::Workers;
use crate::state::AppState;

pub const NAME: &str = "card-settlements";
pub const INTERVAL: Duration = Duration::from_secs(15 * 60);

/// Runs at startup, every 15 minutes (a payment day freezes its month) and
/// after writes (a purchase changes the transfer). Its own writes wake it
/// once more; that run changes nothing, so there is no loop.
pub fn spawn(workers: &mut Workers, state: &AppState) {
    let app = state.app.clone();
    let signal = state.events.clone();
    workers.spawn(NAME, INTERVAL, Some(&state.events), move || {
        let (app, signal) = (app.clone(), signal.clone());
        async move {
            if run(&app).await? > 0 {
                signal.notify();
            }
            Ok(())
        }
    });
}

/// One run over every household. A card that cannot be synced is logged
/// (without amounts or names) and tried again next run. Returns how many
/// changes were made.
pub async fn run(app: &App) -> Result<usize, String> {
    let households = app
        .identity
        .household_contexts()
        .await
        .map_err(|e| e.to_string())?;
    let mut changes = 0;
    for ctx in households {
        let report = app
            .credit_cards
            .sync_settlements(&ctx)
            .await
            .map_err(|e| e.to_string())?;
        for failure in &report.failures {
            warn!(
                household = %ctx.household_id,
                account = %failure.account_id,
                error = %failure.message,
                "credit card settlement could not be synced"
            );
        }
        changes += report.changes;
    }
    if changes > 0 {
        info!(count = changes, "credit card settlements changed");
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use saldo_app::accounts::NewAccount;
    use saldo_app::clock::SystemClock;
    use saldo_app::credit_cards::PlanInput;
    use saldo_app::ctx::{HouseholdCtx, Origin};
    use saldo_app::identity::SetupInput;
    use saldo_app::transactions::{SplitInput, TransactionInput};

    use super::*;

    async fn account(
        app: &App,
        ctx: &HouseholdCtx,
        name: &str,
        kind: &str,
        role: Option<&str>,
    ) -> saldo_core::AccountId {
        let input = NewAccount {
            name: name.to_string(),
            account_type: kind.to_string(),
            asset_role: role.map(str::to_string),
            ..NewAccount::default()
        };
        app.accounts
            .create(ctx, Origin::Api, input)
            .await
            .unwrap()
            .id
            .parse()
            .unwrap()
    }

    #[tokio::test]
    async fn books_the_payment_once_and_then_changes_nothing() {
        let pool = saldo_app::db::connect_in_memory().await.unwrap();
        saldo_app::db::migrate(&pool).await.unwrap();
        let app = App::new(pool, Arc::new(SystemClock));
        let setup = SetupInput {
            email: "owner@example.com".to_string(),
            password: "correct horse battery".to_string(),
            display_name: "Owner".to_string(),
            household_name: "Home".to_string(),
            primary_currency: "EUR".to_string(),
        };
        let ctx = app.identity.setup(Origin::Api, setup).await.unwrap().ctx;
        assert_eq!(run(&app).await, Ok(0));

        let card = account(&app, &ctx, "Visa", "asset", Some("credit_card")).await;
        let bank = account(&app, &ctx, "Checking", "asset", None).await;
        let shop = account(&app, &ctx, "Shop", "expense", None).await;
        let plan = PlanInput {
            monthly_limit: Some("500.00".to_string()),
            settlement_account_id: bank,
            settlement_day: 28,
        };
        app.credit_cards
            .put_plan(&ctx, Origin::Api, card, plan)
            .await
            .unwrap();
        let today = chrono::Utc::now().date_naive();
        let purchase = SplitInput {
            id: None,
            date: today,
            description: "Shopping".to_string(),
            source_id: card,
            destination_id: shop,
            amount: "10.00".to_string(),
            currency: None,
            foreign_amount: None,
            foreign_currency: None,
            category_id: None,
            budget_id: None,
            bill_id: None,
            tag_ids: Vec::new(),
            reconciled: false,
            notes: None,
            meta: serde_json::Map::new(),
        };
        let input = TransactionInput {
            title: None,
            splits: vec![purchase],
        };
        app.transactions
            .create(&ctx, Origin::Api, input)
            .await
            .unwrap();

        assert_eq!(run(&app).await, Ok(1));
        assert_eq!(run(&app).await, Ok(0));
    }
}
