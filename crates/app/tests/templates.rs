#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use common::*;
use saldo_app::accounts::AccountView;
use saldo_app::budgets::NewBudget;
use saldo_app::categories::NewCategory;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::page::PageRequest;
use saldo_app::tags::NewTag;
use saldo_app::templates::{
    NewTemplate, TemplateId, TemplateSplitInput, TemplateUpdate, TransactionTemplateView,
};
use saldo_core::{BudgetId, CategoryId, GroupId, TagId};

struct Fixture {
    test: TestApp,
    ctx: HouseholdCtx,
    bank: AccountView,
    shop: AccountView,
    salary: AccountView,
}

async fn fixture() -> Fixture {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let bank = account(&test, &ctx, "Bank", "asset", None).await;
    let shop = account(&test, &ctx, "Shop", "expense", None).await;
    let salary = account(&test, &ctx, "Employer", "revenue", None).await;
    Fixture {
        test,
        ctx,
        bank,
        shop,
        salary,
    }
}

fn template(name: &str, kind: &str, splits: Vec<TemplateSplitInput>) -> NewTemplate {
    NewTemplate {
        name: name.to_string(),
        kind: kind.to_string(),
        title: None,
        sort_order: None,
        splits,
    }
}

fn groceries(f: &Fixture, amount: Option<&str>) -> TemplateSplitInput {
    TemplateSplitInput {
        description: Some("Groceries".into()),
        source_id: Some(account_id(&f.bank)),
        destination_id: Some(account_id(&f.shop)),
        amount: amount.map(str::to_string),
        ..TemplateSplitInput::default()
    }
}

async fn create(f: &Fixture, input: NewTemplate) -> Result<TransactionTemplateView, AppError> {
    f.test
        .app
        .templates
        .create(&f.ctx, Origin::Api, input)
        .await
}

fn invalid_field(result: Result<TransactionTemplateView, AppError>) -> String {
    match result {
        Err(AppError::Invalid { field, .. }) => field,
        other => panic!("expected a field error, got {other:?}"),
    }
}

fn id_of(view: &TransactionTemplateView) -> TemplateId {
    view.id.parse().unwrap()
}

#[tokio::test]
async fn a_template_keeps_what_it_was_given_and_appends_an_event() {
    let f = fixture().await;
    let input = NewTemplate {
        title: Some("  Weekly shop ".into()),
        ..template("Shop", "withdrawal", vec![groceries(&f, Some("12.5"))])
    };
    let view = create(&f, input).await.unwrap();

    assert_eq!(view.kind, "withdrawal");
    assert_eq!(view.title.as_deref(), Some("Weekly shop"));
    let split = &view.splits[0];
    assert_eq!(split.amount.as_deref(), Some("12.50"));
    assert_eq!(split.currency, None);
    assert_eq!(split.source_id.as_deref(), Some(f.bank.id.as_str()));
    let event = last_event(&f.test, &f.ctx).await;
    assert_eq!(event.event_type, "template.created");
    assert_eq!(event.payload["data"]["name"], "Shop");
}

#[tokio::test]
async fn every_part_of_a_template_may_be_empty() {
    let f = fixture().await;
    let view = create(
        &f,
        template("Empty", "deposit", vec![TemplateSplitInput::default()]),
    )
    .await
    .unwrap();
    assert_eq!(view.splits.len(), 1);
    assert_eq!(view.splits[0], Default::default());
    let none = create(&f, template("No splits", "transfer", vec![]))
        .await
        .unwrap();
    assert!(none.splits.is_empty());
}

