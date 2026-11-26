#![allow(clippy::unwrap_used)] // test helpers outside #[test] functions

mod common;

use chrono::{NaiveDate, TimeDelta};
use common::*;
use saldo_app::accounts::{AccountUpdate, AccountView, NewAccount};
use saldo_app::credit_cards::{CardCycleView, PlanInput};
use saldo_app::ctx::{HouseholdCtx, Origin};
use saldo_app::error::AppError;
use saldo_app::transactions::{SplitInput, TransactionGroupView, input_from_view};
use saldo_core::card_cycle::Cycle;
use saldo_core::{GroupId, SplitId};

// The test clock starts on 13 November 2026, so plans start in November and
// November is paid on 15 December.

struct Fixture {
    test: TestApp,
    ctx: HouseholdCtx,
    card: AccountView,
    checking: AccountView,
    shop: AccountView,
    shop_refunds: AccountView,
}

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn month(y: i32, m: u32) -> Cycle {
    Cycle::new(y, m).unwrap()
}

async fn card_account(test: &TestApp, ctx: &HouseholdCtx, name: &str) -> AccountView {
    let input = NewAccount {
        name: name.to_string(),
        account_type: "asset".to_string(),
        asset_role: Some("credit_card".to_string()),
        ..NewAccount::default()
    };
    test.app
        .accounts
        .create(ctx, Origin::Api, input)
        .await
        .unwrap()
}

fn plan_input(checking: &AccountView, limit: Option<&str>, day: i64) -> PlanInput {
    PlanInput {
        monthly_limit: limit.map(str::to_string),
        settlement_account_id: account_id(checking),
        settlement_day: day,
    }
}

async fn fixture() -> Fixture {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let card = card_account(&test, &ctx, "Visa").await;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let shop = account(&test, &ctx, "Shop", "expense", None).await;
    let shop_refunds = account(&test, &ctx, "Shop", "revenue", None).await;
    test.app
        .credit_cards
        .put_plan(
            &ctx,
            Origin::Api,
            account_id(&card),
            plan_input(&checking, Some("500.00"), 15),
        )
        .await
        .unwrap();
    Fixture {
        test,
        ctx,
        card,
        checking,
        shop,
        shop_refunds,
    }
}

fn dated(
    on: NaiveDate,
    source: &AccountView,
    destination: &AccountView,
    amount: &str,
) -> SplitInput {
    SplitInput {
        date: on,
        ..split(1, source, destination, amount)
    }
}

async fn book(f: &Fixture, s: SplitInput) -> TransactionGroupView {
    f.test
        .app
        .transactions
        .create(&f.ctx, Origin::Api, group(vec![s]))
        .await
        .unwrap()
}

async fn buy(f: &Fixture, on: NaiveDate, amount: &str) -> TransactionGroupView {
    book(f, dated(on, &f.card, &f.shop, amount)).await
}

async fn sync(f: &Fixture) -> usize {
    let report = f
        .test
        .app
        .credit_cards
        .sync_settlements(&f.ctx)
        .await
        .unwrap();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    report.changes
}

async fn cycle(f: &Fixture, y: i32, m: u32) -> CardCycleView {
    let detail = f
        .test
        .app
        .credit_cards
        .cycle(&f.ctx, account_id(&f.card), month(y, m))
        .await;
    detail.unwrap().cycle
}

async fn settlement(f: &Fixture, y: i32, m: u32) -> Option<TransactionGroupView> {
    let id = cycle(f, y, m).await.settlement.group_id?;
    let group: GroupId = id.parse().unwrap();
    Some(f.test.app.transactions.get(&f.ctx, group).await.unwrap())
}

fn first_split(group: &TransactionGroupView) -> SplitId {
    group.splits[0].id.parse().unwrap()
}

async fn move_split(f: &Fixture, group: &TransactionGroupView) -> Result<(), AppError> {
    let cards = &f.test.app.credit_cards;
    cards
        .move_split(&f.ctx, Origin::Api, account_id(&f.card), first_split(group))
        .await
}

async fn unmove_split(f: &Fixture, group: &TransactionGroupView) -> Result<(), AppError> {
    let cards = &f.test.app.credit_cards;
    cards
        .unmove_split(&f.ctx, Origin::Api, account_id(&f.card), first_split(group))
        .await
}

