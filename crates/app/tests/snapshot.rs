//! Snapshots on a real database file (`VACUUM INTO` writes nothing for the
//! shared in-memory databases the other tests use).

#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use common::*;
use saldo_app::clock::FixedClock;
use saldo_app::ctx::Origin;
use saldo_app::error::AppError;
use saldo_app::identity::SessionClient;
use saldo_app::snapshot::{self, Restored, Scrubbed};
use saldo_app::{App, db};

const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("saldo-snapshot-{name}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// An app on `<dir>/saldo.db` with its files under `dir`.
async fn file_app(dir: &Path) -> TestApp {
    let url = format!("sqlite://{}", dir.join("saldo.db").display());
    let pool = db::connect(&url).await.unwrap();
    db::migrate(&pool).await.unwrap();
    let clock = Arc::new(FixedClock::new(
        Utc.with_ymd_and_hms(2026, 11, 13, 9, 0, 0).unwrap(),
    ));
    TestApp {
        app: App::new(pool, clock.clone()).with_data_dir(dir.to_path_buf()),
        clock,
    }
}

/// A webhook endpoint and a notification channel, inserted directly.
async fn add_secrets(test: &TestApp, household_id: &str, user_id: &str) {
    for sql in [
        "INSERT INTO webhook_endpoints (id, household_id, name, url, secret, created_at, updated_at) VALUES ('w1', ?1, 'Hook', 'https://example.com/hook', 'secret', 't', 't')",
        "INSERT INTO notification_channels (id, household_id, user_id, kind, name, config, created_at, updated_at) VALUES ('c1', ?1, ?2, 'ntfy', 'Phone', '{}', 't', 't')",
    ] {
        sqlx::query(sql)
            .bind(household_id)
            .bind(user_id)
            .execute(&test.app.pool)
            .await
            .unwrap();
    }
}

fn read_all(mut file: std::fs::File) -> Vec<u8> {
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    bytes
}

#[tokio::test]
async fn a_snapshot_restores_into_a_fresh_instance_without_secrets() {
    // Arrange: a household with an account, a session, secrets and a file.
    let source_dir = temp_dir("source");
    let source = file_app(&source_dir).await;
    let setup = source
        .app
        .identity
        .setup(Origin::Api, setup_input("owner@example.com"))
        .await
        .unwrap();
    account(&source, &setup.ctx, "Checking", "asset", None).await;
    source
        .app
        .identity
        .login("owner@example.com", PASSWORD, SessionClient::Browser)
        .await
        .unwrap();
    let (household_id, user_id) = (
        setup.ctx.household_id.to_string(),
        setup.ctx.user_id.to_string(),
    );
    add_secrets(&source, &household_id, &user_id).await;
    std::fs::create_dir_all(source_dir.join("attachments")).unwrap();
    std::fs::write(source_dir.join("attachments").join(SHA), b"a receipt").unwrap();

    // Act: snapshot, restore into a new folder, migrate and scrub.
    let file = source.app.snapshots.create(&setup.ctx).await.unwrap();
    let target_dir = temp_dir("target");
    let archive = target_dir.join("snapshot.zip");
    std::fs::write(&archive, read_all(file)).unwrap();
    let database = target_dir.join("data/saldo.db");
    let restored = snapshot::restore(&archive, &database, &target_dir.join("data")).unwrap();
    let target = file_app(&target_dir.join("data")).await;
    let scrubbed = snapshot::scrub(&target.app.pool).await.unwrap();

    // Assert: the ledger came across, the secrets did not, no temp files remain.
    assert_eq!(restored, Restored { files: 1 });
    assert_eq!(
        scrubbed,
        Scrubbed {
            sessions: 1,
            webhooks: 1,
            notification_channels: 1
        }
    );
    assert_eq!(
        count(&target, "accounts").await,
        count(&source, "accounts").await
    );
    assert_eq!(
        count(&target, "events").await,
        count(&source, "events").await
    );
    assert_eq!(count(&target, "sessions").await, 0);
    assert_eq!(count(&target, "webhook_endpoints").await, 0);
    assert_eq!(count(&target, "notification_channels").await, 0);
    assert_eq!(
        std::fs::read(target_dir.join("data/attachments").join(SHA)).unwrap(),
        b"a receipt"
    );
    let leftovers: Vec<_> = std::fs::read_dir(&source_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".snapshot-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    // The restored owner can still log in with their password.
    target
        .app
        .identity
        .login("owner@example.com", PASSWORD, SessionClient::Browser)
        .await
        .unwrap();
}

#[tokio::test]
async fn only_owners_take_snapshots() {
    let dir = temp_dir("member");
    let test = file_app(&dir).await;
    let setup = test
        .app
        .identity
        .setup(Origin::Api, setup_input("owner@example.com"))
        .await
        .unwrap();
    let member = add_member(&test, &setup.ctx, "member@example.com").await;

    let error = test.app.snapshots.create(&member).await.unwrap_err();

    assert!(matches!(error, AppError::Forbidden));
}

#[tokio::test]
async fn no_snapshot_while_other_households_exist() {
    let dir = temp_dir("shared");
    let test = file_app(&dir).await;
    let setup = test
        .app
        .identity
        .setup(Origin::Api, setup_input("owner@example.com"))
        .await
        .unwrap();
    other_household(&test).await;

    let error = test.app.snapshots.create(&setup.ctx).await.unwrap_err();

    assert!(matches!(error, AppError::Conflict(_)));
}
