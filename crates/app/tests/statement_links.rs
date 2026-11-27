#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

//! Brief decision 11: deleting a split forgets its bank-statement link, so a
//! re-import proposes the line again instead of calling it a duplicate.

mod common;

use common::*;
use saldo_app::ctx::Origin;
use saldo_app::external_refs::{BANK_STATEMENT, ExternalRefs, FIREFLY};
use saldo_app::transactions::input_from_view;

#[tokio::test]
async fn deleting_or_dropping_a_split_forgets_only_its_bank_statement_link() {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let shop = account(&test, &ctx, "Shop", "expense", None).await;
    let first = test
        .app
        .transactions
        .create(
            &ctx,
            Origin::Api,
            group(vec![
                split(3, &checking, &shop, "1.00"),
                split(3, &checking, &shop, "2.00"),
            ]),
        )
        .await
        .unwrap();
    let second = test
        .app
        .transactions
        .create(
            &ctx,
            Origin::Api,
            group(vec![split(4, &checking, &shop, "3.00")]),
        )
        .await
        .unwrap();
    let (a, b, c) = (
        &first.splits[0].id,
        &first.splits[1].id,
        &second.splits[0].id,
    );

    let mut conn = test.app.pool.acquire().await.unwrap();
    let mut statement = ExternalRefs::load(&mut conn, &ctx, BANK_STATEMENT)
        .await
        .unwrap();
    let mut firefly = ExternalRefs::load(&mut conn, &ctx, FIREFLY).await.unwrap();
    for (key, id) in [("line-a", a), ("line-b", b), ("line-c", c)] {
        statement
            .upsert(&mut conn, "split", key, id, "now")
            .await
            .unwrap();
    }
    firefly
        .upsert(&mut conn, "split", "42", c, "now")
        .await
        .unwrap();
    drop(conn);

    // Saving the first group without its second split drops that split.
    let mut input = input_from_view(&first).unwrap();
    input.splits.truncate(1);
    test.app
        .transactions
        .replace(&ctx, Origin::Api, first.id.parse().unwrap(), input)
        .await
        .unwrap();
    // Deleting the second group deletes its split.
    test.app
        .transactions
        .delete(&ctx, Origin::Api, second.id.parse().unwrap())
        .await
        .unwrap();

    let mut conn = test.app.pool.acquire().await.unwrap();
    let statement = ExternalRefs::load(&mut conn, &ctx, BANK_STATEMENT)
        .await
        .unwrap();
    assert_eq!(
        statement.entries("split"),
        vec![("line-a".to_string(), a.clone())]
    );
    // A Firefly reference is the importer's business and stays.
    let firefly = ExternalRefs::load(&mut conn, &ctx, FIREFLY).await.unwrap();
    assert_eq!(firefly.resolve("split", "42"), Some(c.as_str()));
}
