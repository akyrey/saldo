#![allow(clippy::unwrap_used)] // test helpers outside #[test] functions

mod common;

use chrono::{NaiveDate, TimeDelta};
use common::*;
use saldo_app::accounts::{AccountUpdate, AccountView};
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::page::PageRequest;
use saldo_app::piggy_banks::NewPiggyBank;
use saldo_app::recurrences::{
    RecurrenceInput, RecurrenceView, RepetitionInput, ScheduleInput, TemplateSplitInput,
};
use saldo_app::transactions::TransactionFilter;
use saldo_core::{BudgetId, PiggyBankId, RecurrenceId};
use uuid::Uuid;

struct Fixture {
    test: TestApp,
    ctx: HouseholdCtx,
    checking: AccountView,
    savings: AccountView,
    landlord: AccountView,
}

async fn fixture() -> Fixture {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let savings = account(&test, &ctx, "Savings", "asset", None).await;
    let landlord = account(&test, &ctx, "Landlord", "expense", None).await;
    Fixture {
        test,
        ctx,
        checking,
        savings,
        landlord,
    }
}

fn template(source: &AccountView, destination: &AccountView, amount: &str) -> TemplateSplitInput {
    TemplateSplitInput {
        description: "Rent".to_string(),
        source_id: account_id(source),
        destination_id: account_id(destination),
        amount: amount.to_string(),
        currency: None,
        foreign_amount: None,
        foreign_currency: None,
        category_id: None,
        budget_id: None,
        bill_id: None,
        piggy_bank_id: None,
        tag_ids: Vec::new(),
    }
}

fn repetition(kind: &str, moment: &str) -> RepetitionInput {
    RepetitionInput {
        kind: kind.to_string(),
        moment: moment.to_string(),
        skip: 0,
        weekend_policy: None,
    }
}

fn input(f: &Fixture, first: NaiveDate, repetitions: Vec<RepetitionInput>) -> RecurrenceInput {
    RecurrenceInput {
        title: "Monthly rent".to_string(),
        description: None,
        first_date: first,
        repeat_until: None,
        max_occurrences: None,
        apply_rules: None,
        active: None,
        notes: None,
        repetitions,
        splits: vec![template(&f.checking, &f.landlord, "800.00")],
    }
}

async fn create(f: &Fixture, input: RecurrenceInput) -> RecurrenceView {
    f.test
        .app
        .recurrences
        .create(&f.ctx, Origin::Api, input)
        .await
        .unwrap()
}

fn id(view: &RecurrenceView) -> RecurrenceId {
    view.id.parse().unwrap()
}

async fn group_count(f: &Fixture) -> i64 {
    f.test
        .app
        .transactions
        .list(
            &f.ctx,
            &TransactionFilter::default(),
            PageRequest::default(),
        )
        .await
        .unwrap()
        .total
}

#[tokio::test]
async fn create_stores_schedule_and_template() {
    let f = fixture().await;
    let mut monthly = repetition("monthly", "2");
    monthly.weekend_policy = Some("next_monday".to_string());
    let view = create(&f, input(&f, date(1), vec![monthly])).await;

    assert_eq!(view.title, "Monthly rent");
    assert_eq!(view.first_date, "2026-11-01");
    assert!(view.apply_rules && view.active);
    assert_eq!(view.fired_through, None);
    assert_eq!(view.last_fired_date, None);
    assert_eq!(view.repetitions[0].kind, "monthly");
    assert_eq!(view.repetitions[0].moment, "2");
    assert_eq!(view.repetitions[0].weekend_policy, "next_monday");
    assert_eq!(view.splits[0].amount, "800.00");
    assert_eq!(view.splits[0].currency, "EUR");
    assert_eq!(view.splits[0].source_account_id, f.checking.id);
    let event = last_event(&f.test, &f.ctx).await;
    assert_eq!(event.event_type, "recurrence.created");
    assert_eq!(event.payload["data"]["splits"][0]["amount"], "800.00");
}

