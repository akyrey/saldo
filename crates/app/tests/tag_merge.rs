//! Merging tags and finding tags whose names differ only in case.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] functions

mod common;

use common::*;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::recurrences::{RecurrenceInput, RepetitionInput, TemplateSplitInput};
use saldo_app::tags::{NewTag, TagMerge, TagUpdate, TagView};
use saldo_app::templates::{NewTemplate, TemplateSplitInput as TemplatePart};
use saldo_core::TagId;
use uuid::Uuid;

async fn tag(test: &TestApp, ctx: &HouseholdCtx, name: &str, description: Option<&str>) -> TagView {
    test.app
        .tags
        .create(
            ctx,
            Origin::Api,
            NewTag {
                name: name.into(),
                description: description.map(Into::into),
            },
        )
        .await
        .unwrap()
}

/// A tag inserted directly, bypassing the case check (as old data may be).
async fn raw_tag(test: &TestApp, ctx: &HouseholdCtx, name: &str) -> String {
    let id = Uuid::now_v7().to_string();
    sqlx::query("INSERT INTO tags (id, household_id, name, created_at, updated_at) VALUES (?, ?, ?, 't', 't')")
        .bind(&id)
        .bind(ctx.household_id.to_string())
        .bind(name)
        .execute(&test.app.pool)
        .await
        .unwrap();
    id
}

fn id(tag: &str) -> TagId {
    tag.parse().unwrap()
}

async fn exec(test: &TestApp, sql: &str, binds: &[&str]) {
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.to_string()));
    for value in binds {
        query = query.bind(*value);
    }
    query.execute(&test.app.pool).await.unwrap();
}

async fn text(test: &TestApp, sql: &str) -> String {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_string()))
        .fetch_one(&test.app.pool)
        .await
        .unwrap()
}

async fn tagged_split(test: &TestApp, ctx: &HouseholdCtx, tags: Vec<TagId>) -> String {
    let bank = account(test, ctx, &format!("Bank {}", tags.len()), "asset", None).await;
    let shop = account(test, ctx, &format!("Shop {}", tags.len()), "expense", None).await;
    let mut input = split(1, &bank, &shop, "5.00");
    input.tag_ids = tags;
    let group = test
        .app
        .transactions
        .create(ctx, Origin::Api, group(vec![input]))
        .await
        .unwrap();
    group.splits[0].id.clone()
}

async fn split_tags(test: &TestApp, split_id: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT tag_id FROM split_tags WHERE split_id = ? ORDER BY tag_id")
        .bind(split_id)
        .fetch_all(&test.app.pool)
        .await
        .unwrap()
}

async fn recurrence_with(test: &TestApp, ctx: &HouseholdCtx, tags: Vec<TagId>) {
    let bank = account(test, ctx, "Checking", "asset", None).await;
    let landlord = account(test, ctx, "Landlord", "expense", None).await;
    let input = RecurrenceInput {
        title: "Rent".into(),
        description: None,
        first_date: date(1),
        repeat_until: None,
        max_occurrences: None,
        apply_rules: None,
        active: None,
        notes: None,
        repetitions: vec![RepetitionInput {
            kind: "monthly".into(),
            moment: "1".into(),
            skip: 0,
            weekend_policy: None,
        }],
        splits: vec![TemplateSplitInput {
            description: "Rent".into(),
            source_id: account_id(&bank),
            destination_id: account_id(&landlord),
            amount: "800.00".into(),
            currency: None,
            foreign_amount: None,
            foreign_currency: None,
            category_id: None,
            budget_id: None,
            bill_id: None,
            piggy_bank_id: None,
            tag_ids: tags,
        }],
    };
    test.app
        .recurrences
        .create(ctx, Origin::Api, input)
        .await
        .unwrap();
}

async fn template_with(test: &TestApp, ctx: &HouseholdCtx, tags: Vec<TagId>) {
    let input = NewTemplate {
        name: "Groceries".into(),
        kind: "withdrawal".into(),
        splits: vec![TemplatePart {
            tag_ids: tags,
            ..TemplatePart::default()
        }],
        ..NewTemplate::default()
    };
    test.app
        .templates
        .create(ctx, Origin::Api, input)
        .await
        .unwrap();
}