#[tokio::test]
async fn names_are_unique_per_household() {
    let f = fixture().await;
    create(&f, template("Rent", "withdrawal", vec![]))
        .await
        .unwrap();
    let again = create(&f, template(" Rent ", "deposit", vec![])).await;
    assert!(matches!(again, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn the_name_and_kind_are_checked() {
    let f = fixture().await;
    assert_eq!(
        invalid_field(create(&f, template(" ", "withdrawal", vec![])).await),
        "name"
    );
    assert_eq!(
        invalid_field(create(&f, template("A", "opening_balance", vec![])).await),
        "kind"
    );
}

#[tokio::test]
async fn amounts_must_parse_in_their_currency_and_be_positive() {
    let f = fixture().await;
    let cases = [
        (groceries(&f, Some("1.234")), "splits[0].amount"),
        (groceries(&f, Some("-3")), "splits[0].amount"),
        (groceries(&f, Some("0")), "splits[0].amount"),
        (
            TemplateSplitInput {
                currency: Some("XXX".into()),
                ..groceries(&f, None)
            },
            "splits[0].currency",
        ),
        (
            TemplateSplitInput {
                foreign_amount: Some("3".into()),
                ..groceries(&f, None)
            },
            "splits[0].foreign_currency",
        ),
        (
            TemplateSplitInput {
                foreign_amount: Some("3.5".into()),
                foreign_currency: Some("JPY".into()),
                ..groceries(&f, None)
            },
            "splits[0].foreign_amount",
        ),
    ];
    for (split, field) in cases {
        let result = create(&f, template("T", "withdrawal", vec![split])).await;
        assert_eq!(invalid_field(result), field);
    }
}

#[tokio::test]
async fn an_amount_without_currency_or_accounts_uses_the_primary_currency() {
    let f = fixture().await;
    let split = TemplateSplitInput {
        amount: Some("7".into()),
        foreign_amount: Some("1000".into()),
        foreign_currency: Some("JPY".into()),
        ..TemplateSplitInput::default()
    };
    let view = create(&f, template("T", "withdrawal", vec![split]))
        .await
        .unwrap();
    assert_eq!(view.splits[0].amount.as_deref(), Some("7.00"));
    assert_eq!(view.splits[0].foreign_amount.as_deref(), Some("1000"));
}

#[tokio::test]
async fn references_must_belong_to_the_household() {
    let f = fixture().await;
    let other = other_household(&f.test).await;
    let foreign = account(&f.test, &other, "Theirs", "asset", None).await;
    let unknown = uuid::Uuid::from_u128(42);
    let cases = [
        (
            TemplateSplitInput {
                source_id: Some(account_id(&foreign)),
                ..Default::default()
            },
            "splits[0].source_id",
        ),
        (
            TemplateSplitInput {
                destination_id: Some(account_id(&foreign)),
                ..Default::default()
            },
            "splits[0].destination_id",
        ),
        (
            TemplateSplitInput {
                category_id: Some(CategoryId(unknown)),
                ..Default::default()
            },
            "splits[0].category_id",
        ),
        (
            TemplateSplitInput {
                budget_id: Some(BudgetId(unknown)),
                ..Default::default()
            },
            "splits[0].budget_id",
        ),
        (
            TemplateSplitInput {
                bill_id: Some(saldo_core::BillId(unknown)),
                ..Default::default()
            },
            "splits[0].bill_id",
        ),
        (
            TemplateSplitInput {
                tag_ids: vec![TagId(unknown)],
                ..Default::default()
            },
            "splits[0].tag_ids",
        ),
    ];
    for (split, field) in cases {
        let result = create(&f, template("T", "withdrawal", vec![split])).await;
        assert_eq!(invalid_field(result), field);
    }
}

#[tokio::test]
async fn budgets_fit_withdrawals_only_and_accounts_must_form_the_kind() {
    let f = fixture().await;
    let budget = f
        .test
        .app
        .budgets
        .create(
            &f.ctx,
            Origin::Api,
            NewBudget {
                name: "Food".into(),
                ..NewBudget::default()
            },
        )
        .await
        .unwrap();
    let with_budget = TemplateSplitInput {
        budget_id: Some(budget.id.parse().unwrap()),
        ..Default::default()
    };
    let deposit = create(&f, template("D", "deposit", vec![with_budget.clone()])).await;
    assert_eq!(invalid_field(deposit), "splits[0].budget_id");
    create(&f, template("W", "withdrawal", vec![with_budget]))
        .await
        .unwrap();

    let wrong = create(&f, template("X", "deposit", vec![groceries(&f, None)])).await;
    assert_eq!(invalid_field(wrong), "splits[0].destination_id");
    let income = TemplateSplitInput {
        source_id: Some(account_id(&f.salary)),
        destination_id: Some(account_id(&f.bank)),
        ..Default::default()
    };
    create(&f, template("Salary", "deposit", vec![income]))
        .await
        .unwrap();
}

#[tokio::test]
async fn deleted_references_are_dropped_when_a_template_is_read() {
    let f = fixture().await;
    let app = &f.test.app;
    let category = app
        .categories
        .create(
            &f.ctx,
            Origin::Api,
            NewCategory {
                name: "Food".into(),
                notes: None,
            },
        )
        .await
        .unwrap();
    let tag = app
        .tags
        .create(
            &f.ctx,
            Origin::Api,
            NewTag {
                name: "weekly".into(),
                description: None,
            },
        )
        .await
        .unwrap();
    let unused = account(&f.test, &f.ctx, "Old shop", "expense", None).await;
    let split = TemplateSplitInput {
        destination_id: Some(account_id(&unused)),
        category_id: Some(category.id.parse().unwrap()),
        tag_ids: vec![tag.id.parse().unwrap()],
        ..groceries(&f, Some("3"))
    };
    let view = create(&f, template("T", "withdrawal", vec![split]))
        .await
        .unwrap();

    app.categories
        .delete(&f.ctx, Origin::Api, category.id.parse().unwrap())
        .await
        .unwrap();
    app.tags
        .delete(&f.ctx, Origin::Api, tag.id.parse().unwrap())
        .await
        .unwrap();
    app.accounts
        .delete(&f.ctx, Origin::Api, account_id(&unused))
        .await
        .unwrap();

    let read = app.templates.get(&f.ctx, id_of(&view)).await.unwrap();
    let split = &read.splits[0];
    assert_eq!(split.category_id, None);
    assert!(split.tag_ids.is_empty());
    assert_eq!(split.destination_id, None);
    assert_eq!(split.source_id.as_deref(), Some(f.bank.id.as_str()));
    assert_eq!(split.amount.as_deref(), Some("3.00"));
    // The template still saves: what was dropped is simply not there.
    app.templates
        .update(&f.ctx, Origin::Api, id_of(&view), TemplateUpdate::default())
        .await
        .unwrap();
}

#[tokio::test]
async fn updates_rename_reorder_and_replace_splits() {
    let f = fixture().await;
    let app = &f.test.app;
    let input = NewTemplate {
        title: Some("Title".into()),
        ..template("Shop", "withdrawal", vec![groceries(&f, Some("5"))])
    };
    let view = create(&f, input).await.unwrap();

    let renamed = app
        .templates
        .update(
            &f.ctx,
            Origin::Api,
            id_of(&view),
            TemplateUpdate {
                name: Some("Market".into()),
                title: Some(None),
                sort_order: Some(9),
                ..TemplateUpdate::default()
            },
        )
        .await
        .unwrap();
    assert_eq!((renamed.name.as_str(), renamed.sort_order), ("Market", 9));
    assert_eq!(renamed.title, None);
    assert_eq!(renamed.splits, view.splits);
    assert_eq!(
        last_event(&f.test, &f.ctx).await.event_type,
        "template.updated"
    );

    let replaced = app
        .templates
        .update(
            &f.ctx,
            Origin::Api,
            id_of(&view),
            TemplateUpdate {
                splits: Some(vec![Default::default(), groceries(&f, None)]),
                ..TemplateUpdate::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(replaced.splits.len(), 2);

    // A new kind is checked against the kept splits.
    let kind = app
        .templates
        .update(
            &f.ctx,
            Origin::Api,
            id_of(&view),
            TemplateUpdate {
                kind: Some("deposit".into()),
                ..TemplateUpdate::default()
            },
        )
        .await;
    assert_eq!(invalid_field(kind), "splits[1].destination_id");
}

#[tokio::test]
async fn templates_list_in_order_and_new_ones_go_last() {
    let f = fixture().await;
    let b = create(&f, template("B", "withdrawal", vec![]))
        .await
        .unwrap();
    let a = create(&f, template("A", "withdrawal", vec![]))
        .await
        .unwrap();
    assert_eq!((b.sort_order, a.sort_order), (0, 1));
    let first = NewTemplate {
        sort_order: Some(-1),
        ..template("C", "deposit", vec![])
    };
    create(&f, first).await.unwrap();

    let page = PageRequest::new(None, None).unwrap();
    let list = f.test.app.templates.list(&f.ctx, page).await.unwrap();
    let names: Vec<_> = list.items.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["C", "B", "A"]);
    assert_eq!(list.total, 3);
}

#[tokio::test]
async fn deleting_appends_an_event_and_other_households_see_nothing() {
    let f = fixture().await;
    let app = &f.test.app;
    let view = create(&f, template("T", "withdrawal", vec![]))
        .await
        .unwrap();
    let other = other_household(&f.test).await;

    let get = app.templates.get(&other, id_of(&view)).await;
    assert!(matches!(get, Err(AppError::NotFound)));
    let delete = app
        .templates
        .delete(&other, Origin::Api, id_of(&view))
        .await;
    assert!(matches!(delete, Err(AppError::NotFound)));

    app.templates
        .delete(&f.ctx, Origin::Api, id_of(&view))
        .await
        .unwrap();
    assert_eq!(
        last_event(&f.test, &f.ctx).await.event_type,
        "template.deleted"
    );
    let gone = app.templates.get(&f.ctx, id_of(&view)).await;
    assert!(matches!(gone, Err(AppError::NotFound)));
}

#[tokio::test]
async fn a_transaction_becomes_a_template_without_its_date() {
    let f = fixture().await;
    let app = &f.test.app;
    let mut first = split(3, &f.bank, &f.shop, "20.00");
    first.reconciled = true;
    first.notes = Some("weekly".into());
    let group = app
        .transactions
        .create(
            &f.ctx,
            Origin::Api,
            group(vec![first, split(3, &f.bank, &f.shop, "4")]),
        )
        .await
        .unwrap();
    let group_id: GroupId = group.id.parse().unwrap();

    let view = app
        .templates
        .from_transaction(&f.ctx, Origin::Api, group_id, "Shop".into())
        .await
        .unwrap();
    assert_eq!(view.kind, "withdrawal");
    assert_eq!(view.title.as_deref(), Some("Test"));
    assert_eq!(view.splits.len(), 2);
    let split = &view.splits[0];
    assert_eq!(split.amount.as_deref(), Some("20.00"));
    assert_eq!(split.currency.as_deref(), Some("EUR"));
    assert_eq!(split.notes.as_deref(), Some("weekly"));
    assert_eq!(split.destination_id.as_deref(), Some(f.shop.id.as_str()));
    assert_eq!(
        last_event(&f.test, &f.ctx).await.event_type,
        "template.created"
    );

    let missing = app
        .templates
        .from_transaction(
            &f.ctx,
            Origin::Api,
            GroupId(uuid::Uuid::from_u128(7)),
            "X".into(),
        )
        .await;
    assert!(matches!(missing, Err(AppError::NotFound)));
}

#[tokio::test]
async fn opening_balances_cannot_become_templates() {
    let f = fixture().await;
    let app = &f.test.app;
    let household = f.ctx.household_id.to_string();
    let (equity,): (String,) = sqlx::query_as(
        "SELECT id FROM accounts WHERE household_id = ? AND equity_role = 'opening_balance'",
    )
    .bind(household)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    let mut opening = split(1, &f.bank, &f.bank, "100");
    opening.source_id = equity.parse().unwrap();
    let group = app
        .transactions
        .create(&f.ctx, Origin::Api, group(vec![opening]))
        .await
        .unwrap();
    let result = app
        .templates
        .from_transaction(&f.ctx, Origin::Api, group.id.parse().unwrap(), "O".into())
        .await;
    assert_eq!(invalid_field(result), "kind");
}
