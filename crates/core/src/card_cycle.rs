//! Credit card months ("cycles"): which month a card transaction counts
//! toward, how much a month spent, what is still available, and what the
//! monthly settlement transfer should look like (Phase 12, ARCHITECTURE.md
//! 4.3 and 5.4f).
//!
//! Pure code: `today` is a parameter, and amounts are `i64` minor units in
//! the card's currency.

use std::fmt;
use std::str::FromStr;

use chrono::{Datelike, Months, NaiveDate};

use crate::error::MoneyError;
use crate::ids::AccountId;

/// A calendar month, `YYYY-MM`.
///
/// `derive(PartialOrd, Ord)` compares the fields in the order they are
/// declared (year, then month), so cycles sort in time order. It is the same
/// as PHP's `[$year, $month] <=> [$otherYear, $otherMonth]`, or a Go `Less`
/// method that compares the year first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Cycle {
    year: i32,
    month: u32,
}

/// `text` is not a `YYYY-MM` month.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not a month (YYYY-MM): {input:?}")]
pub struct CycleParseError {
    pub input: String,
}

impl Cycle {
    /// `None` unless `month` is 1 to 12 and the year is one chrono supports.
    pub fn new(year: i32, month: u32) -> Option<Self> {
        NaiveDate::from_ymd_opt(year, month, 1).map(|_| Self { year, month })
    }

    /// The month that contains `date`.
    pub fn of(date: NaiveDate) -> Self {
        Self {
            year: date.year(),
            month: date.month(),
        }
    }

    pub fn year(self) -> i32 {
        self.year
    }

    pub fn month(self) -> u32 {
        self.month
    }

    /// The following month (December rolls over to January).
    pub fn next(self) -> Self {
        if self.month == 12 {
            Self {
                year: self.year + 1,
                month: 1,
            }
        } else {
            Self {
                year: self.year,
                month: self.month + 1,
            }
        }
    }

    /// The month before (January rolls back to December).
    pub fn previous(self) -> Self {
        if self.month == 1 {
            Self {
                year: self.year - 1,
                month: 12,
            }
        } else {
            Self {
                year: self.year,
                month: self.month - 1,
            }
        }
    }

    /// The 1st of the month. `Cycle::new` checked that it exists.
    pub fn first_day(self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.year, self.month, 1).unwrap_or(NaiveDate::MIN)
    }

    /// The last day of the month.
    pub fn last_day(self) -> NaiveDate {
        self.first_day()
            .checked_add_months(Months::new(1))
            .and_then(|d| d.pred_opt())
            .unwrap_or(NaiveDate::MAX)
    }

    /// The day this month is paid: `day` (1 to 31) of the **following**
    /// month, or that month's last day when it is shorter.
    pub fn settlement_date(self, day: u8) -> NaiveDate {
        let next = self.next();
        let last = next.last_day();
        let wanted = u32::from(day).clamp(1, last.day());
        last.with_day(wanted).unwrap_or(last)
    }
}

impl fmt::Display for Cycle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}", self.year, self.month)
    }
}

/// Parsing `"2026-11"`: `"2026-11".parse::<Cycle>()`, like
/// `Carbon::createFromFormat('Y-m', ...)` or a Go `ParseCycle` function.
impl FromStr for Cycle {
    type Err = CycleParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let error = || CycleParseError {
            input: text.to_string(),
        };
        let (year, month) = text.split_once('-').ok_or_else(error)?;
        if year.len() != 4 || month.len() != 2 {
            return Err(error());
        }
        let year: i32 = year.parse().map_err(|_| error())?;
        let month: u32 = month.parse().map_err(|_| error())?;
        Cycle::new(year, month).ok_or_else(error)
    }
}

/// The month a card transaction counts toward: its own, or the next one
/// when the owner moved it.
pub fn effective_cycle(date: NaiveDate, moved: bool) -> Cycle {
    let own = Cycle::of(date);
    if moved { own.next() } else { own }
}

/// One split that counts toward a card: `amount` is positive for a purchase
/// (a withdrawal from the card) and negative for a refund (a deposit to it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardEntry {
    pub date: NaiveDate,
    pub amount: i64,
    pub moved: bool,
}

