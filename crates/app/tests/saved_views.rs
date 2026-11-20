#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use common::*;
use saldo_app::ctx::Origin;
use saldo_app::error::AppError;
use saldo_app::saved_views::{NewSavedView, SavedViewUpdate};
use serde_json::json;

fn view(name: &str, filters: serde_json::Value) -> NewSavedView {
    NewSavedView {
        name: name.into(),
        page: "transactions".into(),
        filters,
        sort_order: 0,
    }
}

fn invalid_field(result: Result<impl std::fmt::Debug, AppError>) -> String {
    match result {
        Err(AppError::Invalid { field, .. }) => field,
        other => panic!("expected an invalid field, got {other:?}"),
    }
}

#[tokio::test]
async fn create_list_update_and_delete_with_events() {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let views = &test.app.saved_views;

    let big = views
        .create(
            &ctx,
            Origin::Api,
            NewSavedView {
                sort_order: 2,
                ..view(
                    "Big spends",
                    json!({ "amount_min": "100", "kind": "withdrawal" }),
                )
            },
        )
        .await
        .unwrap();
    assert_eq!(big.filters["amount_min"], "100");
    assert_eq!(
        last_event(&test, &ctx).await.event_type,
        "saved_view.created"
    );
    let inbox = views
        .create(
            &ctx,
            Origin::Api,
            view(" Inbox ", json!({ "category_id": "none" })),
        )
        .await
        .unwrap();
    assert_eq!(inbox.name, "Inbox");

    let names: Vec<_> = views
        .list(&ctx, "transactions")
        .await
        .unwrap()
        .into_iter()
        .map(|v| v.name)
        .collect();
    assert_eq!(names, ["Inbox", "Big spends"], "by sort order, then name");

    let id = big.id.parse().unwrap();
    let updated = views
        .update(
            &ctx,
            Origin::Api,
            id,
            SavedViewUpdate {
                name: Some("Large".into()),
                filters: Some(json!({ "amount_min": "250" })),
                sort_order: None,
            },
        )
        .await
        .unwrap();
    assert_eq!((updated.name.as_str(), updated.sort_order), ("Large", 2));
    assert_eq!(updated.filters, json!({ "amount_min": "250" }));
    assert_eq!(
        last_event(&test, &ctx).await.event_type,
        "saved_view.updated"
    );

    views.delete(&ctx, Origin::Api, id).await.unwrap();
    assert_eq!(
        last_event(&test, &ctx).await.event_type,
        "saved_view.deleted"
    );
    assert!(matches!(views.get(&ctx, id).await, Err(AppError::NotFound)));

    let other = other_household(&test).await;
    assert!(views.list(&other, "transactions").await.unwrap().is_empty());
    let inbox_id = inbox.id.parse().unwrap();
    assert!(matches!(
        views.get(&other, inbox_id).await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn names_are_unique_per_page() {
    let (test, setup) = with_household().await;
    let views = &test.app.saved_views;
    views
        .create(&setup.ctx, Origin::Api, view("Inbox", json!({})))
        .await
        .unwrap();
    let again = views
        .create(&setup.ctx, Origin::Api, view("Inbox", json!({})))
        .await;
    assert!(matches!(again, Err(AppError::Conflict(_))), "{again:?}");
}

#[tokio::test]
async fn filters_must_be_a_small_flat_object() {
    let (test, setup) = with_household().await;
    let create = |input: NewSavedView| {
        let app = test.app.clone();
        async move { app.saved_views.create(&setup.ctx, Origin::Api, input).await }
    };
    assert_eq!(
        invalid_field(create(view("A", json!([1]))).await),
        "filters"
    );
    assert_eq!(
        invalid_field(create(view("B", json!({ "nested": { "a": 1 } }))).await),
        "filters"
    );
    assert_eq!(
        invalid_field(create(view("C", json!({ "q": "x".repeat(2100) }))).await),
        "filters"
    );
    let many: serde_json::Map<_, _> = (0..31).map(|i| (format!("k{i}"), json!(i))).collect();
    assert_eq!(
        invalid_field(create(view("D", serde_json::Value::Object(many))).await),
        "filters"
    );
    assert_eq!(invalid_field(create(view(" ", json!({}))).await), "name");
    let wrong_page = NewSavedView {
        page: "accounts".into(),
        ..view("E", json!({}))
    };
    assert_eq!(invalid_field(create(wrong_page).await), "page");
    assert!(
        create(view("F", json!({ "reconciled": false, "page": 2 })))
            .await
            .is_ok()
    );
}