/// A rule `tag_is <tag>` → `add_tag <tag>`, an attachment on the tag and a
/// Firefly reference to it, all inserted directly.
async fn other_references(test: &TestApp, ctx: &HouseholdCtx, tag: &str) {
    let h = ctx.household_id.to_string();
    exec(test, "INSERT INTO rule_groups (id, household_id, title, created_at, updated_at) VALUES ('g', ?, 'G', 't', 't')", &[&h]).await;
    exec(test, "INSERT INTO rules (id, household_id, rule_group_id, title, trigger_on, created_at, updated_at) VALUES ('r', ?, 'g', 'R', 'create', 't', 't')", &[&h]).await;
    exec(test, "INSERT INTO rule_triggers (id, rule_id, position, kind, value) VALUES ('rt', 'r', 0, 'tag_is', ?)", &[tag]).await;
    exec(test, "INSERT INTO rule_actions (id, rule_id, position, kind, value) VALUES ('ra', 'r', 0, 'add_tag', ?)", &[tag]).await;
    exec(test, "INSERT INTO attachments (id, household_id, attachable_type, attachable_id, filename, mime_type, size_bytes, sha256, created_at) VALUES ('a', ?, 'tag', ?, 'f.pdf', 'application/pdf', 1, 'x', 't')", &[&h, tag]).await;
    exec(test, "INSERT INTO external_refs (household_id, source, entity_type, external_id, local_id, updated_at) VALUES (?, 'firefly-iii', 'tag', '7', ?, 't')", &[&h, tag]).await;
}

#[tokio::test]
async fn merging_moves_every_reference_to_the_target_and_deletes_the_source() {
    let (test, setup) = with_household().await;
    let ctx = &setup.ctx;
    let target = tag(&test, ctx, "Food", None).await;
    let source = raw_tag(&test, ctx, "food").await;
    sqlx::query("UPDATE tags SET description = 'meals' WHERE id = ?")
        .bind(&source)
        .execute(&test.app.pool)
        .await
        .unwrap();
    let only_source = tagged_split(&test, ctx, vec![id(&source)]).await;
    let both = tagged_split(&test, ctx, vec![id(&source), id(&target.id)]).await;
    recurrence_with(&test, ctx, vec![id(&source), id(&target.id)]).await;
    template_with(&test, ctx, vec![id(&source)]).await;
    other_references(&test, ctx, &source).await;

    let merged = test
        .app
        .tags
        .merge(
            ctx,
            Origin::Api,
            vec![TagMerge {
                into: id(&target.id),
                sources: vec![id(&source)],
            }],
        )
        .await
        .unwrap();

    assert_eq!(merged.len(), 1);
    assert_eq!(
        (merged[0].name.as_str(), merged[0].description.as_deref()),
        ("Food", Some("meals")),
        "the target keeps its name and takes the description it lacked"
    );
    assert!(matches!(
        test.app.tags.get(ctx, id(&source)).await,
        Err(AppError::NotFound)
    ));
    assert_eq!(
        split_tags(&test, &only_source).await,
        vec![target.id.clone()]
    );
    assert_eq!(split_tags(&test, &both).await, vec![target.id.clone()]);
    let expected = format!("[\"{}\"]", target.id);
    assert_eq!(
        text(&test, "SELECT tags FROM recurrence_splits").await,
        expected
    );
    assert_eq!(
        text(
            &test,
            "SELECT json_extract(splits, '$[0].tag_ids') FROM transaction_templates"
        )
        .await,
        expected
    );
    assert_eq!(
        text(&test, "SELECT value FROM rule_triggers").await,
        target.id
    );
    assert_eq!(
        text(&test, "SELECT value FROM rule_actions").await,
        target.id
    );
    assert_eq!(
        text(&test, "SELECT attachable_id FROM attachments").await,
        target.id
    );
    assert_eq!(
        text(&test, "SELECT local_id FROM external_refs").await,
        target.id
    );

    let event = last_event(&test, ctx).await;
    assert_eq!(event.event_type, "tag.merged");
    assert_eq!(event.entity_id, source);
    assert_eq!(
        event.payload["data"]["into"]["id"],
        serde_json::json!(target.id)
    );
    assert_eq!(
        event.payload["data"]["source"]["name"],
        serde_json::json!("food")
    );
}

#[tokio::test]
async fn several_groups_merge_in_one_go() {
    let (test, setup) = with_household().await;
    let ctx = &setup.ctx;
    let food = tag(&test, ctx, "Food", None).await;
    let food_dupes = [
        raw_tag(&test, ctx, "food").await,
        raw_tag(&test, ctx, "FOOD").await,
    ];
    let rent = tag(&test, ctx, "rent", None).await;
    let rent_dupe = raw_tag(&test, ctx, "Rent").await;

    let merged = test
        .app
        .tags
        .merge(
            ctx,
            Origin::Api,
            vec![
                TagMerge {
                    into: id(&food.id),
                    sources: food_dupes.iter().map(|t| id(t)).collect(),
                },
                TagMerge {
                    into: id(&rent.id),
                    sources: vec![id(&rent_dupe)],
                },
            ],
        )
        .await
        .unwrap();

    let names: Vec<&str> = merged.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Food", "rent"]);
    assert_eq!(count(&test, "tags").await, 2);
}