/// Moves the clock to `day` of December 2026 (09:00 UTC).
fn december(f: &Fixture, day: i64) {
    f.test.clock.advance(TimeDelta::days(17 + day));
}

async fn replace_split(
    f: &Fixture,
    group: &TransactionGroupView,
    change: impl Fn(&mut SplitInput),
) {
    let mut input = input_from_view(group).unwrap();
    change(&mut input.splits[0]);
    let id: GroupId = group.id.parse().unwrap();
    f.test
        .app
        .transactions
        .replace(&f.ctx, Origin::Api, id, input)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_plan_needs_a_credit_card_paid_from_an_asset_account_in_its_currency() {
    let (test, setup) = with_household().await;
    let ctx = setup.ctx;
    let cards = &test.app.credit_cards;
    let card = card_account(&test, &ctx, "Visa").await;
    let plain = account(&test, &ctx, "Savings", "asset", None).await;
    let checking = account(&test, &ctx, "Checking", "asset", None).await;
    let dollars = account(&test, &ctx, "Dollars", "asset", Some("USD")).await;
    let shop = account(&test, &ctx, "Shop", "expense", None).await;
    let card_id = account_id(&card);
    let invalid = |r: Result<_, AppError>, wanted: &str| match r {
        Err(AppError::Invalid { field, .. }) => assert_eq!(field, wanted),
        other => panic!("expected 422 on {wanted}, got {other:?}"),
    };
    let put = |id, input| cards.put_plan(&ctx, Origin::Api, id, input);
    invalid(
        put(account_id(&plain), plan_input(&checking, Some("1.00"), 15)).await,
        "account_id",
    );
    invalid(
        put(card_id, plan_input(&dollars, Some("1.00"), 15)).await,
        "settlement_account_id",
    );
    invalid(
        put(card_id, plan_input(&card, Some("1.00"), 15)).await,
        "settlement_account_id",
    );
    invalid(
        put(card_id, plan_input(&shop, Some("1.00"), 15)).await,
        "settlement_account_id",
    );
    invalid(
        put(card_id, plan_input(&checking, Some("1.00"), 0)).await,
        "settlement_day",
    );
    invalid(
        put(card_id, plan_input(&checking, Some("1.00"), 32)).await,
        "settlement_day",
    );
    invalid(
        put(card_id, plan_input(&checking, None, 15)).await,
        "monthly_limit",
    );
    invalid(
        put(card_id, plan_input(&checking, Some("0.00"), 15)).await,
        "monthly_limit",
    );

    let (view, created) = put(card_id, plan_input(&checking, Some("1500.00"), 31))
        .await
        .unwrap();
    assert!(created);
    assert_eq!(view.starts_cycle, "2026-11");
    assert_eq!(view.settlement_day, 31);
    assert_eq!(view.limits.len(), 1);
    assert_eq!(view.limits[0].amount, "1500.00");
    assert_eq!(
        last_event(&test, &ctx).await.event_type,
        "credit_card_plan.created"
    );

    let (view, created) = put(card_id, plan_input(&checking, None, 10)).await.unwrap();
    assert!(!created);
    assert_eq!(view.settlement_day, 10);
    assert_eq!(view.limits[0].amount, "1500.00");
    assert_eq!(
        last_event(&test, &ctx).await.event_type,
        "credit_card_plan.updated"
    );
    assert_eq!(cards.get_plan(&ctx, card_id).await.unwrap(), view);
}

#[tokio::test]
async fn spending_counts_purchases_minus_refunds_and_ignores_transfers() {
    let f = fixture().await;
    buy(&f, date(3), "100.00").await;
    buy(&f, date(20), "50.00").await;
    book(&f, dated(date(21), &f.shop_refunds, &f.card, "20.00")).await;
    book(&f, dated(date(22), &f.checking, &f.card, "300.00")).await;
    buy(&f, ymd(2026, 12, 1), "999.00").await;

    let november = cycle(&f, 2026, 11).await;
    assert_eq!(november.limit, "500.00");
    assert_eq!(november.spent, "130.00");
    assert_eq!(november.available, "370.00");
    assert_eq!(november.settlement.state, "synced");
    assert_eq!(november.settlement.date, "2026-12-15");
    assert_eq!(november.difference_after_settlement, None);

    let detail = f
        .test
        .app
        .credit_cards
        .cycle(&f.ctx, account_id(&f.card), month(2026, 11));
    let splits = detail.await.unwrap().splits;
    let amounts: Vec<_> = splits.iter().map(|s| s.amount.as_str()).collect();
    assert_eq!(amounts, ["100.00", "50.00", "-20.00"]);
    assert_eq!(splits[0].counterparty.name, "Shop");
}

#[tokio::test]
async fn the_settlement_follows_the_month_until_its_date() {
    let f = fixture().await;
    assert_eq!(sync(&f).await, 0);
    assert!(settlement(&f, 2026, 11).await.is_none());

    let first = buy(&f, date(3), "100.00").await;
    assert_eq!(sync(&f).await, 1);
    let transfer = settlement(&f, 2026, 11).await.unwrap();
    let split = &transfer.splits[0];
    assert_eq!(split.kind, "transfer");
    assert_eq!(
        (split.amount.as_str(), split.date.as_str()),
        ("100.00", "2026-12-15")
    );
    assert_eq!(split.source_account_id, f.checking.id);
    assert_eq!(split.destination_account_id, f.card.id);
    assert_eq!(split.description, "Visa — November 2026");
    let event = last_event(&f.test, &f.ctx).await;
    assert_eq!(
        (event.event_type.as_str(), event.origin.as_str()),
        ("transaction.created", "scheduler")
    );

    let second = buy(&f, date(20), "50.00").await;
    assert_eq!(sync(&f).await, 1);
    let updated = settlement(&f, 2026, 11).await.unwrap();
    assert_eq!(updated.id, transfer.id);
    assert_eq!(updated.splits[0].id, split.id);
    assert_eq!(updated.splits[0].amount, "150.00");

    // Running again changes nothing and appends no events.
    let before = events(&f.test, &f.ctx).await.len();
    assert_eq!(sync(&f).await, 0);
    assert_eq!(events(&f.test, &f.ctx).await.len(), before);

    for group in [first, second] {
        let id: GroupId = group.id.parse().unwrap();
        f.test
            .app
            .transactions
            .delete(&f.ctx, Origin::Api, id)
            .await
            .unwrap();
    }
    assert_eq!(sync(&f).await, 1);
    assert!(settlement(&f, 2026, 11).await.is_none());
    assert_eq!(count(&f.test, "transaction_groups").await, 0);
}

#[tokio::test]
async fn a_change_of_paying_account_or_day_moves_the_open_transfer() {
    let f = fixture().await;
    buy(&f, date(3), "100.00").await;
    sync(&f).await;
    let other = account(&f.test, &f.ctx, "Joint", "asset", None).await;
    let cards = &f.test.app.credit_cards;
    cards
        .put_plan(
            &f.ctx,
            Origin::Api,
            account_id(&f.card),
            plan_input(&other, None, 31),
        )
        .await
        .unwrap();
    assert_eq!(sync(&f).await, 1);
    let split = &settlement(&f, 2026, 11).await.unwrap().splits[0];
    assert_eq!(split.source_account_id, other.id);
    assert_eq!(split.date, "2026-12-31");
}

#[tokio::test]
async fn moving_shifts_a_purchase_to_the_next_month_and_back() {
    let f = fixture().await;
    buy(&f, date(3), "100.00").await;
    let late = buy(&f, date(30), "50.00").await;
    move_split(&f, &late).await.unwrap();
    let event = last_event(&f.test, &f.ctx).await;
    assert_eq!(event.event_type, "credit_card_move.created");
    assert_eq!(event.payload["data"]["from_cycle"], "2026-11");
    assert_eq!(event.payload["data"]["to_cycle"], "2026-12");
    // Moving again is a no-op.
    move_split(&f, &late).await.unwrap();

    let november = cycle(&f, 2026, 11).await;
    assert_eq!(
        (november.spent.as_str(), november.available.as_str()),
        ("100.00", "400.00")
    );
    assert_eq!(november.moved_out, "50.00");
    let december_view = cycle(&f, 2026, 12).await;
    assert_eq!(
        (
            december_view.spent.as_str(),
            december_view.moved_in.as_str()
        ),
        ("50.00", "50.00")
    );
    sync(&f).await;
    assert_eq!(
        settlement(&f, 2026, 11).await.unwrap().splits[0].amount,
        "100.00"
    );

    let detail = f
        .test
        .app
        .credit_cards
        .cycle(&f.ctx, account_id(&f.card), month(2026, 12));
    let splits = detail.await.unwrap().splits;
    assert_eq!(splits.len(), 1);
    assert!(splits[0].moved_in && !splits[0].moved_out);

    unmove_split(&f, &late).await.unwrap();
    assert_eq!(
        last_event(&f.test, &f.ctx).await.event_type,
        "credit_card_move.deleted"
    );
    assert_eq!(cycle(&f, 2026, 11).await.spent, "150.00");
    assert_eq!(sync(&f).await, 1);
    assert_eq!(
        settlement(&f, 2026, 11).await.unwrap().splits[0].amount,
        "150.00"
    );
}

#[tokio::test]
async fn moves_are_refused_outside_open_months_and_for_other_splits() {
    let f = fixture().await;
    let october = buy(&f, ymd(2026, 10, 30), "10.00").await;
    assert!(matches!(
        move_split(&f, &october).await,
        Err(AppError::Conflict(_))
    ));
    let transfer = book(&f, dated(date(4), &f.checking, &f.card, "10.00")).await;
    assert!(matches!(
        move_split(&f, &transfer).await,
        Err(AppError::Invalid { .. })
    ));

    let november = buy(&f, date(28), "10.00").await;
    let moved = buy(&f, date(29), "10.00").await;
    move_split(&f, &moved).await.unwrap();
    december(&f, 15);
    assert!(matches!(
        move_split(&f, &november).await,
        Err(AppError::Conflict(_))
    ));
    assert!(matches!(
        unmove_split(&f, &moved).await,
        Err(AppError::Conflict(_))
    ));
    let in_december = buy(&f, ymd(2026, 12, 20), "10.00").await;
    move_split(&f, &in_december).await.unwrap();
}

#[tokio::test]
async fn the_settlement_date_freezes_the_month() {
    let f = fixture().await;
    buy(&f, date(3), "100.00").await;
    sync(&f).await;
    december(&f, 15);
    // Bought before the payment day but synced on it: a last update.
    buy(&f, date(25), "20.00").await;
    assert_eq!(sync(&f).await, 1);
    let november = cycle(&f, 2026, 11).await;
    assert_eq!(november.settlement.state, "frozen");
    assert_eq!(november.settlement.amount.as_deref(), Some("120.00"));
    assert_eq!(
        november.difference_after_settlement.as_deref(),
        Some("0.00")
    );

    buy(&f, date(26), "5.00").await;
    assert_eq!(sync(&f).await, 0);
    let november = cycle(&f, 2026, 11).await;
    assert_eq!(november.settlement.amount.as_deref(), Some("120.00"));
    assert_eq!(
        november.difference_after_settlement.as_deref(),
        Some("5.00")
    );
}

#[tokio::test]
async fn a_reconciled_transfer_freezes_the_month() {
    let f = fixture().await;
    buy(&f, date(3), "100.00").await;
    sync(&f).await;
    let transfer = settlement(&f, 2026, 11).await.unwrap();
    replace_split(&f, &transfer, |s| s.reconciled = true).await;
    buy(&f, date(4), "1.00").await;
    assert_eq!(sync(&f).await, 0);
    let november = cycle(&f, 2026, 11).await;
    assert_eq!(november.settlement.state, "frozen");
    assert_eq!(november.settlement.amount.as_deref(), Some("100.00"));
}

#[tokio::test]
async fn a_hand_edited_transfer_is_left_alone() {
    let f = fixture().await;
    buy(&f, date(3), "100.00").await;
    sync(&f).await;
    let transfer = settlement(&f, 2026, 11).await.unwrap();
    replace_split(&f, &transfer, |s| s.amount = "80.00".to_string()).await;
    buy(&f, date(4), "1.00").await;
    assert_eq!(sync(&f).await, 0);
    let november = cycle(&f, 2026, 11).await;
    assert_eq!(november.settlement.state, "detached");
    assert_eq!(november.settlement.amount.as_deref(), Some("80.00"));
    assert_eq!(
        november.difference_after_settlement.as_deref(),
        Some("21.00")
    );
}

#[tokio::test]
async fn a_hand_deleted_transfer_is_not_recreated() {
    let f = fixture().await;
    buy(&f, date(3), "100.00").await;
    sync(&f).await;
    let transfer = settlement(&f, 2026, 11).await.unwrap();
    let id: GroupId = transfer.id.parse().unwrap();
    f.test
        .app
        .transactions
        .delete(&f.ctx, Origin::Api, id)
        .await
        .unwrap();
    assert_eq!(sync(&f).await, 0);
    buy(&f, date(4), "1.00").await;
    assert_eq!(sync(&f).await, 0);
    let november = cycle(&f, 2026, 11).await;
    assert_eq!(november.settlement.state, "detached");
    assert_eq!(november.settlement.group_id, None);
    assert_eq!(
        november.difference_after_settlement.as_deref(),
        Some("101.00")
    );
}

#[tokio::test]
async fn a_move_whose_split_leaves_the_card_is_dropped() {
    let f = fixture().await;
    let late = buy(&f, date(30), "50.00").await;
    move_split(&f, &late).await.unwrap();
    let checking = f.checking.id.parse().unwrap();
    replace_split(&f, &late, |s| s.source_id = checking).await;
    assert_eq!(sync(&f).await, 1);
    assert_eq!(count(&f.test, "credit_card_moves").await, 0);
    let event = last_event(&f.test, &f.ctx).await;
    assert_eq!(
        (event.event_type.as_str(), event.origin.as_str()),
        ("credit_card_move.deleted", "scheduler")
    );
}

#[tokio::test]
async fn a_new_budget_applies_from_its_month_on() {
    let f = fixture().await;
    let cards = &f.test.app.credit_cards;
    let card = account_id(&f.card);
    let set = |from, amount| cards.set_limit(&f.ctx, Origin::Api, card, from, amount);
    assert!(matches!(
        set(month(2026, 10), "1.00").await,
        Err(AppError::Invalid { .. })
    ));
    set(month(2027, 1), "800.00").await.unwrap();
    set(month(2027, 1), "700.00").await.unwrap();
    let view = set(month(2026, 12), "600.00").await.unwrap();
    let limits: Vec<_> = view
        .limits
        .iter()
        .map(|l| (l.from_cycle.as_str(), l.amount.as_str()))
        .collect();
    assert_eq!(
        limits,
        [
            ("2026-11", "500.00"),
            ("2026-12", "600.00"),
            ("2027-01", "700.00")
        ]
    );

    let range = cards
        .cycles(&f.ctx, card, None, Some(month(2027, 2)))
        .await
        .unwrap();
    let budgets: Vec<_> = range
        .iter()
        .map(|c| (c.cycle.as_str(), c.limit.as_str()))
        .collect();
    assert_eq!(
        budgets,
        [
            ("2027-02", "700.00"),
            ("2027-01", "700.00"),
            ("2026-12", "600.00"),
            ("2026-11", "500.00")
        ]
    );

    // From December on, the current month's budget can still change, but
    // November's cannot.
    december(&f, 1);
    assert!(matches!(
        set(month(2026, 11), "1.00").await,
        Err(AppError::Invalid { .. })
    ));
    cards
        .put_plan(
            &f.ctx,
            Origin::Api,
            card,
            plan_input(&f.checking, Some("650.00"), 15),
        )
        .await
        .unwrap();
    assert_eq!(cycle(&f, 2026, 11).await.limit, "500.00");
    assert_eq!(cycle(&f, 2026, 12).await.limit, "650.00");
}

#[tokio::test]
async fn cycles_are_listed_newest_first_from_the_plan_start() {
    let f = fixture().await;
    let cards = &f.test.app.credit_cards;
    let card = account_id(&f.card);
    let all = cards.cycles(&f.ctx, card, None, None).await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].cycle, "2026-11");
    let none = cards
        .cycles(&f.ctx, card, Some(month(2026, 1)), Some(month(2026, 10)))
        .await;
    assert!(none.unwrap().is_empty());
    let too_many = cards
        .cycles(&f.ctx, card, Some(month(2020, 1)), Some(month(2026, 11)))
        .await;
    assert!(matches!(too_many, Err(AppError::Invalid { .. })));
    assert!(matches!(
        cards.cycle(&f.ctx, card, month(2026, 10)).await,
        Err(AppError::NotFound)
    ));

    buy(&f, date(3), "42.00").await;
    let summaries = cards.summaries(&f.ctx).await.unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].account_name, "Visa");
    assert_eq!(summaries[0].current.available, "458.00");
}

