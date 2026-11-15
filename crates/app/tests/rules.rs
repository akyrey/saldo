#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use chrono::NaiveDate;
use common::*;
use saldo_app::bills::NewBill;
use saldo_app::categories::NewCategory;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::rules::{ActionView, NewRuleGroup, RuleGroupUpdate, RuleInput, TriggerView};
use saldo_core::GroupId;

fn trigger(kind: &str, value: &str) -> TriggerView {
    TriggerView {
        kind: kind.into(),
        value: (!value.is_empty()).then(|| value.into()),
        negate: false,
    }
}

fn action(kind: &str, value: &str) -> ActionView {
    ActionView {
        kind: kind.into(),
        value: (!value.is_empty()).then(|| value.into()),
        stop_processing: false,
        active: true,
    }
}

async fn group_id(test: &TestApp, ctx: &HouseholdCtx) -> String {
    let input = NewRuleGroup {
        title: "Default".into(),
        active: true,
        ..Default::default()
    };
    test.app
        .rules
        .create_group(ctx, Origin::Api, input)
        .await
        .unwrap()
        .id
}

fn rule(
    group: &str,
    title: &str,
    triggers: Vec<TriggerView>,
    actions: Vec<ActionView>,
) -> RuleInput {
    RuleInput {
        rule_group_id: group.into(),
        title: title.into(),
        description: None,
        trigger_on: "create".into(),
        strict: true,
        stop_processing: false,
        active: true,
        sort_order: 0,
        triggers,
        actions,
    }
}

#[tokio::test]
async fn a_link_to_bill_rule_pays_the_period_when_a_transaction_is_created() {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let bill_input = NewBill {
        name: "Rent".into(),
        amount_min: "800".into(),
        amount_max: "800".into(),
        currency: None,
        first_due_date: NaiveDate::from_ymd_opt(2026, 10, 2).unwrap(),
        end_date: None,
        repeat_freq: "monthly".into(),
        skip: None,
        remind_days_before: None,
        remind_every_days: None,
        active: None,
        sort_order: None,
        notes: None,
    };
    let bill = test
        .app
        .bills
        .create(&ctx, Origin::Api, bill_input)
        .await
        .unwrap();
    let housing = test
        .app
        .categories
        .create(
            &ctx,
            Origin::Api,
            NewCategory {
                name: "Housing".into(),
                notes: None,
                color: None,
            },
        )
        .await
        .unwrap();
    let rules_group = group_id(&test, &ctx).await;
    let rent_rule = rule(
        &rules_group,
        "Rent",
        vec![
            trigger("description_contains", "rent"),
            trigger("amount_more", "700"),
        ],
        vec![
            action("link_to_bill", &bill.id),
            action("set_category", &housing.id),
        ],
    );
    test.app
        .rules
        .create_rule(&ctx, Origin::Api, rent_rule)
        .await
        .unwrap();
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let landlord = account(&test, &ctx, "Landlord", "expense", None).await;
    let mut payment = split(3, &checking, &landlord, "800");
    payment.description = "Rent November".into();

    let view = test
        .app
        .transactions
        .create(&ctx, Origin::Api, group(vec![payment]))
        .await
        .unwrap();

    assert_eq!(view.splits[0].bill_id.as_deref(), Some(bill.id.as_str()));
    assert_eq!(
        view.splits[0].category_id.as_deref(),
        Some(housing.id.as_str())
    );
    let periods = test
        .app
        .bills
        .periods(&ctx, bill.id.parse().unwrap())
        .await
        .unwrap();
    let november = periods.iter().find(|p| p.due_date == "2026-11-02").unwrap();
    assert_eq!(november.status, "paid");
    let tail: Vec<(String, String)> = events(&test, &ctx)
        .await
        .into_iter()
        .rev()
        .take(3)
        .map(|e| (e.event_type, e.origin))
        .collect();
    assert!(
        tail.contains(&("transaction.updated".to_string(), "rule".to_string())),
        "{tail:?}"
    );
    assert!(
        tail.contains(&("bill.paid".to_string(), "rule".to_string())),
        "{tail:?}"
    );
    // A non-matching transaction is left alone.
    let small = test
        .app
        .transactions
        .create(
            &ctx,
            Origin::Api,
            group(vec![split(4, &checking, &landlord, "20")]),
        )
        .await
        .unwrap();
    assert_eq!(small.splits[0].category_id, None);
}

