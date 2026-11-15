#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

//! Attachments go when what they belong to goes: a deleted transaction, a
//! split left out of an update, a deleted bill. Files without a row are
//! removed later by `remove_unused_files`.

mod common;

use std::path::PathBuf;
use std::time::Duration;

use common::*;
use saldo_app::attachments::{AttachmentView, NewAttachment};
use saldo_app::bills::NewBill;
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::transactions::TransactionGroupView;
use saldo_core::{BillId, GroupId, SplitId};

fn data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("saldo-clean-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

async fn attach(
    test: &TestApp,
    ctx: &HouseholdCtx,
    kind: &str,
    id: &str,
    bytes: &[u8],
) -> AttachmentView {
    test.app
        .attachments
        .upload(
            ctx,
            Origin::Api,
            NewAttachment {
                attachable_type: kind.into(),
                attachable_id: id.into(),
                filename: "receipt.pdf".into(),
                title: None,
                notes: None,
                mime_type: "application/pdf".into(),
                bytes: bytes.to_vec(),
            },
        )
        .await
        .unwrap()
}

async fn files_of(test: &TestApp, ctx: &HouseholdCtx, kind: &str, id: &str) -> Vec<AttachmentView> {
    test.app.attachments.list_for(ctx, kind, id).await.unwrap()
}

/// A household with two accounts and a two-split withdrawal.
async fn two_splits(test: &TestApp, ctx: &HouseholdCtx) -> TransactionGroupView {
    let checking = account(test, ctx, "Checking", "asset", None).await;
    let food = account(test, ctx, "Food", "expense", None).await;
    test.app
        .transactions
        .create(
            ctx,
            Origin::Api,
            group(vec![
                split(1, &checking, &food, "5"),
                split(2, &checking, &food, "7"),
            ]),
        )
        .await
        .unwrap()
}

fn group_id(view: &TransactionGroupView) -> GroupId {
    view.id.parse().unwrap()
}

fn split_id(view: &TransactionGroupView, index: usize) -> SplitId {
    view.splits[index].id.parse().unwrap()
}

async fn deleted_attachment_events(test: &TestApp, ctx: &HouseholdCtx) -> usize {
    events(test, ctx)
        .await
        .iter()
        .filter(|e| e.event_type == "attachment.deleted")
        .count()
}

#[tokio::test]
async fn deleting_a_transaction_deletes_its_attachments() {
    let dir = data_dir("group");
    let (mut test, setup) = with_household().await;
    test.app = test.app.with_data_dir(dir.clone());
    let view = two_splits(&test, &setup.ctx).await;
    attach(&test, &setup.ctx, "split", &view.splits[0].id, b"first").await;
    attach(&test, &setup.ctx, "split", &view.splits[1].id, b"second").await;

    test.app
        .transactions
        .delete(&setup.ctx, Origin::Api, group_id(&view))
        .await
        .unwrap();

    assert_eq!(count(&test, "attachments").await, 0);
    assert_eq!(deleted_attachment_events(&test, &setup.ctx).await, 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn an_update_keeps_named_splits_and_their_attachments() {
    let dir = data_dir("keep");
    let (mut test, setup) = with_household().await;
    test.app = test.app.with_data_dir(dir.clone());
    let view = two_splits(&test, &setup.ctx).await;
    let kept = attach(&test, &setup.ctx, "split", &view.splits[0].id, b"kept").await;
    attach(&test, &setup.ctx, "split", &view.splits[1].id, b"dropped").await;

    // Keep the first split (changed amount), leave the second out, add one.
    let mut input = saldo_app::transactions::input_from_view(&view).unwrap();
    input.splits[0].amount = "6".into();
    let fresh = saldo_app::transactions::SplitInput {
        id: None,
        ..input.splits[1].clone()
    };
    input.splits = vec![input.splits[0].clone(), fresh];
    let updated = test
        .app
        .transactions
        .replace(&setup.ctx, Origin::Api, group_id(&view), input)
        .await
        .unwrap();

    assert_eq!(
        updated.splits[0].id, view.splits[0].id,
        "named split keeps its ID"
    );
    assert_eq!(updated.splits[0].amount, "6.00");
    assert_ne!(updated.splits[1].id, view.splits[1].id, "new split, new ID");
    assert_eq!(
        files_of(&test, &setup.ctx, "split", &view.splits[0].id).await,
        vec![kept]
    );
    assert!(
        files_of(&test, &setup.ctx, "split", &view.splits[1].id)
            .await
            .is_empty()
    );
    assert_eq!(deleted_attachment_events(&test, &setup.ctx).await, 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn an_update_rejects_split_ids_of_another_group_or_twice() {
    let (test, setup) = with_household().await;
    let first = two_splits(&test, &setup.ctx).await;
    let checking = account(&test, &setup.ctx, "Wallet", "asset", None).await;
    let food = account(&test, &setup.ctx, "Snacks", "expense", None).await;
    let second = test
        .app
        .transactions
        .create(
            &setup.ctx,
            Origin::Api,
            group(vec![split(3, &checking, &food, "1")]),
        )
        .await
        .unwrap();

    let mut foreign = saldo_app::transactions::input_from_view(&second).unwrap();
    foreign.splits[0].id = Some(split_id(&first, 0));
    let result = test
        .app
        .transactions
        .replace(&setup.ctx, Origin::Api, group_id(&second), foreign)
        .await;
    assert!(
        matches!(&result, Err(AppError::Invalid { field, .. }) if field == "splits[0].id"),
        "{result:?}"
    );

    let mut twice = saldo_app::transactions::input_from_view(&first).unwrap();
    twice.splits[1].id = twice.splits[0].id;
    let result = test
        .app
        .transactions
        .replace(&setup.ctx, Origin::Api, group_id(&first), twice)
        .await;
    assert!(
        matches!(&result, Err(AppError::Invalid { field, .. }) if field == "splits[1].id"),
        "{result:?}"
    );
}

#[tokio::test]
async fn a_new_transaction_cannot_name_split_ids() {
    let (test, setup) = with_household().await;
    let existing = two_splits(&test, &setup.ctx).await;
    let mut input = saldo_app::transactions::input_from_view(&existing).unwrap();
    input.splits.truncate(1);
    let result = test
        .app
        .transactions
        .create(&setup.ctx, Origin::Api, input)
        .await;
    assert!(
        matches!(&result, Err(AppError::Invalid { field, .. }) if field == "splits[0].id"),
        "{result:?}"
    );
}

#[tokio::test]
async fn deleting_a_bill_deletes_its_attachments() {
    let dir = data_dir("bill");
    let (mut test, setup) = with_household().await;
    test.app = test.app.with_data_dir(dir.clone());
    let bill = test
        .app
        .bills
        .create(
            &setup.ctx,
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
    attach(&test, &setup.ctx, "bill", &bill.id, b"contract").await;
    let bill_id: BillId = bill.id.parse().unwrap();

    test.app
        .bills
        .delete(&setup.ctx, Origin::Api, bill_id)
        .await
        .unwrap();

    assert_eq!(count(&test, "attachments").await, 0);
    assert_eq!(deleted_attachment_events(&test, &setup.ctx).await, 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn unused_files_are_removed_once_old_enough() {
    let dir = data_dir("sweep");
    let (mut test, setup) = with_household().await;
    test.app = test.app.with_data_dir(dir.clone());
    let view = two_splits(&test, &setup.ctx).await;
    let gone = attach(&test, &setup.ctx, "split", &view.splits[0].id, b"gone").await;
    let shared = attach(&test, &setup.ctx, "split", &view.splits[1].id, b"shared").await;
    let checking = account(&test, &setup.ctx, "Savings", "asset", None).await;
    attach(&test, &setup.ctx, "account", &checking.id, b"shared").await;
    test.app
        .transactions
        .delete(&setup.ctx, Origin::Api, group_id(&view))
        .await
        .unwrap();
    let path = |sha: &str| dir.join("attachments").join(sha);
    assert!(path(&gone.sha256).exists(), "files wait for the sweep");

    let young = test
        .app
        .attachments
        .remove_unused_files(Duration::from_secs(3600))
        .await
        .unwrap();
    assert_eq!(
        young, 0,
        "a file this new may belong to an upload in progress"
    );
    assert!(path(&gone.sha256).exists());

    let removed = test
        .app
        .attachments
        .remove_unused_files(Duration::ZERO)
        .await
        .unwrap();
    assert_eq!(removed, 1);
    assert!(!path(&gone.sha256).exists());
    assert!(path(&shared.sha256).exists(), "still used by the account");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn the_sweep_copes_with_no_folder() {
    let dir = data_dir("empty");
    let (mut test, _setup) = with_household().await;
    test.app = test.app.with_data_dir(dir.clone());
    let removed = test
        .app
        .attachments
        .remove_unused_files(Duration::ZERO)
        .await
        .unwrap();
    assert_eq!(removed, 0);
}

#[tokio::test]
async fn kept_splits_can_change_places() {
    let (test, setup) = with_household().await;
    let view = two_splits(&test, &setup.ctx).await;
    let mut input = saldo_app::transactions::input_from_view(&view).unwrap();
    input.splits.reverse();
    let updated = test
        .app
        .transactions
        .replace(&setup.ctx, Origin::Api, group_id(&view), input)
        .await
        .unwrap();
    let ids: Vec<_> = updated.splits.iter().map(|s| s.id.clone()).collect();
    assert_eq!(
        ids,
        vec![view.splits[1].id.clone(), view.splits[0].id.clone()]
    );
    assert_eq!(updated.splits[0].amount, "7.00");
}
