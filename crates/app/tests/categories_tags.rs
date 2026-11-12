mod common;

use common::*;
use saldo_app::categories::{CategoryUpdate, NewCategory};
use saldo_app::ctx::Origin;
use saldo_app::error::AppError;
use saldo_app::page::PageRequest;
use saldo_app::tags::{NewTag, TagUpdate};

#[tokio::test]
async fn category_lifecycle_with_events() {
    let (test, setup) = with_household().await;
    let created = test
        .app
        .categories
        .create(
            &setup.ctx,
            Origin::Api,
            NewCategory {
                name: " Food ".into(),
                notes: Some("n".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        (created.name.as_str(), created.notes.as_deref()),
        ("Food", Some("n"))
    );
    assert_eq!(
        last_event(&test, &setup.ctx).await.event_type,
        "category.created"
    );
    let id = created.id.parse().unwrap();

    let updated = test
        .app
        .categories
        .update(
            &setup.ctx,
            Origin::Api,
            id,
            CategoryUpdate {
                name: Some("Groceries".into()),
                notes: Some(None),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        (updated.name.as_str(), updated.notes.clone()),
        ("Groceries", None)
    );
    let event = last_event(&test, &setup.ctx).await;
    assert_eq!(
        (event.event_type.as_str(), &event.payload["data"]["name"]),
        ("category.updated", &serde_json::json!("Groceries"))
    );
    assert_eq!(
        test.app.categories.get(&setup.ctx, id).await.unwrap(),
        updated
    );

    test.app
        .categories
        .delete(&setup.ctx, Origin::Api, id)
        .await
        .unwrap();
    assert_eq!(
        last_event(&test, &setup.ctx).await.event_type,
        "category.deleted"
    );
    assert!(matches!(
        test.app.categories.get(&setup.ctx, id).await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn category_names_are_unique_and_required() {
    let (test, setup) = with_household().await;
    let new = |name: &str| NewCategory {
        name: name.into(),
        notes: None,
    };
    let first = test
        .app
        .categories
        .create(&setup.ctx, Origin::Api, new("Food"))
        .await
        .unwrap();
    let other = test
        .app
        .categories
        .create(&setup.ctx, Origin::Api, new("Fuel"))
        .await
        .unwrap();
    assert!(matches!(
        test.app
            .categories
            .create(&setup.ctx, Origin::Api, new("Food"))
            .await,
        Err(AppError::Conflict(_))
    ));
    assert!(matches!(
        test.app
            .categories
            .create(&setup.ctx, Origin::Api, new(" "))
            .await,
        Err(AppError::Invalid { .. })
    ));
    let rename = CategoryUpdate {
        name: Some("Food".into()),
        notes: None,
    };
    let clash = test
        .app
        .categories
        .update(&setup.ctx, Origin::Api, other.id.parse().unwrap(), rename)
        .await;
    assert!(matches!(clash, Err(AppError::Conflict(_))));

    let page = test
        .app
        .categories
        .list(&setup.ctx, PageRequest::default())
        .await
        .unwrap();
    let names: Vec<_> = page.items.iter().map(|c| c.name.as_str()).collect();
    assert_eq!((names, page.total), (vec!["Food", "Fuel"], 2));
    assert_eq!(page.items[0], first);
}

#[tokio::test]
async fn deleting_a_category_clears_it_from_splits() {
    let (test, setup) = with_household().await;
    let checking = account(&test, &setup.ctx, "Checking", "asset", None).await;
    let food = account(&test, &setup.ctx, "Food", "expense", None).await;
    let category = test
        .app
        .categories
        .create(
            &setup.ctx,
            Origin::Api,
            NewCategory {
                name: "Food".into(),
                notes: None,
            },
        )
        .await
        .unwrap();
    let mut input = split(1, &checking, &food, "5");
    input.category_id = Some(category.id.parse().unwrap());
    let view = test
        .app
        .transactions
        .create(&setup.ctx, Origin::Api, group(vec![input]))
        .await
        .unwrap();

    test.app
        .categories
        .delete(&setup.ctx, Origin::Api, category.id.parse().unwrap())
        .await
        .unwrap();

    let after = test
        .app
        .transactions
        .get(&setup.ctx, view.id.parse().unwrap())
        .await
        .unwrap();
    assert_eq!(after.splits[0].category_id, None);
}

#[tokio::test]
async fn tag_lifecycle_with_events() {
    let (test, setup) = with_household().await;
    let created = test
        .app
        .tags
        .create(
            &setup.ctx,
            Origin::Api,
            NewTag {
                name: "holiday".into(),
                description: Some("2026".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        last_event(&test, &setup.ctx).await.event_type,
        "tag.created"
    );
    let id = created.id.parse().unwrap();
    let updated = test
        .app
        .tags
        .update(
            &setup.ctx,
            Origin::Api,
            id,
            TagUpdate {
                name: None,
                description: Some(None),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        (updated.name.as_str(), updated.description.clone()),
        ("holiday", None)
    );
    assert_eq!(
        last_event(&test, &setup.ctx).await.event_type,
        "tag.updated"
    );
    assert!(matches!(
        test.app
            .tags
            .create(
                &setup.ctx,
                Origin::Api,
                NewTag {
                    name: "holiday".into(),
                    description: None
                }
            )
            .await,
        Err(AppError::Conflict(_))
    ));
    assert_eq!(
        test.app
            .tags
            .list(&setup.ctx, PageRequest::default())
            .await
            .unwrap()
            .total,
        1
    );
    assert_eq!(test.app.tags.get(&setup.ctx, id).await.unwrap(), updated);
    test.app
        .tags
        .delete(&setup.ctx, Origin::Api, id)
        .await
        .unwrap();
    assert_eq!(
        last_event(&test, &setup.ctx).await.event_type,
        "tag.deleted"
    );
    assert!(test.app.tags.get(&setup.ctx, id).await.is_err());
}

#[tokio::test]
async fn currencies_are_listed_by_code() {
    let test = test_app().await;
    let currencies = test.app.currencies.list().await.unwrap();
    assert_eq!(currencies.first().map(|c| c.code.as_str()), Some("AUD"));
    let jpy = currencies.iter().find(|c| c.code == "JPY").unwrap();
    assert_eq!(jpy.decimal_places, 0);
}