#[tokio::test]
async fn rules_only_run_when_they_should() {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let food = test
        .app
        .categories
        .create(
            &ctx,
            Origin::Api,
            NewCategory {
                name: "Food".into(),
                notes: None,
                color: None,
            },
        )
        .await
        .unwrap();
    let rules_group = group_id(&test, &ctx).await;
    let on_update = RuleInput {
        trigger_on: "update".into(),
        ..rule(
            &rules_group,
            "Food",
            vec![trigger("description_contains", "shop")],
            vec![action("set_category", &food.id)],
        )
    };
    test.app
        .rules
        .create_rule(&ctx, Origin::Api, on_update)
        .await
        .unwrap();
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let shop = account(&test, &ctx, "Shop", "expense", None).await;
    let mut input = split(3, &checking, &shop, "5");
    input.description = "Shop".into();

    let created = test
        .app
        .transactions
        .create(&ctx, Origin::Api, group(vec![input.clone()]))
        .await
        .unwrap();
    assert_eq!(
        created.splits[0].category_id, None,
        "an update rule does not run on create"
    );
    let replaced = test
        .app
        .transactions
        .replace(
            &ctx,
            Origin::Api,
            created.id.parse().unwrap(),
            group(vec![input.clone()]),
        )
        .await
        .unwrap();
    assert_eq!(
        replaced.splits[0].category_id.as_deref(),
        Some(food.id.as_str())
    );
    let imported = test
        .app
        .transactions
        .create(&ctx, Origin::Import, group(vec![input.clone()]))
        .await
        .unwrap();
    let id: GroupId = imported.id.parse().unwrap();
    let again = test
        .app
        .transactions
        .replace(&ctx, Origin::Import, id, group(vec![input]))
        .await
        .unwrap();
    assert_eq!(again.splits[0].category_id, None, "imports never run rules");
}

#[tokio::test]
async fn a_broken_rule_never_blocks_a_transaction() {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let doomed = test
        .app
        .categories
        .create(
            &ctx,
            Origin::Api,
            NewCategory {
                name: "Doomed".into(),
                notes: None,
                color: None,
            },
        )
        .await
        .unwrap();
    let rules_group = group_id(&test, &ctx).await;
    test.app
        .rules
        .create_rule(
            &ctx,
            Origin::Api,
            rule(
                &rules_group,
                "All",
                vec![trigger("has_no_category", "")],
                vec![action("set_category", &doomed.id)],
            ),
        )
        .await
        .unwrap();
    test.app
        .categories
        .delete(&ctx, Origin::Api, doomed.id.parse().unwrap())
        .await
        .unwrap();
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let shop = account(&test, &ctx, "Shop", "expense", None).await;
    let view = test
        .app
        .transactions
        .create(
            &ctx,
            Origin::Api,
            group(vec![split(3, &checking, &shop, "5")]),
        )
        .await
        .unwrap();
    assert_eq!(view.splits[0].category_id, None);
}

#[tokio::test]
async fn active_rules_are_validated_and_inactive_ones_keep_unknown_keywords() {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let rules_group = group_id(&test, &ctx).await;
    let field_of = |input: RuleInput| {
        let app = test.app.clone();
        async move {
            match app.rules.create_rule(&ctx, Origin::Api, input).await {
                Err(AppError::Invalid { field, .. }) => field,
                other => panic!("expected invalid, got {other:?}"),
            }
        }
    };
    let category = "0190f5c3-7a2b-7c00-8000-000000000009";
    assert_eq!(
        field_of(rule(
            &rules_group,
            "x",
            vec![trigger("journal_id", "1")],
            vec![action("clear_category", "")]
        ))
        .await,
        "triggers[0]"
    );
    assert_eq!(
        field_of(rule(
            &rules_group,
            "x",
            vec![trigger("has_no_tag", "")],
            vec![action("set_category", category)]
        ))
        .await,
        "actions[0]"
    );
    assert_eq!(
        field_of(rule(
            &rules_group,
            "x",
            vec![],
            vec![action("clear_category", "")]
        ))
        .await,
        "triggers"
    );
    assert_eq!(
        field_of(RuleInput {
            trigger_on: "sometimes".into(),
            ..rule(&rules_group, "x", vec![], vec![])
        })
        .await,
        "trigger_on"
    );
    assert_eq!(
        field_of(rule("nope", "x", vec![], vec![])).await,
        "rule_group_id"
    );

    let inactive = RuleInput {
        active: false,
        ..rule(
            &rules_group,
            "Imported",
            vec![trigger("journal_id", "1")],
            vec![action("clear_category", "")],
        )
    };
    let stored = test
        .app
        .rules
        .create_rule(&ctx, Origin::Api, inactive.clone())
        .await
        .unwrap();
    assert_eq!(stored.triggers[0].kind, "journal_id");
    let activate = RuleInput {
        active: true,
        ..inactive
    };
    assert!(
        test.app
            .rules
            .replace_rule(&ctx, Origin::Api, &stored.id, activate)
            .await
            .is_err()
    );
    assert!(matches!(
        test.app.rules.test_rule(&ctx, &stored.id).await,
        Err(AppError::Invalid { .. })
    ));
}

