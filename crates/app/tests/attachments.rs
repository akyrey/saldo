mod common;

use std::path::PathBuf;

use common::*;
use saldo_app::attachments::{MAX_SIZE, NewAttachment};
use saldo_app::ctx::Origin;
use saldo_app::error::AppError;
use saldo_core::AttachmentId;

/// A fresh folder per test under the system temp dir.
fn data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("saldo-att-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn file(attachable_type: &str, attachable_id: &str, bytes: &[u8]) -> NewAttachment {
    NewAttachment {
        attachable_type: attachable_type.into(),
        attachable_id: attachable_id.into(),
        filename: "receipt.pdf".into(),
        title: Some("Receipt".into()),
        notes: None,
        mime_type: "application/pdf".into(),
        bytes: bytes.to_vec(),
    }
}

#[tokio::test]
async fn upload_download_and_delete_with_shared_files() {
    let dir = data_dir("cycle");
    let (mut test, setup) = with_household().await;
    test.app = test.app.with_data_dir(dir.clone());
    let checking = account(&test, &setup.ctx, "Checking", "asset", None).await;
    let food = account(&test, &setup.ctx, "Food", "expense", None).await;
    let group_view = test
        .app
        .transactions
        .create(
            &setup.ctx,
            Origin::Api,
            group(vec![split(1, &checking, &food, "5")]),
        )
        .await
        .unwrap();
    let split_id = group_view.splits[0].id.clone();

    let first = test
        .app
        .attachments
        .upload(
            &setup.ctx,
            Origin::Api,
            file("split", &split_id, b"%PDF-1 receipt"),
        )
        .await
        .unwrap();
    let second = test
        .app
        .attachments
        .upload(
            &setup.ctx,
            Origin::Api,
            file("account", &checking.id, b"%PDF-1 receipt"),
        )
        .await
        .unwrap();
    assert_eq!((first.size_bytes, first.sha256.len()), (14, 64));
    assert_eq!(first.sha256, second.sha256, "same content, one file");
    assert_eq!(
        last_event(&test, &setup.ctx).await.event_type,
        "attachment.created"
    );
    let stored = dir.join("attachments").join(&first.sha256);
    assert!(stored.exists());

    let first_id: AttachmentId = first.id.parse().unwrap();
    let (view, bytes) = test
        .app
        .attachments
        .download(&setup.ctx, first_id)
        .await
        .unwrap();
    assert_eq!(
        (view, bytes.as_slice()),
        (first.clone(), b"%PDF-1 receipt".as_slice())
    );

    test.app
        .attachments
        .delete(&setup.ctx, Origin::Api, first_id)
        .await
        .unwrap();
    assert!(stored.exists(), "still used by the second attachment");
    assert_eq!(
        last_event(&test, &setup.ctx).await.event_type,
        "attachment.deleted"
    );
    test.app
        .attachments
        .delete(&setup.ctx, Origin::Api, second.id.parse().unwrap())
        .await
        .unwrap();
    assert!(!stored.exists(), "last reference gone, file removed");
    assert!(matches!(
        test.app.attachments.get(&setup.ctx, first_id).await,
        Err(AppError::NotFound)
    ));
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn rejects_bad_uploads_without_writing() {
    let dir = data_dir("reject");
    let (mut test, setup) = with_household().await;
    test.app = test.app.with_data_dir(dir.clone());
    let checking = account(&test, &setup.ctx, "Checking", "asset", None).await;
    let other = other_household(&test).await;
    let events = count(&test, "events").await;

    let cases = [
        (
            "attachable_type",
            NewAttachment {
                attachable_type: "bill".into(),
                ..file("account", &checking.id, b"x")
            },
        ),
        (
            "filename",
            NewAttachment {
                filename: "../etc/passwd".into(),
                ..file("account", &checking.id, b"x")
            },
        ),
        (
            "mime_type",
            NewAttachment {
                mime_type: "text/html".into(),
                ..file("account", &checking.id, b"x")
            },
        ),
        ("file", file("account", &checking.id, b"")),
        (
            "file",
            file("account", &checking.id, &vec![0u8; MAX_SIZE + 1]),
        ),
        (
            "attachable_id",
            file("account", "0190f5c3-7a2b-7c00-8000-000000000001", b"x"),
        ),
    ];
    for (field, input) in cases {
        let err = test
            .app
            .attachments
            .upload(&setup.ctx, Origin::Api, input)
            .await
            .unwrap_err();
        assert!(
            matches!(&err, AppError::Invalid { field: f, .. } if f == field),
            "{field}: {err:?}"
        );
    }
    // Another household cannot attach to this household's account.
    let foreign = test
        .app
        .attachments
        .upload(&other, Origin::Api, file("account", &checking.id, b"x"))
        .await;
    assert!(matches!(foreign, Err(AppError::Invalid { .. })));
    assert_eq!(count(&test, "attachments").await, 0);
    assert_eq!(count(&test, "events").await, events);
    assert!(!dir.join("attachments").exists());
}

#[tokio::test]
async fn other_households_cannot_read_or_delete() {
    let dir = data_dir("isolation");
    let (mut test, setup) = with_household().await;
    test.app = test.app.with_data_dir(dir.clone());
    let checking = account(&test, &setup.ctx, "Checking", "asset", None).await;
    let tag = test
        .app
        .tags
        .create(
            &setup.ctx,
            Origin::Api,
            saldo_app::tags::NewTag {
                name: "t".into(),
                description: None,
            },
        )
        .await
        .unwrap();
    let mine = test
        .app
        .attachments
        .upload(&setup.ctx, Origin::Api, file("tag", &tag.id, b"tag file"))
        .await
        .unwrap();
    let other = other_household(&test).await;
    let id: AttachmentId = mine.id.parse().unwrap();
    assert!(matches!(
        test.app.attachments.get(&other, id).await,
        Err(AppError::NotFound)
    ));
    assert!(matches!(
        test.app.attachments.download(&other, id).await,
        Err(AppError::NotFound)
    ));
    assert!(matches!(
        test.app.attachments.delete(&other, Origin::Api, id).await,
        Err(AppError::NotFound)
    ));
    let mut conn = test.app.pool.acquire().await.unwrap();
    assert_eq!(
        test.app
            .attachments
            .list_in(&mut conn, &setup.ctx)
            .await
            .unwrap(),
        vec![mine]
    );
    assert!(
        test.app
            .attachments
            .list_in(&mut conn, &other)
            .await
            .unwrap()
            .is_empty()
    );
    drop(conn);
    let _ = checking;
    let _ = std::fs::remove_dir_all(dir);
}