fn field(error: AppError) -> String {
    match error {
        AppError::Invalid { field, .. } => field,
        AppError::Validation(errors) => errors[0].code().to_string(),
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn create_rejects_bad_schedules_and_templates() {
    let f = fixture().await;
    let service = &f.test.app.recurrences;
    let base = || input(&f, date(1), vec![repetition("monthly", "1")]);
    let mut cases = Vec::new();
    cases.push((
        RecurrenceInput {
            title: " ".into(),
            ..base()
        },
        "title",
    ));
    cases.push((
        RecurrenceInput {
            repetitions: vec![repetition("monthly", "32")],
            ..base()
        },
        "repetitions[0].moment",
    ));
    let mut bad_weekend = repetition("daily", "");
    bad_weekend.weekend_policy = Some("sometimes".into());
    cases.push((
        RecurrenceInput {
            repetitions: vec![bad_weekend],
            ..base()
        },
        "repetitions[0].weekend_policy",
    ));
    cases.push((
        RecurrenceInput {
            repetitions: vec![],
            ..base()
        },
        "repetitions",
    ));
    cases.push((
        RecurrenceInput {
            repeat_until: Some(date(1) - TimeDelta::days(1)),
            ..base()
        },
        "repeat_until",
    ));
    cases.push((
        RecurrenceInput {
            max_occurrences: Some(0),
            ..base()
        },
        "max_occurrences",
    ));
    cases.push((
        RecurrenceInput {
            splits: vec![],
            ..base()
        },
        "empty_group",
    ));
    cases.push((
        RecurrenceInput {
            splits: vec![template(&f.landlord, &f.checking, "1.00")],
            ..base()
        },
        "invalid_account_pair",
    ));
    let mut piggy = template(&f.checking, &f.savings, "1.00");
    piggy.piggy_bank_id = Some(PiggyBankId(Uuid::from_u128(9)));
    cases.push((
        RecurrenceInput {
            splits: vec![piggy],
            ..base()
        },
        "splits[0].piggy_bank_id",
    ));
    // Budgets are another track's work: until they exist, any budget is unknown.
    let mut budget = template(&f.checking, &f.landlord, "1.00");
    budget.budget_id = Some(BudgetId(Uuid::from_u128(9)));
    cases.push((
        RecurrenceInput {
            splits: vec![budget],
            ..base()
        },
        "unknown_reference",
    ));
    for (input, expected) in cases {
        let error = service
            .create(&f.ctx, Origin::Api, input)
            .await
            .unwrap_err();
        assert_eq!(field(error), expected);
    }
    assert_eq!(count(&f.test, "recurrences").await, 0);
}

#[tokio::test]
async fn firing_twice_on_one_date_creates_one_group() {
    let f = fixture().await;
    let view = create(&f, input(&f, date(1), vec![repetition("monthly", "1")])).await;
    let service = &f.test.app.recurrences;

    let first = service
        .fire(&f.ctx, Origin::Scheduler, id(&view), date(1))
        .await
        .unwrap();
    let second = service
        .fire(&f.ctx, Origin::Scheduler, id(&view), date(1))
        .await
        .unwrap();

    assert!(first.created);
    assert!(!second.created);
    assert_eq!(first.group.id, second.group.id);
    assert_eq!(first.group.title.as_deref(), Some("Monthly rent"));
    assert_eq!(first.group.splits[0].date, "2026-11-01");
    assert_eq!(first.group.splits[0].kind, "withdrawal");
    assert_eq!(group_count(&f).await, 1);
    let (recurrence_id, recurrence_date): (String, String) =
        sqlx::query_as("SELECT recurrence_id, recurrence_date FROM transaction_groups")
            .fetch_one(&f.test.app.pool)
            .await
            .unwrap();
    assert_eq!(
        (recurrence_id.as_str(), recurrence_date.as_str()),
        (view.id.as_str(), "2026-11-01")
    );

    let all = events(&f.test, &f.ctx).await;
    let fired: Vec<_> = all
        .iter()
        .filter(|e| e.event_type == "recurrence.fired")
        .collect();
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].origin, "scheduler");
    assert_eq!(
        fired[0].payload["data"]["group"]["id"],
        first.group.id.as_str()
    );
    let created = all
        .iter()
        .find(|e| e.event_type == "transaction.created")
        .unwrap();
    assert_eq!(created.origin, "scheduler");

    let view = service.get(&f.ctx, id(&view)).await.unwrap();
    assert_eq!(view.last_fired_date.as_deref(), Some("2026-11-01"));
}

