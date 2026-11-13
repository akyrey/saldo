//! The auto-budget task: creates each household's missing automatic budget
//! limits up to the current period (household timezone), at startup and
//! hourly.

use std::time::Duration;

use saldo_app::App;

pub const NAME: &str = "auto-budgets";
pub const INTERVAL: Duration = Duration::from_secs(60 * 60);

/// One run over every household. A failing household is reported and the
/// others still run.
pub async fn run(app: App) -> Result<(), String> {
    let households = app
        .identity
        .household_contexts()
        .await
        .map_err(|e| e.to_string())?;
    let mut failures = Vec::new();
    for ctx in households {
        let result = match app.identity.household_today(&ctx).await {
            Ok(today) => app
                .budgets
                .create_auto_limits(&ctx, today)
                .await
                .map(|_| ()),
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            failures.push(format!("household {}: {error}", ctx.household_id));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{TimeZone, Utc};
    use saldo_app::budgets::{AutoBudgetInput, NewBudget};
    use saldo_app::clock::FixedClock;
    use saldo_app::ctx::Origin;
    use saldo_app::db;
    use saldo_app::identity::SetupInput;

    use super::*;

    #[tokio::test]
    async fn fills_missing_periods_of_every_household() {
        let pool = db::connect_in_memory().await.unwrap();
        db::migrate(&pool).await.unwrap();
        let clock = Arc::new(FixedClock::new(
            Utc.with_ymd_and_hms(2026, 11, 13, 9, 0, 0).unwrap(),
        ));
        let app = App::new(pool.clone(), clock.clone());
        let input = SetupInput {
            email: "owner@example.com".into(),
            password: "correct horse battery".into(),
            display_name: "Owner".into(),
            household_name: "Home".into(),
            primary_currency: "EUR".into(),
        };
        let ctx = app.identity.setup(Origin::Api, input).await.unwrap().ctx;
        let auto = AutoBudgetInput {
            auto_type: "reset".into(),
            period: "monthly".into(),
            amount: "10".into(),
            currency: None,
        };
        let budget = NewBudget {
            name: "Fun".into(),
            auto_budget: Some(auto),
            ..NewBudget::default()
        };
        app.budgets.create(&ctx, Origin::Api, budget).await.unwrap();
        clock.advance(chrono::TimeDelta::days(62)); // mid January 2027

        run(app.clone()).await.unwrap();
        run(app).await.unwrap();

        let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM budget_limits")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 3, "November, December and January, once each");
    }
}
