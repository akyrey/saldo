#![allow(clippy::unwrap_used)] // the seeding helper is test code that clippy does not see as tests

mod common;

use std::io::Read;

use common::*;
use saldo_app::attachments::NewAttachment;
use saldo_app::bills::NewBill;
use saldo_app::budgets::{NewBudget, NewBudgetLimit};
use saldo_app::categories::NewCategory;
use saldo_app::ctx::Origin;
use saldo_app::error::AppError;
use saldo_app::external_refs::{ExternalRefs, FIREFLY};
use saldo_app::piggy_banks::{NewPiggyBank, NewPiggyBankEvent};
use saldo_app::recurrences::{RecurrenceInput, RepetitionInput, TemplateSplitInput};
use saldo_app::rules::{ActionView, NewRuleGroup, RuleInput, TriggerView};
use saldo_app::tags::NewTag;

/// A household with a bit of everything.
async fn seeded() -> (TestApp, saldo_app::identity::SetupResult) {
    let (mut test, setup) = with_household().await;
    let dir = std::env::temp_dir().join(format!("saldo-export-test-{}", uuid::Uuid::new_v4()));
    test.app = test.app.with_data_dir(dir);
    add_member(&test, &setup.ctx, "partner@example.com").await;
    let checking = account(&test, &setup.ctx, "Checking, main", "asset", None).await;
    let food = account(&test, &setup.ctx, "Food", "expense", None).await;
    let rent = account(&test, &setup.ctx, "Rent", "expense", None).await;
    let category = test
        .app
        .categories
        .create(
            &setup.ctx,
            Origin::Api,
            NewCategory {
                name: "Home".into(),
                notes: None,
                color: None,
            },
        )
        .await
        .unwrap();
    let tag = test
        .app
        .tags
        .create(
            &setup.ctx,
            Origin::Api,
            NewTag {
                name: "monthly".into(),
                description: None,
            },
        )
        .await
        .unwrap();
    let bill = seed_bill_and_rule(&test, &setup.ctx).await;
    let salary = account(&test, &setup.ctx, "Salary", "revenue", None).await;
    test.app
        .transactions
        .create(
            &setup.ctx,
            Origin::Api,
            group(vec![split(1, &salary, &checking, "2000")]),
        )
        .await
        .unwrap();
    let budget = seed_savings_and_recurrence(&test, &setup.ctx, &checking, &rent).await;
    let mut first = split(1, &checking, &food, "42.50");
    first.budget_id = Some(budget.parse().unwrap());
    first.description = "Shop \"weekly\"".into();
    first.tag_ids = vec![tag.id.parse().unwrap()];
    let mut second = split(1, &checking, &rent, "800");
    second.category_id = Some(category.id.parse().unwrap());
    second.bill_id = Some(bill.parse().unwrap());
    let view = test
        .app
        .transactions
        .create(&setup.ctx, Origin::Api, group(vec![first, second]))
        .await
        .unwrap();
    test.app
        .templates
        .from_transaction(
            &setup.ctx,
            Origin::Api,
            view.id.parse().unwrap(),
            "Weekly shop".into(),
        )
        .await
        .unwrap();
    test.app
        .saved_views
        .create(
            &setup.ctx,
            Origin::Api,
            saldo_app::saved_views::NewSavedView {
                name: "Inbox".into(),
                page: "transactions".into(),
                filters: serde_json::json!({ "category_id": "none" }),
                sort_order: 0,
            },
        )
        .await
        .unwrap();
    test.app
        .transactions
        .create(
            &setup.ctx,
            Origin::Api,
            group(vec![split(2, &checking, &food, "3")]),
        )
        .await
        .unwrap();
    let file = NewAttachment {
        attachable_type: "split".into(),
        attachable_id: view.splits[0].id.clone(),
        filename: "r.txt".into(),
        title: None,
        notes: None,
        mime_type: "text/plain".into(),
        bytes: b"receipt".to_vec(),
    };
    test.app
        .attachments
        .upload(&setup.ctx, Origin::Api, file)
        .await
        .unwrap();
    let mut conn = test.app.pool.acquire().await.unwrap();
    let mut refs = ExternalRefs::load(&mut conn, &setup.ctx, FIREFLY)
        .await
        .unwrap();
    refs.upsert(&mut conn, "account", "1", &checking.id, "t")
        .await
        .unwrap();
    drop(conn);
    (test, setup)
}