#[tokio::test]
async fn fire_due_catches_up_to_today_once() {
    let f = fixture().await;
    // The test clock says 2026-11-13; the household's timezone is UTC.
    let view = create(&f, input(&f, date(10), vec![repetition("daily", "")])).await;
    let service = &f.test.app.recurrences;

    let report = service.fire_due(&f.ctx).await.unwrap();
    let dates: Vec<_> = report.fired.iter().map(|v| v.date.as_str()).collect();
    assert_eq!(
        dates,
        ["2026-11-10", "2026-11-11", "2026-11-12", "2026-11-13"]
    );
    assert!(report.failures.is_empty());
    assert!(service.fire_due(&f.ctx).await.unwrap().fired.is_empty());
    let view = service.get(&f.ctx, id(&view)).await.unwrap();
    assert_eq!(view.fired_through.as_deref(), Some("2026-11-13"));

    f.test.clock.advance(TimeDelta::days(1));
    let report = service.fire_due(&f.ctx).await.unwrap();
    assert_eq!(report.fired.len(), 1);
    assert_eq!(report.fired[0].date, "2026-11-14");
    assert_eq!(group_count(&f).await, 5);
}

#[tokio::test]
async fn deleting_a_fired_group_does_not_bring_it_back() {
    let f = fixture().await;
    create(&f, input(&f, date(13), vec![repetition("daily", "")])).await;
    let service = &f.test.app.recurrences;
    let report = service.fire_due(&f.ctx).await.unwrap();
    let group = report.fired[0].group.id.parse().unwrap();
    f.test
        .app
        .transactions
        .delete(&f.ctx, Origin::Api, group)
        .await
        .unwrap();
    assert!(service.fire_due(&f.ctx).await.unwrap().fired.is_empty());
    assert_eq!(group_count(&f).await, 0);
}

#[tokio::test]
async fn inactive_recurrences_do_not_fire_or_catch_up() {
    let f = fixture().await;
    let inactive = RecurrenceInput {
        active: Some(false),
        ..input(&f, date(10), vec![repetition("daily", "")])
    };
    let view = create(&f, inactive.clone()).await;
    let service = &f.test.app.recurrences;
    assert!(service.fire_due(&f.ctx).await.unwrap().fired.is_empty());

    f.test.clock.advance(TimeDelta::days(1));
    let active = RecurrenceInput {
        active: Some(true),
        ..inactive
    };
    service
        .replace(&f.ctx, Origin::Api, id(&view), active)
        .await
        .unwrap();
    let report = service.fire_due(&f.ctx).await.unwrap();
    let dates: Vec<_> = report.fired.iter().map(|v| v.date.as_str()).collect();
    assert_eq!(dates, ["2026-11-14"]);
}

#[tokio::test]
async fn a_failing_date_is_reported_and_retried() {
    let f = fixture().await;
    let view = create(&f, input(&f, date(12), vec![repetition("daily", "")])).await;
    let service = &f.test.app.recurrences;
    let archive = |active| AccountUpdate {
        active: Some(active),
        ..AccountUpdate::default()
    };
    let accounts = &f.test.app.accounts;
    accounts
        .update(&f.ctx, Origin::Api, account_id(&f.checking), archive(false))
        .await
        .unwrap();

    let report = service.fire_due(&f.ctx).await.unwrap();
    assert!(report.fired.is_empty());
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].date, date(12));
    assert_eq!(report.failures[0].recurrence_id, view.id);
    let view = service.get(&f.ctx, id(&view)).await.unwrap();
    assert_eq!(view.fired_through.as_deref(), Some("2026-11-11"));

    accounts
        .update(&f.ctx, Origin::Api, account_id(&f.checking), archive(true))
        .await
        .unwrap();
    let report = service.fire_due(&f.ctx).await.unwrap();
    assert_eq!(report.fired.len(), 2);
}