#[tokio::test]
async fn a_failed_merge_changes_nothing() {
    let (test, setup) = with_household().await;
    let ctx = &setup.ctx;
    let food = tag(&test, ctx, "Food", None).await;
    let dupe = raw_tag(&test, ctx, "food").await;
    let unknown = TagId(Uuid::now_v7());
    let merge = |into: TagId, sources: Vec<TagId>| vec![TagMerge { into, sources }];

    let cases = [
        (
            merge(id(&food.id), vec![id(&dupe), unknown]),
            "unknown source",
        ),
        (merge(unknown, vec![id(&dupe)]), "unknown target"),
    ];
    for (input, why) in cases {
        let result = test.app.tags.merge(ctx, Origin::Api, input).await;
        assert!(matches!(result, Err(AppError::NotFound)), "{why}");
    }
    for (input, field) in [
        (merge(id(&food.id), vec![]), "sources"),
        (merge(id(&food.id), vec![id(&food.id)]), "sources"),
        (vec![], "merges"),
    ] {
        let result = test.app.tags.merge(ctx, Origin::Api, input).await;
        assert!(
            matches!(&result, Err(AppError::Invalid { field: f, .. }) if f == field),
            "{result:?}"
        );
    }
    assert_eq!(count(&test, "tags").await, 2);
}

#[tokio::test]
async fn another_households_tags_cannot_be_merged() {
    let (test, setup) = with_household().await;
    let other = other_household(&test).await;
    let mine = tag(&test, &setup.ctx, "Food", None).await;
    let theirs = tag(&test, &other, "food", None).await;
    for (into, source) in [(&mine.id, &theirs.id), (&theirs.id, &mine.id)] {
        let result = test
            .app
            .tags
            .merge(
                &setup.ctx,
                Origin::Api,
                vec![TagMerge {
                    into: id(into),
                    sources: vec![id(source)],
                }],
            )
            .await;
        assert!(matches!(result, Err(AppError::NotFound)));
    }
    assert_eq!(count(&test, "tags").await, 2);
}

#[tokio::test]
async fn case_duplicates_are_grouped_most_used_first() {
    let (test, setup) = with_household().await;
    let ctx = &setup.ctx;
    let food = tag(&test, ctx, "Food", None).await;
    let food_lower = raw_tag(&test, ctx, "food").await;
    raw_tag(&test, ctx, "FOOD").await;
    tag(&test, ctx, "Rent", None).await;
    tag(&test, ctx, "città", None).await;
    raw_tag(&test, ctx, "CITTÀ").await;
    tagged_split(&test, ctx, vec![id(&food_lower)]).await;
    tagged_split(&test, ctx, vec![id(&food_lower), id(&food.id)]).await;

    let groups = test.app.tags.case_duplicates(ctx).await.unwrap();

    let shape: Vec<Vec<(&str, i64)>> = groups
        .iter()
        .map(|g| {
            g.tags
                .iter()
                .map(|t| (t.name.as_str(), t.split_count))
                .collect()
        })
        .collect();
    assert_eq!(
        shape,
        vec![
            vec![("CITTÀ", 0), ("città", 0)],
            vec![("food", 2), ("Food", 1), ("FOOD", 0)],
        ]
    );
}

#[tokio::test]
async fn names_are_unique_ignoring_case() {
    let (test, setup) = with_household().await;
    let ctx = &setup.ctx;
    let food = tag(&test, ctx, "Food", None).await;
    let rent = tag(&test, ctx, "Rent", None).await;
    let create = test
        .app
        .tags
        .create(
            ctx,
            Origin::Api,
            NewTag {
                name: " FOOD ".into(),
                description: None,
            },
        )
        .await;
    assert!(matches!(create, Err(AppError::Conflict(_))));
    let rename = |name: &str| TagUpdate {
        name: Some(name.into()),
        description: None,
    };
    let clash = test
        .app
        .tags
        .update(ctx, Origin::Api, id(&rent.id), rename("food"))
        .await;
    assert!(matches!(clash, Err(AppError::Conflict(_))));
    let recased = test
        .app
        .tags
        .update(ctx, Origin::Api, id(&food.id), rename("FOOD"))
        .await
        .unwrap();
    assert_eq!(recased.name, "FOOD");

    // Old duplicates can still be edited as long as the name stays.
    let dupe = raw_tag(&test, ctx, "food").await;
    let described = test
        .app
        .tags
        .update(
            ctx,
            Origin::Api,
            id(&dupe),
            TagUpdate {
                name: Some("food".into()),
                description: Some(Some("meals".into())),
            },
        )
        .await
        .unwrap();
    assert_eq!(described.description.as_deref(), Some("meals"));
}

#[tokio::test]
async fn names_by_lowercase_find_tags_in_any_case() {
    let (test, setup) = with_household().await;
    let food = tag(&test, &setup.ctx, "Food", None).await;
    let mut conn = test.app.pool.acquire().await.unwrap();
    let found = saldo_app::tags::find_by_name_ignoring_case(&mut conn, &setup.ctx, " fOOD ")
        .await
        .unwrap();
    assert_eq!(found.map(|t| t.id), Some(food.id.clone()));
    let names = saldo_app::tags::ids_by_name(&mut conn, &setup.ctx)
        .await
        .unwrap();
    assert_eq!(names.get("food"), Some(&food.id));
}
