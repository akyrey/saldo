#![allow(clippy::unwrap_used)] // the seeding helper is test code that clippy does not see as tests

mod common;

use std::io::Read;

use common::*;
use saldo_app::attachments::NewAttachment;
use saldo_app::categories::NewCategory;
use saldo_app::ctx::Origin;
use saldo_app::error::AppError;
use saldo_app::external_refs::{ExternalRefs, FIREFLY};
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
    let mut first = split(1, &checking, &food, "42.50");
    first.description = "Shop \"weekly\"".into();
    first.tag_ids = vec![tag.id.parse().unwrap()];
    let mut second = split(1, &checking, &rent, "800");
    second.category_id = Some(category.id.parse().unwrap());
    let view = test
        .app
        .transactions
        .create(&setup.ctx, Origin::Api, group(vec![first, second]))
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
            "categories.csv",
            "currencies.csv",
            "members.csv",
            "tags.csv",
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
    assert_eq!(lines.len(), 1 + 3, "header plus one row per split");
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
    assert!(shop.contains(",monthly,"), "tag names: {shop}");
    assert!(
        lines.iter().any(|l| l.contains(",Rent,800.00,EUR,,,Home,")),
        "category name"
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