/// A budget with a limit, a piggy bank with money in it, and a monthly
/// recurrence; returns the budget's ID.
async fn seed_savings_and_recurrence(
    test: &TestApp,
    ctx: &saldo_app::ctx::HouseholdCtx,
    checking: &saldo_app::accounts::AccountView,
    rent: &saldo_app::accounts::AccountView,
) -> String {
    let app = &test.app;
    let budget = app
        .budgets
        .create(
            ctx,
            Origin::Api,
            NewBudget {
                name: "Groceries".into(),
                ..NewBudget::default()
            },
        )
        .await
        .unwrap();
    let limit = NewBudgetLimit {
        start_date: date(1),
        end_date: date(30),
        amount: "300".into(),
        currency: None,
        notes: None,
    };
    let budget_id = budget.id.parse().unwrap();
    app.budgets
        .create_limit(ctx, Origin::Api, budget_id, limit)
        .await
        .unwrap();
    let piggy = app
        .piggy_banks
        .create(
            ctx,
            Origin::Api,
            NewPiggyBank {
                name: "Holiday".into(),
                currency: None,
                target_amount: Some("1000".into()),
                start_date: None,
                target_date: None,
                active: None,
                sort_order: None,
                notes: None,
                account_ids: vec![account_id(checking)],
            },
        )
        .await
        .unwrap();
    let money = NewPiggyBankEvent {
        account_id: None,
        amount: "25".into(),
        date: date(2),
        split_id: None,
    };
    app.piggy_banks
        .add_event(ctx, Origin::Api, piggy.id.parse().unwrap(), money)
        .await
        .unwrap();
    let template = TemplateSplitInput {
        description: "Monthly rent".into(),
        source_id: account_id(checking),
        destination_id: account_id(rent),
        amount: "800".into(),
        currency: None,
        foreign_amount: None,
        foreign_currency: None,
        category_id: None,
        budget_id: Some(budget_id),
        bill_id: None,
        piggy_bank_id: None,
        tag_ids: Vec::new(),
    };
    let recurrence = RecurrenceInput {
        title: "Rent".into(),
        description: None,
        first_date: date(1),
        repeat_until: None,
        max_occurrences: None,
        apply_rules: None,
        active: Some(false),
        notes: None,
        repetitions: vec![RepetitionInput {
            kind: "monthly".into(),
            moment: "1".into(),
            skip: 0,
            weekend_policy: None,
        }],
        splits: vec![template],
    };
    app.recurrences
        .create(ctx, Origin::Api, recurrence)
        .await
        .unwrap();
    budget.id
}

/// A monthly rent bill and one rule; returns the bill's ID.
async fn seed_bill_and_rule(test: &TestApp, ctx: &saldo_app::ctx::HouseholdCtx) -> String {
    let bill = test
        .app
        .bills
        .create(
            ctx,
            Origin::Api,
            NewBill {
                name: "Rent".into(),
                amount_min: "800".into(),
                amount_max: "800".into(),
                currency: None,
                first_due_date: date(1),
                end_date: None,
                repeat_freq: "monthly".into(),
                skip: None,
                remind_days_before: None,
                remind_every_days: None,
                active: None,
                sort_order: None,
                notes: None,
            },
        )
        .await
        .unwrap();
    let rule_group = test
        .app
        .rules
        .create_group(
            ctx,
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
        title: "Shop, weekly".into(),
        description: None,
        trigger_on: "create".into(),
        strict: true,
        stop_processing: false,
        active: true,
        sort_order: 0,
        triggers: vec![TriggerView {
            kind: "description_contains".into(),
            value: Some("Shop".into()),
            negate: false,
        }],
        actions: vec![ActionView {
            kind: "set_notes".into(),
            value: Some("weekly shop".into()),
            stop_processing: false,
            active: true,
        }],
    };
    test.app
        .rules
        .create_rule(ctx, Origin::Api, rule)
        .await
        .unwrap();
    bill.id
}

#[tokio::test]
async fn json_export_contains_every_row_of_the_household() {
    let (test, setup) = seeded().await;
    let other = other_household(&test).await;
    account(&test, &other, "Not mine", "asset", None).await;

    let doc = test.app.export.json(&setup.ctx).await.unwrap();

    assert_eq!((doc.format.as_str(), doc.version), ("saldo-export", 1));
    assert_eq!(doc.household.id, setup.household.id);
    let household = setup.household.id.as_str();
    let rows = |table: &str| {
        let sql = format!("SELECT count(*) FROM {table} WHERE household_id = '{household}'");
        let pool = test.app.pool.clone();
        async move {
            let (n,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(sql))
                .fetch_one(&pool)
                .await
                .unwrap();
            n as usize
        }
    };
    assert_eq!(doc.members.len(), rows("household_members").await);
    assert_eq!(doc.accounts.len(), rows("accounts").await);
    assert_eq!(doc.categories.len(), rows("categories").await);
    assert_eq!(doc.tags.len(), rows("tags").await);
    assert_eq!(doc.transactions.len(), rows("transaction_groups").await);
    let splits: usize = doc.transactions.iter().map(|g| g.splits.len()).sum();
    assert_eq!(splits, rows("splits").await);
    assert_eq!(doc.attachments.len(), rows("attachments").await);
    assert_eq!(doc.external_refs.len(), rows("external_refs").await);
    assert_eq!(doc.bills.len(), rows("bills").await);
    assert_eq!(doc.bill_periods.len(), rows("bill_periods").await);
    assert!(!doc.bill_periods.is_empty());
    assert_eq!(doc.rule_groups.len(), rows("rule_groups").await);
    assert_eq!(doc.rules.len(), rows("rules").await);
    assert_eq!(doc.rules[0].triggers.len(), 1);
    assert_eq!(doc.budgets.len(), rows("budgets").await);
    assert_eq!(doc.budget_limits.len(), rows("budget_limits").await);
    assert_eq!(doc.budget_limits[0].spent, "42.50");
    assert_eq!(doc.piggy_banks.len(), rows("piggy_banks").await);
    assert_eq!(doc.piggy_bank_events.len(), rows("piggy_bank_events").await);
    assert_eq!(doc.piggy_banks[0].saved_amount, "25.00");
    assert_eq!(doc.recurrences.len(), rows("recurrences").await);
    assert_eq!(doc.recurrences[0].splits.len(), 1);
    assert_eq!(doc.templates.len(), rows("transaction_templates").await);
    assert_eq!(doc.templates[0].splits.len(), 2);
    assert_eq!(doc.saved_views.len(), rows("saved_views").await);
    assert_eq!(doc.saved_views[0].filters["category_id"], "none");
    assert_eq!(
        doc.currencies.len(),
        count(&test, "currencies").await as usize
    );
    assert!(doc.accounts.iter().all(|a| a.name != "Not mine"));
    // Oldest first.
    assert_eq!(doc.transactions[0].splits[0].date, "2026-11-01");
}