#[tokio::test]
async fn deleting_a_plan_removes_only_open_synced_transfers() {
    let f = fixture().await;
    buy(&f, date(3), "100.00").await;
    sync(&f).await;
    december(&f, 15);
    buy(&f, ymd(2026, 12, 16), "30.00").await;
    sync(&f).await;
    let booked = settlement(&f, 2026, 11).await.unwrap();
    let open = settlement(&f, 2026, 12).await.unwrap();
    assert_eq!(open.splits[0].date, "2027-01-15");

    let cards = &f.test.app.credit_cards;
    cards
        .delete_plan(&f.ctx, Origin::Api, account_id(&f.card))
        .await
        .unwrap();
    assert_eq!(
        last_event(&f.test, &f.ctx).await.event_type,
        "credit_card_plan.deleted"
    );
    let get = |id: &str| f.test.app.transactions.get(&f.ctx, id.parse().unwrap());
    assert!(get(&booked.id).await.is_ok());
    assert!(matches!(get(&open.id).await, Err(AppError::NotFound)));
    for table in [
        "credit_card_plans",
        "credit_card_limits",
        "credit_card_cycles",
    ] {
        assert_eq!(count(&f.test, table).await, 0, "{table}");
    }
    assert!(matches!(
        cards.get_plan(&f.ctx, account_id(&f.card)).await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn accounts_in_a_plan_keep_their_role() {
    let f = fixture().await;
    let accounts = &f.test.app.accounts;
    let update = |active, role| AccountUpdate {
        active,
        asset_role: role,
        ..AccountUpdate::default()
    };
    let card_change = accounts.update(
        &f.ctx,
        Origin::Api,
        account_id(&f.card),
        update(None, Some(None)),
    );
    assert!(matches!(card_change.await, Err(AppError::Conflict(_))));
    let archive = accounts.update(
        &f.ctx,
        Origin::Api,
        account_id(&f.checking),
        update(Some(false), None),
    );
    assert!(matches!(archive.await, Err(AppError::Conflict(_))));
    let delete = accounts.delete(&f.ctx, Origin::Api, account_id(&f.checking));
    assert!(matches!(delete.await, Err(AppError::Conflict(_))));
    accounts
        .update(&f.ctx, Origin::Api, account_id(&f.card), update(None, None))
        .await
        .unwrap();
}

#[tokio::test]
async fn another_household_cannot_see_or_change_a_plan() {
    let f = fixture().await;
    let late = buy(&f, date(30), "50.00").await;
    let other = other_household(&f.test).await;
    let cards = &f.test.app.credit_cards;
    let card = account_id(&f.card);
    assert!(matches!(
        cards.get_plan(&other, card).await,
        Err(AppError::NotFound)
    ));
    assert!(matches!(
        cards.cycles(&other, card, None, None).await,
        Err(AppError::NotFound)
    ));
    let moved = cards
        .move_split(&other, Origin::Api, card, first_split(&late))
        .await;
    assert!(matches!(moved, Err(AppError::NotFound)));
    let put = cards
        .put_plan(
            &other,
            Origin::Api,
            card,
            plan_input(&f.checking, Some("1.00"), 1),
        )
        .await;
    assert!(matches!(put, Err(AppError::NotFound)));
    assert!(matches!(
        cards.delete_plan(&other, Origin::Api, card).await,
        Err(AppError::NotFound)
    ));
    assert!(cards.summaries(&other).await.unwrap().is_empty());
    assert_eq!(cards.sync_settlements(&other).await.unwrap().changes, 0);
}

#[tokio::test]
async fn the_export_has_plans_budgets_and_moves() {
    let f = fixture().await;
    let late = buy(&f, date(30), "50.00").await;
    move_split(&f, &late).await.unwrap();
    let doc = f.test.app.export.json(&f.ctx).await.unwrap();
    assert_eq!(doc.credit_card_plans.len(), 1);
    assert_eq!(doc.credit_card_plans[0].limits[0].amount, "500.00");
    assert_eq!(doc.credit_card_moves.len(), 1);
    assert_eq!(doc.credit_card_moves[0].split_id, late.splits[0].id);

    let bytes = f.test.app.export.csv_zip(&f.ctx).await.unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut limits = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("credit_card_limits.csv").unwrap(),
        &mut limits,
    )
    .unwrap();
    assert_eq!(
        limits.lines().nth(1).unwrap(),
        format!("{},2026-11,500.00,EUR", f.card.id)
    );
}