/// What `cycle` spent: the sum of the entries that count toward it. May be
/// negative when refunds exceed purchases.
pub fn cycle_spending(entries: &[CardEntry], cycle: Cycle) -> Result<i64, MoneyError> {
    entries
        .iter()
        .filter(|e| effective_cycle(e.date, e.moved) == cycle)
        .try_fold(0_i64, |total, e| {
            total.checked_add(e.amount).ok_or(MoneyError::Overflow)
        })
}

/// What is still available: `limit - spent` (negative when over budget).
pub fn available(limit: i64, spent: i64) -> Result<i64, MoneyError> {
    limit.checked_sub(spent).ok_or(MoneyError::Overflow)
}

/// A month's budget: the amount of the latest entry that starts on or before
/// `cycle`; `None` before the first entry. `limits` may be in any order.
pub fn limit_for(limits: &[(Cycle, i64)], cycle: Cycle) -> Option<i64> {
    limits
        .iter()
        .filter(|(from, _)| *from <= cycle)
        .max_by_key(|(from, _)| *from)
        .map(|(_, amount)| *amount)
}

/// Who manages a month's settlement transfer (decision 9 of the Phase 12
/// brief).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettlementState {
    /// The server keeps the transfer equal to the month's spending.
    Synced,
    /// Booked: never changed again.
    Frozen,
    /// Edited or deleted by hand: the server leaves it alone.
    Detached,
}

impl SettlementState {
    pub fn as_str(self) -> &'static str {
        match self {
            SettlementState::Synced => "synced",
            SettlementState::Frozen => "frozen",
            SettlementState::Detached => "detached",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "synced" => Some(SettlementState::Synced),
            "frozen" => Some(SettlementState::Frozen),
            "detached" => Some(SettlementState::Detached),
            _ => None,
        }
    }
}

/// The parts of a settlement transfer the server decides: from `source`
/// (the paying account) to the card, `amount` on `date`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transfer {
    pub amount: i64,
    pub date: NaiveDate,
    pub source: AccountId,
}

/// What the database holds for the transfer the server last wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stored {
    /// The group is gone.
    Missing,
    /// One split from `source` to the card.
    Transfer(Transfer),
    /// The group no longer looks like a settlement (more splits, another
    /// destination or currency).
    Other,
}

/// Everything [`settlement_step`] needs about one card month.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettlementInput {
    pub state: SettlementState,
    /// What the server last wrote; `None` when there is no transfer.
    pub written: Option<Transfer>,
    /// What the database holds for `written`'s group (ignored without one).
    pub stored: Stored,
    /// Whether that transfer's split is reconciled.
    pub reconciled: bool,
    pub spent: i64,
    pub paying_account: AccountId,
    pub settlement_date: NaiveDate,
    pub today: NaiveDate,
}

/// A change to the settlement transfer. An enum whose variants carry data:
/// like a Go interface with one struct per case and a type switch, except
/// that `match` refuses to compile when a case is forgotten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    None,
    Create(Transfer),
    Update(Transfer),
    Delete,
}

/// What to do with a month: change the transfer, then store `next_state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettlementStep {
    pub change: Change,
    pub next_state: SettlementState,
}

/// The settlement decision table (decision 9):
///
/// 1. A frozen or detached month is left alone.
/// 2. A transfer that differs from what the server wrote (edited, deleted,
///    reshaped) detaches the month.
/// 3. A reconciled transfer freezes the month as it is.
/// 4. Otherwise the transfer is made to match the spending (none when the
///    month spent nothing or less), and the month freezes from its
///    settlement date on, after that last update.
pub fn settlement_step(input: SettlementInput) -> SettlementStep {
    let keep = |state| SettlementStep {
        change: Change::None,
        next_state: state,
    };
    if input.state != SettlementState::Synced {
        return keep(input.state);
    }
    if let Some(written) = input.written {
        if input.stored != Stored::Transfer(written) {
            return keep(SettlementState::Detached);
        }
        if input.reconciled {
            return keep(SettlementState::Frozen);
        }
    }
    let wanted = (input.spent > 0).then_some(Transfer {
        amount: input.spent,
        date: input.settlement_date,
        source: input.paying_account,
    });
    let change = match (input.written, wanted) {
        (None, None) => Change::None,
        (None, Some(t)) => Change::Create(t),
        (Some(_), None) => Change::Delete,
        (Some(w), Some(t)) if w == t => Change::None,
        (Some(_), Some(t)) => Change::Update(t),
    };
    let next_state = if input.today >= input.settlement_date {
        SettlementState::Frozen
    } else {
        SettlementState::Synced
    };
    SettlementStep { change, next_state }
}

