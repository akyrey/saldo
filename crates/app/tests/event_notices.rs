#![allow(clippy::unwrap_used)]

//! The short event notices behind `GET /events/stream`.

mod common;

use common::*;
use saldo_app::ctx::Origin;
use saldo_app::tags::NewTag;

#[tokio::test]
async fn notices_follow_the_household_log_in_order() {
    let (test, setup) = with_household().await;
    let other = other_household(&test).await;
    assert_eq!(test.app.events.latest_seq(&other).await.unwrap(), 0);
    let latest = test.app.events.latest_seq(&setup.ctx).await.unwrap();
    assert_eq!(latest, 4);

    let tag = NewTag {
        name: "x".into(),
        description: None,
    };
    let created = test
        .app
        .tags
        .create(&other, Origin::Api, tag.clone())
        .await
        .unwrap();
    let mine = test
        .app
        .tags
        .create(&setup.ctx, Origin::Api, tag)
        .await
        .unwrap();

    let notices = test
        .app
        .events
        .notices_after(&setup.ctx, latest, 10)
        .await
        .unwrap();
    assert_eq!(notices.len(), 1);
    let notice = &notices[0];
    assert_eq!(
        (
            notice.event_type.as_str(),
            notice.entity_type.as_str(),
            notice.entity_id.as_str()
        ),
        ("tag.created", "tag", mine.id.as_str())
    );
    let others = test.app.events.notices_after(&other, 0, 10).await.unwrap();
    assert_eq!(others.len(), 1);
    assert_eq!(others[0].entity_id, created.id);
    assert_eq!(
        test.app
            .events
            .notices_after(&setup.ctx, 0, 2)
            .await
            .unwrap()
            .len(),
        2
    );
    let json = serde_json::to_value(notice).unwrap();
    assert_eq!(json["type"], "tag.created");
}
