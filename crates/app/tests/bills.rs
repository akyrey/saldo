#![allow(clippy::unwrap_used)] // helpers below are test code that clippy does not see as tests

mod common;

use chrono::NaiveDate;
use common::*;
use saldo_app::bills::{BillService, BillUpdate, NewBill};
use saldo_app::ctx::Origin;
use saldo_app::error::AppError;
use saldo_core::{BillId, BillPeriodId, GroupId, ReminderKind};

fn day(text: &str) -> NaiveDate {
    text.parse().unwrap()
}

fn rent(first: &str) -> NewBill {
    NewBill {
        name: "Rent".into(),
        amount_min: "800".into(),
        amount_max: "800".into(),
        currency: None,
        first_due_date: day(first),
        end_date: None,
        repeat_freq: "monthly".into(),
        skip: None,
        remind_days_before: None,
        remind_every_days: None,
        active: None,
        sort_order: None,
        notes: None,
    }
}

async fn due_dates(
    test: &TestApp,
    ctx: &saldo_app::ctx::HouseholdCtx,
    bill: BillId,
    status: &str,
) -> Vec<String> {
    let mut periods = test.app.bills.periods(ctx, bill).await.unwrap();
    periods.reverse();
    periods
        .into_iter()
        .filter(|p| p.status == status)
        .map(|p| p.due_date)
        .collect()
}

#[tokio::test]
async fn creating_a_bill_creates_its_periods_sixty_days_ahead() {
    let (test, setup) = with_household().await;
    let view = test
        .app
        .bills
        .create(&setup.ctx, Origin::Api, rent("2026-09-02"))
        .await
        .unwrap();
    assert_eq!(
        (
            view.amount_min.as_str(),
            view.currency.as_str(),
            view.repeat_freq.as_str()
        ),
        ("800.00", "EUR", "monthly")
    );
    assert_eq!((view.remind_days_before, view.remind_every_days), (3, 1));
    assert_eq!(view.next_due_date.as_deref(), Some("2026-09-02"));
    // Today is 2026-11-13: periods through 2027-01-12.
    let id: BillId = view.id.parse().unwrap();
    assert_eq!(
        due_dates(&test, &setup.ctx, id, "open").await,
        [
            "2026-09-02",
            "2026-10-02",
            "2026-11-02",
            "2026-12-02",
            "2027-01-02"
        ]
    );
    assert_eq!(
        last_event(&test, &setup.ctx).await.event_type,
        "bill.created"
    );

    test.app
        .bills
        .materialise_periods(&setup.ctx, day("2026-11-13"))
        .await
        .unwrap();
    assert_eq!(
        count(&test, "bill_periods").await,
        5,
        "running again adds nothing"
    );
    test.app
        .bills
        .materialise_periods(&setup.ctx, day("2026-12-13"))
        .await
        .unwrap();
    assert_eq!(
        count(&test, "bill_periods").await,
        6,
        "a month later: one more"
    );
}