#[tokio::test]
async fn csv_zip_has_one_file_per_entity_and_one_row_per_split() {
    let (test, setup) = seeded().await;
    let bytes = test.app.export.csv_zip(&setup.ctx).await.unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut names: Vec<String> = archive
        .file_names()
        .map(|n| n.unwrap().to_string())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "accounts.csv",
            "attachments.csv",
            "bill_periods.csv",
            "bills.csv",
            "budget_limits.csv",
            "budgets.csv",
            "categories.csv",
            "currencies.csv",
            "members.csv",
            "net_worth_goals.csv",
            "piggy_bank_events.csv",
            "piggy_banks.csv",
            "reconciliations.csv",
            "recurrences.csv",
            "rule_groups.csv",
            "rules.csv",
            "saved_views.csv",
            "subscription_dismissals.csv",
            "tags.csv",
            "templates.csv",
            "transactions.csv"
        ]
    );

    let mut transactions = String::new();
    archive
        .by_name("transactions.csv")
        .unwrap()
        .read_to_string(&mut transactions)
        .unwrap();
    let lines: Vec<&str> = transactions.lines().collect();
    assert_eq!(lines.len(), 1 + 4, "header plus one row per split");
    assert!(lines[0].starts_with("group_id,group_title,split_id,date,kind"));
    let shop = lines.iter().find(|l| l.contains("Shop")).unwrap();
    assert!(
        shop.contains("\"Shop \"\"weekly\"\"\""),
        "quotes are escaped: {shop}"
    );
    assert!(
        shop.contains("\"Checking, main\",Food,42.50,EUR"),
        "names and decimal amounts: {shop}"
    );
    assert!(
        shop.contains(",Groceries,,monthly,"),
        "budget and tag names: {shop}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains(",Rent,800.00,EUR,,,Home,,Rent,")),
        "category and bill names"
    );

    let mut rules = String::new();
    archive
        .by_name("rules.csv")
        .unwrap()
        .read_to_string(&mut rules)
        .unwrap();
    assert!(
        rules.contains("Default,\"Shop, weekly\",create,true,false,true,0,description_contains=Shop,set_notes=weekly shop"),
        "rules with readable triggers and actions: {rules}"
    );

    let mut recurrences = String::new();
    archive
        .by_name("recurrences.csv")
        .unwrap()
        .read_to_string(&mut recurrences)
        .unwrap();
    assert!(
        recurrences.contains("Rent,2026-11-01,,,monthly 1,false,true,Monthly rent,\"Checking, main\",Rent,800.00,EUR,,Groceries,,,"),
        "one row per template split, with names: {recurrences}"
    );
    let mut templates = String::new();
    archive
        .by_name("templates.csv")
        .unwrap()
        .read_to_string(&mut templates)
        .unwrap();
    assert!(
        templates.contains(
            ",Weekly shop,withdrawal,Test,0,\"Checking, main to Rent\",\"Checking, main\",Rent,800.00,EUR,,,Home,,Rent,,"
        ),
        "one row per template split, with names: {templates}"
    );
    let mut piggy = String::new();
    archive
        .by_name("piggy_bank_events.csv")
        .unwrap()
        .read_to_string(&mut piggy)
        .unwrap();
    assert!(
        piggy.contains("Holiday,\"Checking, main\",25.00,EUR,2026-11-02,"),
        "piggy bank events with names: {piggy}"
    );
}

#[tokio::test]
async fn only_owners_can_export() {
    let (test, setup) = seeded().await;
    let member = test
        .app
        .identity
        .login("partner@example.com", PASSWORD)
        .await
        .unwrap()
        .ctx;
    assert!(matches!(
        test.app.export.json(&member).await,
        Err(AppError::Forbidden)
    ));
    assert!(matches!(
        test.app.export.csv_zip(&member).await,
        Err(AppError::Forbidden)
    ));
    let _ = setup;
}
