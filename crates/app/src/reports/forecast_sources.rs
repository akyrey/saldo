//! Where the forecast's planned amounts come from: the accounts it covers,
//! their balances today, transactions already entered with a later date,
//! recurrences that will fire and bills that will fall due.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use sqlx::SqliteConnection;

use crate::bills::upcoming::UpcomingBill;
use crate::ctx::HouseholdCtx;
use crate::db::json_list;
use crate::error::{AppError, parse_stored};
use crate::recurrences::upcoming::{UpcomingRecurrence, UpcomingSplit};
use crate::scope::Scope;

/// One planned amount: positive when money comes in, negative when it goes
/// out. `account_id` is `None` for a bill nobody has paid yet (it counts in
/// the total only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub date: NaiveDate,
    /// `transaction`, `recurrence` or `bill`.
    pub source: &'static str,
    pub source_id: String,
    pub title: String,
    pub account_id: Option<String>,
    pub currency: String,
    pub amount_minor: i64,
}

/// Account ID → currency code of the accounts the forecast covers.
pub(crate) type Accounts = HashMap<String, String>;

/// Asset and liability accounts counted in net worth, by owner (`scope`), or
/// the one `account_id` (which must be an asset or liability account).
pub(crate) async fn accounts(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    scope: Scope,
    account_id: Option<&str>,
) -> Result<Accounts, AppError> {
    let (household_id, owner) = (ctx.household_id.to_string(), scope.owner(ctx));
    let rows = sqlx::query!(
        r#"SELECT id AS "id!", currency_code FROM accounts
           WHERE household_id = ?1 AND type IN ('asset', 'liability')
             AND (?2 IS NULL OR owner_user_id = ?2)
             AND (CASE WHEN ?3 IS NULL THEN include_net_worth = 1 ELSE id = ?3 END)"#,
        household_id,
        owner,
        account_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    if account_id.is_some() && rows.is_empty() {
        return Err(AppError::invalid(
            "account_id",
            "must be an asset or liability account in the scope",
        ));
    }
    Ok(rows.into_iter().map(|r| (r.id, r.currency_code)).collect())
}

/// Per currency: the accounts' balance at the end of `today`.
pub(crate) async fn start_balances(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    accounts: &Accounts,
    today: NaiveDate,
) -> Result<HashMap<String, i64>, AppError> {
    let household_id = ctx.household_id.to_string();
    let ids = json_list(&accounts.keys().cloned().collect::<Vec<_>>())?;
    let today = today.to_string();
    // Same sign rule as net worth: + when the account receives, - when it
    // pays, in the account's currency.
    let rows = sqlx::query!(
        r#"SELECT a.currency_code AS "currency!",
                  SUM(CASE WHEN s.destination_account_id = a.id THEN 1 ELSE -1 END
                      * CASE WHEN s.foreign_currency_code = a.currency_code
                             THEN s.foreign_amount_minor ELSE s.amount_minor END) AS "total!: i64"
           FROM splits s
           JOIN accounts a ON a.household_id = s.household_id
                          AND a.id IN (s.source_account_id, s.destination_account_id)
           WHERE s.household_id = ?1 AND s.date <= ?3
             AND a.id IN (SELECT value FROM json_each(?2))
           GROUP BY a.currency_code"#,
        household_id,
        ids,
        today,
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut balances: HashMap<String, i64> = accounts.values().map(|c| (c.clone(), 0)).collect();
    for row in rows {
        balances.insert(row.currency, row.total);
    }
    Ok(balances)
}

/// Each side of a split dated after `today` (up to `to`) on a covered account.
pub(crate) async fn future_splits(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    accounts: &Accounts,
    today: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<Entry>, AppError> {
    let household_id = ctx.household_id.to_string();
    let ids = json_list(&accounts.keys().cloned().collect::<Vec<_>>())?;
    let (today, to) = (today.to_string(), to.to_string());
    let rows = sqlx::query!(
        r#"SELECT s.group_id AS "group_id!", s.date AS "date!", s.description AS "description!",
                  a.id AS "account_id!", a.currency_code AS "currency!",
                  CASE WHEN s.destination_account_id = a.id THEN 1 ELSE -1 END
                      * CASE WHEN s.foreign_currency_code = a.currency_code
                             THEN s.foreign_amount_minor ELSE s.amount_minor END AS "amount!: i64"
           FROM splits s
           JOIN accounts a ON a.household_id = s.household_id
                          AND a.id IN (s.source_account_id, s.destination_account_id)
           WHERE s.household_id = ?1 AND s.date > ?3 AND s.date <= ?4
             AND a.id IN (SELECT value FROM json_each(?2))
           ORDER BY s.date, s.group_id, s.position"#,
        household_id,
        ids,
        today,
        to,
    )
    .fetch_all(&mut *conn)
    .await?;
    rows.into_iter()
        .map(|r| {
            Ok(Entry {
                date: parse_stored(&r.date)?,
                source: "transaction",
                source_id: r.group_id,
                title: r.description,
                account_id: Some(r.account_id),
                currency: r.currency,
                amount_minor: r.amount,
            })
        })
        .collect()
}

/// What each firing of the recurrences books on covered accounts. Dates not
/// fired yet that are already past count tomorrow (the runner is due).
pub(crate) fn recurrence_entries(
    recurrences: &[UpcomingRecurrence],
    accounts: &Accounts,
    tomorrow: NaiveDate,
) -> Vec<Entry> {
    let mut entries = Vec::new();
    for recurrence in recurrences {
        for date in &recurrence.dates {
            for split in &recurrence.splits {
                for (account_id, amount_minor) in sides(split, accounts) {
                    entries.push(Entry {
                        date: (*date).max(tomorrow),
                        source: "recurrence",
                        source_id: recurrence.id.clone(),
                        title: recurrence.title.clone(),
                        currency: accounts.get(&account_id).cloned().unwrap_or_default(),
                        account_id: Some(account_id),
                        amount_minor,
                    });
                }
            }
        }
    }
    entries
}

/// The covered accounts a template split moves money on, with the signed
/// amount in each account's currency (skipped when neither amount is in it).
fn sides(split: &UpcomingSplit, accounts: &Accounts) -> Vec<(String, i64)> {
    let in_currency = |currency: &str| {
        if split.currency_code == currency {
            Some(split.amount_minor)
        } else if split.foreign_currency_code.as_deref() == Some(currency) {
            split.foreign_amount_minor
        } else {
            None
        }
    };
    let mut sides = Vec::new();
    for (account_id, sign) in [
        (&split.source_account_id, -1),
        (&split.destination_account_id, 1),
    ] {
        let amount = accounts.get(account_id).and_then(|c| in_currency(c));
        if let Some(amount) = amount {
            sides.push((account_id.clone(), sign * amount));
        }
    }
    sides
}

/// Bills at their highest amount, paid from the account that paid them last.
/// A bill that a recurrence pays is left out (the recurrence counts it). A
/// bill nobody paid yet counts in the total only, and only when `whole` (the
/// forecast covers the whole household, not one member or one account).
pub(crate) fn bill_entries(
    bills: &[UpcomingBill],
    recurrences: &[UpcomingRecurrence],
    accounts: &Accounts,
    tomorrow: NaiveDate,
    whole: bool,
) -> Vec<Entry> {
    let paid_by_recurrence: HashSet<&str> = recurrences
        .iter()
        .flat_map(|r| &r.splits)
        .filter_map(|s| s.bill_id.as_deref())
        .collect();
    let mut entries = Vec::new();
    for bill in bills
        .iter()
        .filter(|b| !paid_by_recurrence.contains(b.id.as_str()))
    {
        let account_id = match &bill.payer_account_id {
            Some(payer) if accounts.get(payer) == Some(&bill.currency_code) => Some(payer.clone()),
            Some(_) => continue,
            None if whole => None,
            None => continue,
        };
        for date in &bill.dates {
            entries.push(Entry {
                date: (*date).max(tomorrow),
                source: "bill",
                source_id: bill.id.clone(),
                title: bill.name.clone(),
                account_id: account_id.clone(),
                currency: bill.currency_code.clone(),
                amount_minor: -bill.amount_max_minor,
            });
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, day).unwrap()
    }

    fn accounts() -> Accounts {
        [("checking", "EUR"), ("savings", "EUR"), ("usd", "USD")]
            .into_iter()
            .map(|(a, c)| (a.to_string(), c.to_string()))
            .collect()
    }

    fn split(source: &str, destination: &str, amount: i64, bill: Option<&str>) -> UpcomingSplit {
        UpcomingSplit {
            source_account_id: source.into(),
            destination_account_id: destination.into(),
            amount_minor: amount,
            currency_code: "EUR".into(),
            foreign_amount_minor: None,
            foreign_currency_code: None,
            bill_id: bill.map(str::to_string),
        }
    }

    fn recurrence(dates: Vec<NaiveDate>, splits: Vec<UpcomingSplit>) -> UpcomingRecurrence {
        UpcomingRecurrence {
            id: "r".into(),
            title: "Rent".into(),
            dates,
            splits,
        }
    }

    fn bill(id: &str, payer: Option<&str>) -> UpcomingBill {
        UpcomingBill {
            id: id.into(),
            name: format!("bill {id}"),
            amount_max_minor: 1_500,
            currency_code: "EUR".into(),
            payer_account_id: payer.map(str::to_string),
            dates: vec![d(11, 10), d(12, 10)],
        }
    }

    #[test]
    fn recurrences_move_covered_accounts_in_their_currency() {
        let mut foreign = split("usd", "shop", 900, None);
        foreign.foreign_amount_minor = Some(1_000);
        foreign.foreign_currency_code = Some("USD".into());
        let recurrences = [recurrence(
            vec![d(11, 10), d(12, 1)],
            vec![
                split("checking", "landlord", 80_000, None),
                split("checking", "savings", 5_000, None), // both covered
                foreign,
            ],
        )];
        let entries = recurrence_entries(&recurrences, &accounts(), d(11, 18));
        let summary: Vec<_> = entries
            .iter()
            .map(|e| (e.date, e.account_id.clone().unwrap(), e.amount_minor))
            .collect();
        let late = d(11, 18); // 10 November was not fired yet: tomorrow
        assert_eq!(
            summary,
            vec![
                (late, "checking".into(), -80_000),
                (late, "checking".into(), -5_000),
                (late, "savings".into(), 5_000),
                (late, "usd".into(), -1_000),
                (d(12, 1), "checking".into(), -80_000),
                (d(12, 1), "checking".into(), -5_000),
                (d(12, 1), "savings".into(), 5_000),
                (d(12, 1), "usd".into(), -1_000),
            ]
        );
        assert_eq!(entries[3].currency, "USD");
    }

    #[test]
    fn bills_use_the_last_payer_and_skip_those_a_recurrence_pays() {
        let recurrences = [recurrence(
            vec![d(12, 1)],
            vec![split("checking", "gym", 1_500, Some("gym"))],
        )];
        let bills = [
            bill("rent", Some("checking")),
            bill("gym", Some("checking")), // paid by the recurrence
            bill("new", None),             // nobody paid it yet
            bill("theirs", Some("partner-account")),
        ];
        let whole = bill_entries(&bills, &recurrences, &accounts(), d(11, 18), true);
        let summary: Vec<_> = whole
            .iter()
            .map(|e| (e.source_id.as_str(), e.date, e.account_id.as_deref()))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("rent", d(11, 18), Some("checking")), // overdue: tomorrow
                ("rent", d(12, 10), Some("checking")),
                ("new", d(11, 18), None),
                ("new", d(12, 10), None),
            ]
        );
        assert!(whole.iter().all(|e| e.amount_minor == -1_500));

        let mine = bill_entries(&bills, &recurrences, &accounts(), d(11, 18), false);
        assert!(mine.iter().all(|e| e.source_id == "rent"));
    }
}