#[tokio::test]
async fn invalid_bills_are_rejected() {
    let (test, setup) = with_household().await;
    let cases = [
        (
            "repeat_freq",
            NewBill {
                repeat_freq: "daily".into(),
                ..rent("2026-01-01")
            },
        ),
        (
            "amount_max",
            NewBill {
                amount_max: "700".into(),
                ..rent("2026-01-01")
            },
        ),
        (
            "amount_min",
            NewBill {
                amount_min: "-1".into(),
                ..rent("2026-01-01")
            },
        ),
        (
            "end_date",
            NewBill {
                end_date: Some(day("2025-01-01")),
                ..rent("2026-01-01")
            },
        ),
        (
            "remind_every_days",
            NewBill {
                remind_every_days: Some(0),
                ..rent("2026-01-01")
            },
        ),
        (
            "currency",
            NewBill {
                currency: Some("XXX".into()),
                ..rent("2026-01-01")
            },
        ),
        (
            "name",
            NewBill {
                name: " ".into(),
                ..rent("2026-01-01")
            },
        ),
    ];
    for (field, input) in cases {
        let err = test
            .app
            .bills
            .create(&setup.ctx, Origin::Api, input)
            .await
            .unwrap_err();
        assert!(
            matches!(&err, AppError::Invalid { field: f, .. } if f == field),
            "{field}: {err:?}"
        );
    }
    test.app
        .bills
        .create(&setup.ctx, Origin::Api, rent("2026-01-01"))
        .await
        .unwrap();
    let duplicate = test
        .app
        .bills
        .create(&setup.ctx, Origin::Api, rent("2026-01-01"))
        .await;
    assert!(matches!(duplicate, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn a_transaction_with_the_bill_pays_the_nearest_period_and_deleting_it_reopens() {
    let (test, setup) = with_household().await;
    let bill = test
        .app
        .bills
        .create(&setup.ctx, Origin::Api, rent("2026-09-02"))
        .await
        .unwrap();
    let bill_id: BillId = bill.id.parse().unwrap();
    let checking = account(&test, &setup.ctx, "Checking", "asset", None).await;
    let landlord = account(&test, &setup.ctx, "Landlord", "expense", None).await;
    let mut payment = split(5, &checking, &landlord, "800");
    payment.bill_id = Some(bill_id);

    let group_view = test
        .app
        .transactions
        .create(&setup.ctx, Origin::Api, group(vec![payment.clone()]))
        .await
        .unwrap();

    assert_eq!(
        due_dates(&test, &setup.ctx, bill_id, "paid").await,
        ["2026-11-02"]
    );
    let paid = test.app.bills.periods(&setup.ctx, bill_id).await.unwrap();
    let november = paid.iter().find(|p| p.due_date == "2026-11-02").unwrap();
    assert_eq!(
        november.paid_split_id.as_ref(),
        Some(&group_view.splits[0].id)
    );
    let types: Vec<_> = events(&test, &setup.ctx)
        .await
        .into_iter()
        .map(|e| e.event_type)
        .collect();
    assert_eq!(
        types[types.len() - 2..],
        ["transaction.created", "bill.paid"]
    );

    // Replacing the group keeps the period paid, by the new split.
    let replaced = test
        .app
        .transactions
        .replace(
            &setup.ctx,
            Origin::Api,
            group_view.id.parse().unwrap(),
            group(vec![payment]),
        )
        .await
        .unwrap();
    let periods = test.app.bills.periods(&setup.ctx, bill_id).await.unwrap();
    let november = periods.iter().find(|p| p.due_date == "2026-11-02").unwrap();
    assert_eq!(
        (november.status.as_str(), november.paid_split_id.as_ref()),
        ("paid", Some(&replaced.splits[0].id))
    );

    let id: GroupId = replaced.id.parse().unwrap();
    test.app
        .transactions
        .delete(&setup.ctx, Origin::Api, id)
        .await
        .unwrap();
    assert!(
        due_dates(&test, &setup.ctx, bill_id, "paid")
            .await
            .is_empty()
    );
    assert_eq!(
        last_event(&test, &setup.ctx).await.event_type,
        "transaction.deleted"
    );
    let reopened = events(&test, &setup.ctx)
        .await
        .into_iter()
        .filter(|e| e.event_type == "bill.period_reopened")
        .count();
    assert_eq!(reopened, 2, "once on replace, once on delete");
}

#[tokio::test]
async fn bills_must_belong_to_the_household() {
    let (test, setup) = with_household().await;
    let other = other_household(&test).await;
    let foreign = test
        .app
        .bills
        .create(&other, Origin::Api, rent("2026-01-01"))
        .await
        .unwrap();
    let checking = account(&test, &setup.ctx, "Checking", "asset", None).await;
    let landlord = account(&test, &setup.ctx, "Landlord", "expense", None).await;
    let mut payment = split(5, &checking, &landlord, "800");
    payment.bill_id = Some(foreign.id.parse().unwrap());
    match test
        .app
        .transactions
        .create(&setup.ctx, Origin::Api, group(vec![payment]))
        .await
    {
        Err(AppError::Validation(errors)) => assert_eq!(errors[0].code(), "unknown_reference"),
        other => panic!("expected validation error, got {other:?}"),
    }
    let foreign_id: BillId = foreign.id.parse().unwrap();
    assert!(matches!(
        test.app.bills.get(&setup.ctx, foreign_id).await,
        Err(AppError::NotFound)
    ));
    assert!(matches!(
        test.app.bills.periods(&setup.ctx, foreign_id).await,
        Err(AppError::NotFound)
    ));
    let period: BillPeriodId = test.app.bills.periods(&other, foreign_id).await.unwrap()[0]
        .id
        .parse()
        .unwrap();
    assert!(matches!(
        test.app.bills.skip(&setup.ctx, Origin::Api, period).await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn periods_can_be_paid_skipped_and_reopened_by_hand() {
    let (test, setup) = with_household().await;
    let bill = test
        .app
        .bills
        .create(&setup.ctx, Origin::Api, rent("2026-11-02"))
        .await
        .unwrap();
    let bill_id: BillId = bill.id.parse().unwrap();
    let checking = account(&test, &setup.ctx, "Checking", "asset", None).await;
    let landlord = account(&test, &setup.ctx, "Landlord", "expense", None).await;
    let payment = test
        .app
        .transactions
        .create(
            &setup.ctx,
            Origin::Api,
            group(vec![split(3, &checking, &landlord, "800")]),
        )
        .await
        .unwrap();
    let split_id = payment.splits[0].id.parse().unwrap();
    let periods = test.app.bills.periods(&setup.ctx, bill_id).await.unwrap();
    let november: BillPeriodId = periods
        .iter()
        .find(|p| p.due_date == "2026-11-02")
        .unwrap()
        .id
        .parse()
        .unwrap();
    let december: BillPeriodId = periods
        .iter()
        .find(|p| p.due_date == "2026-12-02")
        .unwrap()
        .id
        .parse()
        .unwrap();

    let paid = test
        .app
        .bills
        .pay(&setup.ctx, Origin::Api, november, split_id)
        .await
        .unwrap();
    assert_eq!(
        (paid.status.as_str(), paid.paid_split_id.as_deref()),
        ("paid", Some(payment.splits[0].id.as_str()))
    );
    assert!(matches!(
        test.app
            .bills
            .pay(&setup.ctx, Origin::Api, november, split_id)
            .await,
        Err(AppError::Conflict(_))
    ));
    let missing = saldo_core::SplitId(uuid::Uuid::from_u128(9));
    assert!(matches!(
        test.app
            .bills
            .pay(&setup.ctx, Origin::Api, december, missing)
            .await,
        Err(AppError::Invalid { .. })
    ));

    let skipped = test
        .app
        .bills
        .skip(&setup.ctx, Origin::Api, december)
        .await
        .unwrap();
    assert_eq!(skipped.status, "skipped");
    assert_eq!(
        last_event(&test, &setup.ctx).await.event_type,
        "bill.period_skipped"
    );
    let reopened = test
        .app
        .bills
        .reopen(&setup.ctx, Origin::Api, november)
        .await
        .unwrap();
    assert_eq!(
        (reopened.status.as_str(), reopened.paid_split_id.clone()),
        ("open", None)
    );
    assert_eq!(
        last_event(&test, &setup.ctx).await.event_type,
        "bill.period_reopened"
    );
    assert!(matches!(
        test.app
            .bills
            .reopen(&setup.ctx, Origin::Api, november)
            .await,
        Err(AppError::Conflict(_))
    ));
}

#[tokio::test]
async fn schedule_changes_rebuild_future_periods_and_delete_clears_split_links() {
    let (test, setup) = with_household().await;
    let bill = test
        .app
        .bills
        .create(&setup.ctx, Origin::Api, rent("2026-09-02"))
        .await
        .unwrap();
    let id: BillId = bill.id.parse().unwrap();
    let update = BillUpdate {
        repeat_freq: Some("quarterly".into()),
        amount_max: Some("850".into()),
        name: Some("Flat".into()),
        ..Default::default()
    };
    let updated = test
        .app
        .bills
        .update(&setup.ctx, Origin::Api, id, update)
        .await
        .unwrap();
    assert_eq!(
        (
            updated.name.as_str(),
            updated.amount_max.as_str(),
            updated.repeat_freq.as_str()
        ),
        ("Flat", "850.00", "quarterly")
    );
    // Past periods stay; from today on, the quarterly schedule applies.
    assert_eq!(
        due_dates(&test, &setup.ctx, id, "open").await,
        ["2026-09-02", "2026-10-02", "2026-11-02", "2026-12-02"]
    );
    let bad = BillUpdate {
        amount_min: Some("900".into()),
        ..Default::default()
    };
    assert!(
        test.app
            .bills
            .update(&setup.ctx, Origin::Api, id, bad)
            .await
            .is_err()
    );

    let checking = account(&test, &setup.ctx, "Checking", "asset", None).await;
    let landlord = account(&test, &setup.ctx, "Landlord", "expense", None).await;
    let mut payment = split(1, &checking, &landlord, "800");
    payment.bill_id = Some(id);
    let created = test
        .app
        .transactions
        .create(&setup.ctx, Origin::Api, group(vec![payment]))
        .await
        .unwrap();
    test.app
        .bills
        .delete(&setup.ctx, Origin::Api, id)
        .await
        .unwrap();
    assert_eq!(
        last_event(&test, &setup.ctx).await.event_type,
        "bill.deleted"
    );
    let after = test
        .app
        .transactions
        .get(&setup.ctx, created.id.parse().unwrap())
        .await
        .unwrap();
    assert_eq!(after.splits[0].bill_id, None);
    assert_eq!(count(&test, "bill_periods").await, 0);
    let page = test
        .app
        .bills
        .list(&setup.ctx, saldo_app::page::PageRequest::default())
        .await
        .unwrap();
    assert_eq!(page.total, 0);
}

#[tokio::test]
async fn reminders_are_recorded_once_per_day() {
    let (test, setup) = with_household().await;
    test.app
        .bills
        .create(&setup.ctx, Origin::Api, rent("2026-11-15"))
        .await
        .unwrap();
    let today = day("2026-11-13");
    let open = test.app.bills.open_periods(&setup.ctx).await.unwrap();
    assert_eq!(
        open.len(),
        2,
        "2026-11-15 and 2026-12-15 fall within 60 days"
    );
    let first = &open[0];
    assert_eq!(first.period.due_date, day("2026-11-15"));
    assert_eq!(
        BillService::reminder_due(first, today),
        Some(ReminderKind::DueSoon)
    );

    test.app
        .bills
        .record_reminder(&setup.ctx, first.period_id, ReminderKind::DueSoon, today)
        .await
        .unwrap();
    test.app
        .bills
        .record_reminder(&setup.ctx, first.period_id, ReminderKind::DueSoon, today)
        .await
        .unwrap();

    let reminders: Vec<_> = events(&test, &setup.ctx)
        .await
        .into_iter()
        .filter(|e| e.event_type == "bill.due_soon")
        .collect();
    assert_eq!(
        reminders.len(),
        1,
        "a restart on the same day sends nothing new"
    );
    assert_eq!(reminders[0].origin, "scheduler");
    assert_eq!(
        reminders[0].payload["data"]["period"]["due_date"],
        "2026-11-15"
    );
    assert_eq!(reminders[0].payload["data"]["bill"]["name"], "Rent");
    let open = test.app.bills.open_periods(&setup.ctx).await.unwrap();
    assert_eq!(open[0].period.last_reminded_on, Some(today));
    assert_eq!(BillService::reminder_due(&open[0], today), None);
    assert_eq!(
        BillService::reminder_due(&open[0], day("2026-11-16")),
        Some(ReminderKind::Overdue)
    );
}