#[tokio::test]
async fn marking_fired_through_prevents_a_backfill() {
    let f = fixture().await;
    let view = create(&f, input(&f, date(1), vec![repetition("daily", "")])).await;
    let service = &f.test.app.recurrences;
    let mut tx = f.test.app.pool.begin().await.unwrap();
    service
        .mark_fired_through_in(&mut tx, &f.ctx, id(&view), date(12))
        .await
        .unwrap();
    // Never moves backwards.
    service
        .mark_fired_through_in(&mut tx, &f.ctx, id(&view), date(2))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let report = service.fire_due(&f.ctx).await.unwrap();
    let dates: Vec<_> = report.fired.iter().map(|v| v.date.as_str()).collect();
    assert_eq!(dates, ["2026-11-13"]);
}

#[tokio::test]
async fn transfers_into_a_piggy_bank_account_save_money() {
    let f = fixture().await;
    let salary = account(&f.test, &f.ctx, "Salary", "revenue", None).await;
    f.test
        .app
        .transactions
        .create(
            &f.ctx,
            Origin::Api,
            group(vec![split(1, &salary, &f.checking, "1000.00")]),
        )
        .await
        .unwrap();
    let piggy = f
        .test
        .app
        .piggy_banks
        .create(
            &f.ctx,
            Origin::Api,
            NewPiggyBank {
                name: "Holiday".into(),
                target_amount: Some("150.00".into()),
                account_ids: vec![account_id(&f.savings)],
                ..NewPiggyBank::default()
            },
        )
        .await
        .unwrap();
    let mut transfer = template(&f.checking, &f.savings, "100.00");
    transfer.piggy_bank_id = Some(piggy.id.parse().unwrap());
    let recurrence = RecurrenceInput {
        splits: vec![transfer],
        ..input(&f, date(1), vec![repetition("monthly", "1")])
    };
    let view = create(&f, recurrence).await;
    let service = &f.test.app.recurrences;
    service
        .fire(&f.ctx, Origin::Scheduler, id(&view), date(1))
        .await
        .unwrap();
    service
        .fire(&f.ctx, Origin::Scheduler, id(&view), date(2))
        .await
        .unwrap();

    let piggy = f
        .test
        .app
        .piggy_banks
        .get(&f.ctx, piggy.id.parse().unwrap())
        .await
        .unwrap();
    // 100 + the 50 left until the target.
    assert_eq!(piggy.saved_amount, "150.00");
    let reached = events(&f.test, &f.ctx)
        .await
        .into_iter()
        .filter(|e| e.event_type == "piggy_bank.target_reached")
        .count();
    assert_eq!(reached, 1);
}

