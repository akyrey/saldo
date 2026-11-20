//! Net worth goals (Phase 10). Today is 2026-11-13.

#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use chrono::NaiveDate;
use common::*;
use saldo_app::ctx::Origin;
use saldo_app::error::AppError;
use saldo_app::goals::{GoalUpdate, NewGoal};
use uuid::Uuid;

fn goal(name: &str, target: &str) -> NewGoal {
    NewGoal {
        name: name.into(),
        target_amount: target.into(),
        ..NewGoal::default()
    }
}

#[tokio::test]
async fn goals_start_from_todays_net_worth_and_alert_once_when_reached() {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let salary = account(&test, &ctx, "Salary", "revenue", None).await;
    let pay = |amount: &'static str| group(vec![split(1, &salary, &checking, amount)]);
    test.app
        .transactions
        .create(&ctx, Origin::Api, pay("1000"))
        .await
        .unwrap();

    let input = NewGoal {
        target_date: NaiveDate::from_ymd_opt(2027, 2, 28),
        ..goal("Emergency fund", "3000")
    };
    let created = test
        .app
        .goals
        .create(&ctx, Origin::Api, input)
        .await
        .unwrap();
    assert_eq!(created.start_amount, "1000.00");
    assert_eq!(created.currency, "EUR");
    let p = &created.progress;
    assert_eq!(
        (p.current.as_str(), p.remaining.as_str()),
        ("1000.00", "2000.00")
    );
    assert_eq!(p.progress_bp, 0);
    // November to February: 4 months.
    assert_eq!(p.required_monthly.as_deref(), Some("500.00"));
    assert_eq!(
        last_event(&test, &ctx).await.event_type,
        "net_worth_goal.created"
    );

    test.app
        .transactions
        .create(&ctx, Origin::Api, pay("1500"))
        .await
        .unwrap();
    let id: Uuid = created.id.parse().unwrap();
    let halfway = test.app.goals.get(&ctx, id).await.unwrap();
    assert_eq!(halfway.progress.progress_bp, 7500);
    test.app.alerts.run(&ctx).await.unwrap();
    let reached = |test: &TestApp| {
        let app = test.app.clone();
        async move {
            app.events
                .list(
                    &ctx,
                    &Default::default(),
                    saldo_app::page::PageRequest::max(),
                )
                .await
                .unwrap()
                .items
                .into_iter()
                .filter(|e| e.event_type == "net_worth_goal.reached")
                .count()
        }
    };
    assert_eq!(reached(&test).await, 0);

    test.app
        .transactions
        .create(&ctx, Origin::Api, pay("600"))
        .await
        .unwrap();
    test.app.alerts.run(&ctx).await.unwrap();
    test.app.alerts.run(&ctx).await.unwrap();
    assert_eq!(
        reached(&test).await,
        1,
        "once, however often the scheduler runs"
    );
    let done = test.app.goals.get(&ctx, id).await.unwrap();
    assert_eq!(done.reached_on.as_deref(), Some("2026-11-13"));
    assert!(done.progress.reached && done.progress.on_track);
    assert_eq!(done.progress.progress_bp, 10000);

    // A higher target is not reached yet: the goal can alert again later.
    let update = GoalUpdate {
        target_amount: Some("5000".into()),
        ..GoalUpdate::default()
    };
    let raised = test
        .app
        .goals
        .update(&ctx, Origin::Api, id, update)
        .await
        .unwrap();
    assert_eq!(raised.reached_on, None);
    assert!(!raised.progress.reached);

    let export = test.app.export.json(&ctx).await.unwrap();
    assert_eq!(export.net_worth_goals.len(), 1);
    test.app.goals.delete(&ctx, Origin::Api, id).await.unwrap();
    assert!(matches!(
        test.app.goals.get(&ctx, id).await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn goals_check_their_input() {
    let (test, setup) = with_household().await;
    let goals = &test.app.goals;
    let field = |result: Result<_, AppError>| match result {
        Err(AppError::Invalid { field, .. }) => field,
        other => panic!("expected an invalid field, got {other:?}"),
    };
    assert_eq!(
        field(goals.create(&setup.ctx, Origin::Api, goal(" ", "1")).await),
        "name"
    );
    assert_eq!(
        field(
            goals
                .create(&setup.ctx, Origin::Api, goal("A", "lots"))
                .await
        ),
        "target_amount"
    );
    let stranger = NewGoal {
        owner_user_id: Some(saldo_core::UserId(Uuid::from_u128(7))),
        ..goal("B", "1")
    };
    assert_eq!(
        field(goals.create(&setup.ctx, Origin::Api, stranger).await),
        "owner_user_id"
    );
    goals
        .create(&setup.ctx, Origin::Api, goal("Debt free", "0"))
        .await
        .unwrap();
    assert!(matches!(
        goals
            .create(&setup.ctx, Origin::Api, goal("Debt free", "0"))
            .await,
        Err(AppError::Conflict(_))
    ));
    let mine = NewGoal {
        owner_user_id: Some(setup.ctx.user_id),
        ..goal("Mine", "100")
    };
    let created = goals.create(&setup.ctx, Origin::Api, mine).await.unwrap();
    assert_eq!(
        created.owner_user_id.as_deref(),
        Some(setup.ctx.user_id.to_string().as_str())
    );
    assert_eq!(goals.list(&setup.ctx, Some(true)).await.unwrap().len(), 2);
}