#[tokio::test]
async fn groups_and_rules_crud_with_events_and_order() {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let first = group_id(&test, &ctx).await;
    let update = RuleGroupUpdate {
        title: Some("Renamed".into()),
        sort_order: Some(5),
        description: Some(Some("d".into())),
        active: Some(false),
    };
    let renamed = test
        .app
        .rules
        .update_group(&ctx, Origin::Api, &first, update)
        .await
        .unwrap();
    assert_eq!(
        (renamed.title.as_str(), renamed.sort_order, renamed.active),
        ("Renamed", 5, false)
    );
    let second = test
        .app
        .rules
        .create_group(
            &ctx,
            Origin::Api,
            NewRuleGroup {
                title: "Early".into(),
                sort_order: 1,
                active: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let titles: Vec<_> = test
        .app
        .rules
        .list_groups(&ctx)
        .await
        .unwrap()
        .into_iter()
        .map(|g| g.title)
        .collect();
    assert_eq!(titles, ["Early", "Renamed"]);
    let created = test
        .app
        .rules
        .create_rule(
            &ctx,
            Origin::Api,
            rule(
                &second.id,
                "R",
                vec![trigger("has_no_tag", "")],
                vec![action("set_notes", "x")],
            ),
        )
        .await
        .unwrap();
    assert_eq!(last_event(&test, &ctx).await.event_type, "rule.created");
    assert_eq!(
        test.app.rules.get_rule(&ctx, &created.id).await.unwrap(),
        created
    );
    assert_eq!(
        test.app
            .rules
            .list_rules(&ctx, Some(&second.id))
            .await
            .unwrap()
            .len(),
        1
    );
    test.app
        .rules
        .delete_group(&ctx, Origin::Api, &second.id)
        .await
        .unwrap();
    assert!(matches!(
        test.app.rules.get_rule(&ctx, &created.id).await,
        Err(AppError::NotFound)
    ));
    assert_eq!(
        last_event(&test, &ctx).await.event_type,
        "rule_group.deleted"
    );
    let other = other_household(&test).await;
    assert!(matches!(
        test.app.rules.get_group(&other, &first).await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn test_previews_and_apply_changes_existing_transactions() {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let shop = account(&test, &ctx, "Shop", "expense", None).await;
    for (day, description) in [(1, "Bakery"), (2, "Bakery again"), (3, "Fuel")] {
        let mut input = split(day, &checking, &shop, "5");
        input.description = description.into();
        test.app
            .transactions
            .create(&ctx, Origin::Api, group(vec![input]))
            .await
            .unwrap();
    }
    let food = test
        .app
        .categories
        .create(
            &ctx,
            Origin::Api,
            NewCategory {
                name: "Food".into(),
                notes: None,
                color: None,
            },
        )
        .await
        .unwrap();
    let rules_group = group_id(&test, &ctx).await;
    let manual = RuleInput {
        trigger_on: "manual".into(),
        ..rule(
            &rules_group,
            "Bakery",
            vec![trigger("description_starts", "bakery")],
            vec![action("set_category", &food.id)],
        )
    };
    let created = test
        .app
        .rules
        .create_rule(&ctx, Origin::Api, manual)
        .await
        .unwrap();

    let preview = test.app.rules.test_rule(&ctx, &created.id).await.unwrap();
    assert_eq!(preview.total, 2);
    assert_eq!(preview.matches[0].date, "2026-11-02", "newest first");
    let events_before = count(&test, "events").await;

    let result = test.app.rules.apply_rule(&ctx, &created.id).await.unwrap();
    assert_eq!((result.matched, result.changed), (2, 2));
    assert_eq!(count(&test, "events").await, events_before + 2);
    let again = test.app.rules.apply_rule(&ctx, &created.id).await.unwrap();
    assert_eq!(
        (again.matched, again.changed),
        (2, 0),
        "already applied: nothing changes"
    );
}