#[tokio::test]
async fn replace_delete_preview_and_isolation() {
    let f = fixture().await;
    let view = create(&f, input(&f, date(1), vec![repetition("monthly", "1")])).await;
    let service = &f.test.app.recurrences;
    service
        .fire(&f.ctx, Origin::Api, id(&view), date(1))
        .await
        .unwrap();

    let replaced = RecurrenceInput {
        title: "Rent".into(),
        repetitions: vec![repetition("weekly", "5"), repetition("nth_weekday", "2,3")],
        ..input(&f, date(1), vec![])
    };
    let updated = service
        .replace(&f.ctx, Origin::Api, id(&view), replaced)
        .await
        .unwrap();
    assert_eq!(updated.title, "Rent");
    assert_eq!(updated.repetitions.len(), 2);
    assert_eq!(updated.last_fired_date.as_deref(), Some("2026-11-01"));
    assert_eq!(
        last_event(&f.test, &f.ctx).await.event_type,
        "recurrence.updated"
    );

    let schedule = ScheduleInput {
        first_date: date(1),
        repeat_until: None,
        max_occurrences: None,
        repetitions: vec![repetition("weekly", "1")],
    };
    let dates = service.preview(&f.ctx, &schedule, 3).await.unwrap();
    let dates: Vec<_> = dates.iter().map(ToString::to_string).collect();
    assert_eq!(dates, ["2026-11-16", "2026-11-23", "2026-11-30"]);
    let bad = ScheduleInput {
        repetitions: vec![],
        ..schedule
    };
    assert!(service.preview(&f.ctx, &bad, 3).await.is_err());

    let other = other_household(&f.test).await;
    assert!(matches!(
        service.get(&other, id(&view)).await,
        Err(AppError::NotFound)
    ));
    assert!(matches!(
        service.fire(&other, Origin::Api, id(&view), date(2)).await,
        Err(AppError::NotFound)
    ));
    assert_eq!(
        service
            .list(&other, None, PageRequest::default())
            .await
            .unwrap()
            .total,
        0
    );
    assert_eq!(
        service
            .list(&f.ctx, Some(true), PageRequest::default())
            .await
            .unwrap()
            .total,
        1
    );

    service
        .delete(&f.ctx, Origin::Api, id(&view))
        .await
        .unwrap();
    assert_eq!(
        last_event(&f.test, &f.ctx).await.event_type,
        "recurrence.deleted"
    );
    assert_eq!(count(&f.test, "recurrence_splits").await, 0);
    assert_eq!(group_count(&f).await, 1, "fired transactions stay");
    assert!(matches!(
        service.get(&f.ctx, id(&view)).await,
        Err(AppError::NotFound)
    ));
}

/// A rule that writes a note on every new split described "Rent".
async fn rent_note_rule(f: &Fixture) {
    use saldo_app::rules::{ActionView, NewRuleGroup, RuleInput, TriggerView};
    let rules = &f.test.app.rules;
    let rule_group = rules
        .create_group(
            &f.ctx,
            Origin::Api,
            NewRuleGroup {
                title: "Default".into(),
                active: true,
                ..NewRuleGroup::default()
            },
        )
        .await
        .unwrap();
    let rule = RuleInput {
        rule_group_id: rule_group.id,
        title: "Note rent".into(),
        description: None,
        trigger_on: "create".into(),
        strict: true,
        stop_processing: false,
        active: true,
        sort_order: 0,
        triggers: vec![TriggerView {
            kind: "description_is".into(),
            value: Some("Rent".into()),
            negate: false,
        }],
        actions: vec![ActionView {
            kind: "set_notes".into(),
            value: Some("from a rule".into()),
            stop_processing: false,
            active: true,
        }],
    };
    rules.create_rule(&f.ctx, Origin::Api, rule).await.unwrap();
}

#[tokio::test]
async fn fired_groups_run_the_rules_only_when_the_recurrence_asks() {
    let f = fixture().await;
    rent_note_rule(&f).await;
    let with_rules = create(&f, input(&f, date(1), vec![repetition("monthly", "1")])).await;
    let mut quiet = input(&f, date(1), vec![repetition("monthly", "1")]);
    quiet.title = "Quiet rent".into();
    quiet.apply_rules = Some(false);
    let without_rules = create(&f, quiet).await;
    let service = &f.test.app.recurrences;

    let ruled = service
        .fire(&f.ctx, Origin::Scheduler, id(&with_rules), date(1))
        .await
        .unwrap();
    let plain = service
        .fire(&f.ctx, Origin::Scheduler, id(&without_rules), date(1))
        .await
        .unwrap();

    assert_eq!(ruled.group.splits[0].notes.as_deref(), Some("from a rule"));
    assert_eq!(plain.group.splits[0].notes, None);
    // The rule's rewrite keeps the link to the recurrence.
    let again = service
        .fire(&f.ctx, Origin::Scheduler, id(&with_rules), date(1))
        .await
        .unwrap();
    assert!(!again.created);
    assert_eq!(again.group.id, ruled.group.id);
}