/// Whether a month no longer accepts moves: frozen, or its settlement date
/// has come (a synced month freezes at the next run).
pub fn is_closed(state: SettlementState, settlement_date: NaiveDate, today: NaiveDate) -> bool {
    state == SettlementState::Frozen || today >= settlement_date
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use uuid::Uuid;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).expect("valid date")
    }

    fn c(y: i32, m: u32) -> Cycle {
        Cycle::new(y, m).expect("valid month")
    }

    fn bank() -> AccountId {
        AccountId(Uuid::from_u128(1))
    }

    fn other_bank() -> AccountId {
        AccountId(Uuid::from_u128(2))
    }

    fn transfer(amount: i64) -> Transfer {
        Transfer {
            amount,
            date: d(2026, 12, 15),
            source: bank(),
        }
    }

    fn input(written: Option<Transfer>, spent: i64, today: NaiveDate) -> SettlementInput {
        SettlementInput {
            state: SettlementState::Synced,
            written,
            stored: written.map_or(Stored::Missing, Stored::Transfer),
            reconciled: false,
            spent,
            paying_account: bank(),
            settlement_date: d(2026, 12, 15),
            today,
        }
    }

    fn step(change: Change, next_state: SettlementState) -> SettlementStep {
        SettlementStep { change, next_state }
    }

    /// Before November's settlement date (15 December).
    fn before() -> NaiveDate {
        d(2026, 11, 26)
    }

    /// November's settlement date.
    fn on() -> NaiveDate {
        d(2026, 12, 15)
    }

    #[test]
    fn new_rejects_invalid_months() {
        assert!(Cycle::new(2026, 0).is_none());
        assert!(Cycle::new(2026, 13).is_none());
        assert!(Cycle::new(2026, 12).is_some());
    }

    #[test]
    fn next_and_previous_roll_over_the_year() {
        assert_eq!(c(2026, 12).next(), c(2027, 1));
        assert_eq!(c(2026, 5).next(), c(2026, 6));
        assert_eq!(c(2027, 1).previous(), c(2026, 12));
        assert_eq!(c(2026, 6).previous(), c(2026, 5));
    }

    #[test]
    fn cycles_sort_in_time_order() {
        assert!(c(2025, 12) < c(2026, 1));
        assert!(c(2026, 2) < c(2026, 10));
    }

    #[test]
    fn first_and_last_days() {
        assert_eq!(c(2026, 2).first_day(), d(2026, 2, 1));
        assert_eq!(c(2026, 2).last_day(), d(2026, 2, 28));
        assert_eq!(c(2028, 2).last_day(), d(2028, 2, 29));
        assert_eq!(c(2026, 12).last_day(), d(2026, 12, 31));
        assert_eq!(c(2026, 4).last_day(), d(2026, 4, 30));
    }

    #[test]
    fn settlement_date_is_in_the_following_month() {
        assert_eq!(c(2026, 11).settlement_date(15), d(2026, 12, 15));
        assert_eq!(c(2026, 12).settlement_date(1), d(2027, 1, 1));
    }

    #[test]
    fn settlement_date_clamps_to_a_shorter_month() {
        assert_eq!(c(2026, 1).settlement_date(31), d(2026, 2, 28));
        assert_eq!(c(2028, 1).settlement_date(30), d(2028, 2, 29));
        assert_eq!(c(2026, 3).settlement_date(31), d(2026, 4, 30));
        assert_eq!(c(2026, 4).settlement_date(31), d(2026, 5, 31));
    }

    #[test]
    fn settlement_date_clamps_day_zero_to_the_first() {
        assert_eq!(c(2026, 4).settlement_date(0), d(2026, 5, 1));
    }

    #[test]
    fn parses_and_prints_months() {
        assert_eq!("2026-11".parse::<Cycle>(), Ok(c(2026, 11)));
        assert_eq!(c(2026, 3).to_string(), "2026-03");
        for bad in [
            "2026-13", "2026-1", "26-11", "2026/11", "abcd-ef", "", "2026-00",
        ] {
            assert!(bad.parse::<Cycle>().is_err(), "{bad}");
        }
    }

    #[test]
    fn effective_cycle_moves_to_the_next_month() {
        assert_eq!(effective_cycle(d(2026, 11, 30), false), c(2026, 11));
        assert_eq!(effective_cycle(d(2026, 12, 31), true), c(2027, 1));
    }

    #[test]
    fn spending_counts_purchases_minus_refunds_in_the_month() {
        let entries = [
            CardEntry {
                date: d(2026, 11, 3),
                amount: 10_000,
                moved: false,
            },
            CardEntry {
                date: d(2026, 11, 9),
                amount: -2_500,
                moved: false,
            },
            CardEntry {
                date: d(2026, 11, 30),
                amount: 4_000,
                moved: true,
            },
            CardEntry {
                date: d(2026, 10, 31),
                amount: 700,
                moved: true,
            },
            CardEntry {
                date: d(2026, 12, 1),
                amount: 9_999,
                moved: false,
            },
        ];
        assert_eq!(cycle_spending(&entries, c(2026, 11)), Ok(8_200));
        assert_eq!(cycle_spending(&entries, c(2026, 12)), Ok(13_999));
        assert_eq!(cycle_spending(&[], c(2026, 12)), Ok(0));
    }

    #[test]
    fn spending_reports_overflow() {
        let entries = [
            CardEntry {
                date: d(2026, 11, 3),
                amount: i64::MAX,
                moved: false,
            },
            CardEntry {
                date: d(2026, 11, 4),
                amount: 1,
                moved: false,
            },
        ];
        assert_eq!(
            cycle_spending(&entries, c(2026, 11)),
            Err(MoneyError::Overflow)
        );
    }

    #[test]
    fn available_is_limit_minus_spent() {
        assert_eq!(available(150_000, 81_240), Ok(68_760));
        assert_eq!(available(10_000, 12_000), Ok(-2_000));
        assert_eq!(available(10_000, -500), Ok(10_500));
        assert_eq!(available(i64::MIN, 1), Err(MoneyError::Overflow));
    }

    #[test]
    fn limit_for_takes_the_latest_entry_on_or_before_the_month() {
        let limits = [
            (c(2026, 12), 180_000),
            (c(2026, 9), 150_000),
            (c(2027, 3), 120_000),
        ];
        assert_eq!(limit_for(&limits, c(2026, 8)), None);
        assert_eq!(limit_for(&limits, c(2026, 9)), Some(150_000));
        assert_eq!(limit_for(&limits, c(2026, 11)), Some(150_000));
        assert_eq!(limit_for(&limits, c(2026, 12)), Some(180_000));
        assert_eq!(limit_for(&limits, c(2027, 2)), Some(180_000));
        assert_eq!(limit_for(&limits, c(2030, 1)), Some(120_000));
        assert_eq!(limit_for(&[], c(2030, 1)), None);
    }

    #[test]
    fn state_names_round_trip() {
        for state in [
            SettlementState::Synced,
            SettlementState::Frozen,
            SettlementState::Detached,
        ] {
            assert_eq!(SettlementState::parse(state.as_str()), Some(state));
        }
        assert_eq!(SettlementState::parse("open"), None);
    }

    #[test]
    fn frozen_and_detached_months_are_left_alone() {
        for state in [SettlementState::Frozen, SettlementState::Detached] {
            let mut i = input(Some(transfer(100)), 500, before());
            i.state = state;
            i.stored = Stored::Missing;
            assert_eq!(settlement_step(i), step(Change::None, state));
        }
    }

    #[test]
    fn first_spending_creates_the_transfer() {
        assert_eq!(
            settlement_step(input(None, 4_200, before())),
            step(Change::Create(transfer(4_200)), SettlementState::Synced)
        );
    }

    #[test]
    fn no_spending_and_no_transfer_does_nothing() {
        assert_eq!(
            settlement_step(input(None, 0, before())),
            step(Change::None, SettlementState::Synced)
        );
        assert_eq!(
            settlement_step(input(None, -300, before())),
            step(Change::None, SettlementState::Synced)
        );
    }

    #[test]
    fn new_spending_updates_the_transfer() {
        assert_eq!(
            settlement_step(input(Some(transfer(4_200)), 5_000, before())),
            step(Change::Update(transfer(5_000)), SettlementState::Synced)
        );
    }

    #[test]
    fn a_new_paying_account_updates_the_transfer() {
        let mut i = input(Some(transfer(4_200)), 4_200, before());
        i.paying_account = other_bank();
        let expected = Transfer {
            source: other_bank(),
            ..transfer(4_200)
        };
        assert_eq!(
            settlement_step(i),
            step(Change::Update(expected), SettlementState::Synced)
        );
    }

    #[test]
    fn a_matching_transfer_is_unchanged() {
        assert_eq!(
            settlement_step(input(Some(transfer(4_200)), 4_200, before())),
            step(Change::None, SettlementState::Synced)
        );
    }

    #[test]
    fn spending_back_to_zero_deletes_the_transfer() {
        assert_eq!(
            settlement_step(input(Some(transfer(4_200)), 0, before())),
            step(Change::Delete, SettlementState::Synced)
        );
    }

    #[test]
    fn the_settlement_date_freezes_after_a_last_update() {
        assert_eq!(
            settlement_step(input(Some(transfer(4_200)), 4_300, on())),
            step(Change::Update(transfer(4_300)), SettlementState::Frozen)
        );
        assert_eq!(
            settlement_step(input(Some(transfer(4_200)), 4_200, d(2027, 1, 2))),
            step(Change::None, SettlementState::Frozen)
        );
        assert_eq!(
            settlement_step(input(None, 0, on())),
            step(Change::None, SettlementState::Frozen)
        );
    }

    #[test]
    fn a_reconciled_transfer_freezes_as_it_is() {
        let mut i = input(Some(transfer(4_200)), 9_000, before());
        i.reconciled = true;
        assert_eq!(
            settlement_step(i),
            step(Change::None, SettlementState::Frozen)
        );
    }

    #[test]
    fn a_hand_edit_or_delete_detaches_the_month() {
        for stored in [
            Stored::Missing,
            Stored::Other,
            Stored::Transfer(transfer(1)),
            Stored::Transfer(Transfer {
                date: d(2026, 12, 20),
                ..transfer(4_200)
            }),
        ] {
            let mut i = input(Some(transfer(4_200)), 5_000, before());
            i.stored = stored;
            i.reconciled = true;
            assert_eq!(
                settlement_step(i),
                step(Change::None, SettlementState::Detached)
            );
        }
    }

    #[test]
    fn is_closed_from_the_settlement_date_or_when_frozen() {
        let date = d(2026, 12, 15);
        assert!(!is_closed(SettlementState::Synced, date, d(2026, 12, 14)));
        assert!(is_closed(SettlementState::Synced, date, date));
        assert!(is_closed(SettlementState::Frozen, date, d(2026, 11, 1)));
        assert!(!is_closed(SettlementState::Detached, date, d(2026, 12, 14)));
    }

    fn entries() -> impl Strategy<Value = Vec<CardEntry>> {
        proptest::collection::vec(
            (0_i64..730, -100_000_i64..100_000, any::<bool>()).prop_map(
                |(offset, amount, moved)| CardEntry {
                    date: d(2025, 1, 1) + chrono::Days::new(offset.unsigned_abs()),
                    amount,
                    moved,
                },
            ),
            0..40,
        )
    }

    proptest! {
        #[test]
        fn spending_over_all_months_sums_every_entry(entries in entries()) {
            let mut cycle = c(2025, 1);
            let mut total = 0_i64;
            while cycle <= c(2027, 1) {
                total += cycle_spending(&entries, cycle).expect("no overflow");
                cycle = cycle.next();
            }
            prop_assert_eq!(total, entries.iter().map(|e| e.amount).sum::<i64>());
        }

        #[test]
        fn moving_shifts_exactly_its_amount(entries in entries(), pick in any::<prop::sample::Index>()) {
            prop_assume!(!entries.is_empty());
            let i = pick.index(entries.len());
            let mut moved = entries.clone();
            moved[i].moved = !moved[i].moved;
            let own = Cycle::of(entries[i].date);
            let next = own.next();
            let before = (cycle_spending(&entries, own).expect("ok"), cycle_spending(&entries, next).expect("ok"));
            let after = (cycle_spending(&moved, own).expect("ok"), cycle_spending(&moved, next).expect("ok"));
            let sign = if entries[i].moved { -1 } else { 1 };
            prop_assert_eq!(after.0, before.0 - sign * entries[i].amount);
            prop_assert_eq!(after.1, before.1 + sign * entries[i].amount);
        }

        #[test]
        fn settlement_date_is_always_in_the_next_month(year in 2000_i32..2100, month in 1_u32..=12, day in 1_u8..=31) {
            let cycle = c(year, month);
            prop_assert_eq!(Cycle::of(cycle.settlement_date(day)), cycle.next());
        }
    }
}
