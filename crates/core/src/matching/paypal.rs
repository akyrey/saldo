//! Bank lines paying PayPal, matched with the PayPal activity export
//! (L23 to L25).

use chrono::{NaiveDate, NaiveTime};

use crate::matching::assign::{Pair, assign};
use crate::matching::lag::LagWindow;
use crate::matching::ledger::{DAY_COST, Line, SIMILARITY_WEIGHT_DAYS};
use crate::matching::text::{coverage, tokens};
use crate::money::Money;
use crate::period::DateRange;

/// What a PayPal row did with the money.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayPalKind {
    /// Money from the bank into PayPal, usually to pay one payment.
    Funding,
    /// Money from PayPal to the bank.
    Withdrawal,
    /// Money out of PayPal to a merchant.
    Payment,
    /// Money back from a merchant.
    Refund,
    /// Moves no money: holds, their release, authorisations, currency
    /// conversions, denied or cancelled rows.
    Other,
}

/// `Tipo` prefixes (lowercase) of rows that move no money.
const OTHER_TYPE_PREFIXES: [&str; 5] = [
    "autorizzazione",
    "annullamento",
    "blocco",
    "storno di blocco",
    "conversione di valuta",
];

/// `Stato` values (lowercase) of rows whose money never moved.
const VOID_STATUSES: [&str; 3] = ["negata", "annullata", "stornata"];

/// Classifies a row by its `Tipo` (type) and `Stato` (status), Italian
/// export. A row of unknown type counts as a payment when money left PayPal
/// and is ignored otherwise.
pub fn classify_paypal(kind: &str, status: &str, gross: Money) -> PayPalKind {
    // Some exports write the apostrophe of "dall'utente" as a curly one.
    let kind = kind.trim().to_lowercase().replace('\u{2019}', "'");
    let status = status.trim().to_lowercase();
    if VOID_STATUSES.contains(&status.as_str())
        || OTHER_TYPE_PREFIXES
            .iter()
            .any(|prefix| kind.starts_with(prefix))
    {
        return PayPalKind::Other;
    }
    if kind.starts_with("bonifico bancario sul conto paypal") {
        PayPalKind::Funding
    } else if kind.starts_with("trasferimento avviato dall'utente") {
        PayPalKind::Withdrawal
    } else if kind.starts_with("rimborso") {
        PayPalKind::Refund
    } else if gross.minor() < 0 {
        PayPalKind::Payment
    } else {
        PayPalKind::Other
    }
}

/// A PayPal row, already classified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayPalRow {
    pub date: NaiveDate,
    pub time: Option<NaiveTime>,
    pub kind: PayPalKind,
    /// The gross amount, signed as PayPal writes it.
    pub amount: Money,
    /// `Nome`: the merchant or person.
    pub name: String,
}

/// One PayPal movement as the bank sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayPalMovement {
    /// The row whose amount reaches the bank.
    pub movement: usize,
    /// The row naming the counterparty: the paired payment for a funding
    /// row, otherwise `movement` itself.
    pub merchant: usize,
    pub kind: PayPalKind,
}

impl PayPalMovement {
    /// A funding row paired with the payment it paid for.
    pub fn funded(&self) -> bool {
        self.movement != self.merchant
    }

    /// Funding and withdrawals always reach the bank; a bare payment or
    /// refund may live on the PayPal balance only (L25).
    fn must_reach_bank(&self) -> bool {
        matches!(self.kind, PayPalKind::Funding | PayPalKind::Withdrawal)
    }
}

/// Turns rows into movements, pairing each funding row with the payment
/// written at the same date and time (L24): the same amount wins; a single
/// payment at that exact time is taken even with another amount (a
/// foreign-currency payment funded in euro); without a time, date and amount
/// must agree. Funding movements come first in row order, then the other
/// unpaired payments, refunds and withdrawals; `Other` rows are dropped.
pub fn pair_paypal_funding(rows: &[PayPalRow]) -> Vec<PayPalMovement> {
    let fundings: Vec<usize> = (0..rows.len())
        .filter(|&index| rows[index].kind == PayPalKind::Funding)
        .collect();
    // `merchant_of[f]` is the payment paired with funding row `f`. Every
    // funding takes its exact amount first; only then may a funding take the
    // single payment left at its moment, so the fallback never steals an
    // exact pair from a later row.
    let mut merchant_of: Vec<Option<usize>> = vec![None; rows.len()];
    let mut paired = vec![false; rows.len()];
    for exact in [true, false] {
        for &funding in &fundings {
            if merchant_of[funding].is_some() {
                continue;
            }
            if let Some(payment) = funded_payment(&rows[funding], rows, &paired, exact) {
                paired[payment] = true;
                merchant_of[funding] = Some(payment);
            }
        }
    }
    let mut movements: Vec<PayPalMovement> = fundings
        .iter()
        .map(|&funding| PayPalMovement {
            movement: funding,
            merchant: merchant_of[funding].unwrap_or(funding),
            kind: PayPalKind::Funding,
        })
        .collect();
    for (index, row) in rows.iter().enumerate() {
        if paired[index] || matches!(row.kind, PayPalKind::Funding | PayPalKind::Other) {
            continue;
        }
        movements.push(PayPalMovement {
            movement: index,
            merchant: index,
            kind: row.kind,
        });
    }
    movements
}

/// The unpaired payment a funding row paid for (L24), or `None`: with
/// `exact`, one of the same amount at its moment; without, the only payment
/// at its moment (and only when the funding row has a time).
fn funded_payment(
    funding: &PayPalRow,
    rows: &[PayPalRow],
    paired: &[bool],
    exact: bool,
) -> Option<usize> {
    let at_that_moment: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(index, row)| {
            row.kind == PayPalKind::Payment
                && !paired[*index]
                && row.date == funding.date
                && (funding.time.is_none() || row.time == funding.time)
        })
        .map(|(index, _)| index)
        .collect();
    if exact {
        return at_that_moment
            .into_iter()
            .find(|&index| same_size(rows[index].amount, funding.amount));
    }
    match (funding.time, at_that_moment.as_slice()) {
        (Some(_), [only]) => Some(*only),
        _ => None,
    }
}

/// The same amount ignoring the sign, in the same currency.
fn same_size(a: Money, b: Money) -> bool {
    a.currency() == b.currency() && a.minor().unsigned_abs() == b.minor().unsigned_abs()
}

/// What [`match_paypal`] found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PayPalMatch {
    /// `(line, movement)`, sorted by line.
    pub matched: Vec<(usize, usize)>,
    /// Funding and withdrawals with no bank line: a finding.
    pub missing_from_bank: Vec<usize>,
    /// Payments and refunds with no bank line: settled on the PayPal balance.
    pub balance_only: Vec<usize>,
}

/// Matches bank lines with movements one-to-one. Only lines marked in
/// `eligible` (the PayPal patterns hit them, L18) take part. A pair needs
/// the same absolute amount and currency, a matching direction (a debit pays
/// a funding or payment, a credit receives a withdrawal or refund) and the
/// line within `window` of the movement's date. It scores the weighted lag
/// minus the coverage of the merchant's name in the bank text (L12).
/// Leftover movements are reported only when dated inside `range`: rows are
/// stored for years, and only the statement's period can be missing.
pub fn match_paypal(
    lines: &[Line],
    eligible: &[bool],
    rows: &[PayPalRow],
    movements: &[PayPalMovement],
    window: LagWindow,
    range: DateRange,
) -> PayPalMatch {
    let mut pairs = Vec::new();
    for (l, line) in lines.iter().enumerate() {
        if !eligible.get(l).copied().unwrap_or(false) {
            continue;
        }
        let line_tokens = tokens(&line.text);
        for (m, movement) in movements.iter().enumerate() {
            let (Some(moved), Some(merchant)) =
                (rows.get(movement.movement), rows.get(movement.merchant))
            else {
                continue;
            };
            let debit = line.amount.minor() < 0;
            let pays_paypal = matches!(movement.kind, PayPalKind::Funding | PayPalKind::Payment);
            if debit != pays_paypal || !same_size(line.amount, moved.amount) {
                continue;
            }
            let Some(distance) = window.distance(line.date, moved.date) else {
                continue;
            };
            let similarity = coverage(&tokens(&merchant.name), &line_tokens);
            pairs.push(Pair {
                line: l,
                candidate: m,
                score: distance * DAY_COST - SIMILARITY_WEIGHT_DAYS * i64::from(similarity),
            });
        }
    }
    let uses: Vec<Vec<usize>> = (0..movements.len()).map(|m| vec![m]).collect();
    let matched = assign(&pairs, &uses, &[]);

    let mut result = PayPalMatch::default();
    for (m, movement) in movements.iter().enumerate() {
        let in_range = rows
            .get(movement.movement)
            .is_some_and(|row| range.contains(row.date));
        if !in_range || matched.iter().any(|(_, used)| *used == m) {
            continue;
        }
        if movement.must_reach_bank() {
            result.missing_from_bank.push(m);
        } else {
            result.balance_only.push(m);
        }
    }
    result.matched = matched;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{date, eur, usd};

    const WINDOW: LagWindow = LagWindow {
        early_days: 2,
        late_days: 7,
    };

    fn at(hour: u32, minute: u32) -> Option<NaiveTime> {
        NaiveTime::from_hms_opt(hour, minute, 0)
    }

    fn row(
        day: u32,
        time: Option<NaiveTime>,
        kind: PayPalKind,
        amount: Money,
        name: &str,
    ) -> PayPalRow {
        PayPalRow {
            date: date(day),
            time,
            kind,
            amount,
            name: name.to_string(),
        }
    }

    fn line(day: u32, minor: i64, text: &str) -> Line {
        Line {
            date: date(day),
            other_dates: vec![],
            window: None,
            amount: eur(minor),
            text: text.to_string(),
        }
    }

    #[test]
    fn classifies_by_type_status_and_sign() {
        let cases = [
            (
                "Bonifico bancario sul conto PayPal",
                "Completata",
                eur(1_299),
                PayPalKind::Funding,
            ),
            (
                "Trasferimento avviato dall'utente",
                "Completata",
                eur(-5_000),
                PayPalKind::Withdrawal,
            ),
            (
                "Rimborso di pagamento",
                "Completata",
                eur(1_299),
                PayPalKind::Refund,
            ),
            (
                "Pagamento preautorizzato",
                "Completata",
                eur(-1_299),
                PayPalKind::Payment,
            ),
            (
                "Pagamento Express Checkout",
                "Completata",
                eur(-1_299),
                PayPalKind::Payment,
            ),
            (
                "Autorizzazione generica",
                "In sospeso",
                eur(-1_299),
                PayPalKind::Other,
            ),
            (
                "Blocco su saldo per verifica",
                "Completata",
                eur(-500),
                PayPalKind::Other,
            ),
            (
                "Storno di blocco generico",
                "Completata",
                eur(500),
                PayPalKind::Other,
            ),
            (
                "Conversione di valuta generica",
                "Completata",
                eur(-1_100),
                PayPalKind::Other,
            ),
            (
                "Pagamento Express Checkout",
                "Negata",
                eur(-1_299),
                PayPalKind::Other,
            ),
            (
                "  BONIFICO BANCARIO sul conto PayPal ",
                "Annullata",
                eur(1_299),
                PayPalKind::Other,
            ),
            ("Something new", "Completata", eur(200), PayPalKind::Other),
        ];
        for (kind, status, gross, expected) in cases {
            assert_eq!(
                classify_paypal(kind, status, gross),
                expected,
                "{kind} / {status}"
            );
        }
    }

    #[test]
    fn a_funding_row_pairs_with_the_payment_at_the_same_time() {
        let rows = [
            row(5, at(10, 0), PayPalKind::Payment, eur(-1_299), "Netflix"),
            row(5, at(10, 0), PayPalKind::Funding, eur(1_299), ""),
            row(5, at(11, 0), PayPalKind::Payment, eur(-1_299), "Spotify"),
        ];
        assert_eq!(
            pair_paypal_funding(&rows),
            vec![
                PayPalMovement {
                    movement: 1,
                    merchant: 0,
                    kind: PayPalKind::Funding
                },
                PayPalMovement {
                    movement: 2,
                    merchant: 2,
                    kind: PayPalKind::Payment
                },
            ]
        );
    }

    #[test]
    fn the_amount_decides_between_two_payments_at_the_same_time() {
        let rows = [
            row(5, at(10, 0), PayPalKind::Payment, eur(-500), "Coffee"),
            row(5, at(10, 0), PayPalKind::Payment, eur(-1_299), "Netflix"),
            row(5, at(10, 0), PayPalKind::Funding, eur(1_299), ""),
        ];
        let movements = pair_paypal_funding(&rows);
        assert_eq!(
            movements[0],
            PayPalMovement {
                movement: 2,
                merchant: 1,
                kind: PayPalKind::Funding
            }
        );
    }

    #[test]
    fn a_single_payment_at_that_time_pairs_even_in_another_currency() {
        let rows = [
            row(5, at(10, 0), PayPalKind::Payment, usd(-1_500), "US shop"),
            row(5, at(10, 0), PayPalKind::Funding, eur(1_380), ""),
        ];
        let movements = pair_paypal_funding(&rows);
        assert_eq!(
            movements,
            vec![PayPalMovement {
                movement: 1,
                merchant: 0,
                kind: PayPalKind::Funding
            }]
        );
    }

    #[test]
    fn without_a_time_date_and_amount_must_agree() {
        let rows = [
            row(5, None, PayPalKind::Payment, eur(-1_000), "Other amount"),
            row(5, None, PayPalKind::Funding, eur(1_299), ""),
        ];
        let movements = pair_paypal_funding(&rows);
        assert!(movements.iter().all(|m| !m.funded()));
        assert_eq!(movements.len(), 2);
    }

    #[test]
    fn other_rows_are_dropped() {
        let rows = [row(5, at(9, 0), PayPalKind::Other, eur(-1_299), "Hold")];
        assert!(pair_paypal_funding(&rows).is_empty());
    }

    #[test]
    fn a_funded_purchase_matches_its_bank_line_and_names_the_merchant() {
        let rows = [
            row(
                5,
                at(10, 0),
                PayPalKind::Payment,
                eur(-1_299),
                "Netflix International",
            ),
            row(5, at(10, 0), PayPalKind::Funding, eur(1_299), ""),
        ];
        let movements = pair_paypal_funding(&rows);
        let lines = [line(7, -1_299, "PAYPAL *NETFLIX")];
        let result = match_paypal(&lines, &[true], &rows, &movements, WINDOW, november());
        assert_eq!(result.matched, vec![(0, 0)]);
        assert_eq!(rows[movements[0].merchant].name, "Netflix International");
        assert!(result.missing_from_bank.is_empty());
    }

    #[test]
    fn direction_amount_window_and_eligibility_must_fit() {
        let rows = [
            row(10, at(10, 0), PayPalKind::Funding, eur(1_000), ""),
            row(10, at(11, 0), PayPalKind::Withdrawal, eur(-2_000), ""),
        ];
        let movements = pair_paypal_funding(&rows);
        let lines = [
            line(11, 1_000, "PAYPAL"),  // a credit cannot be a funding
            line(11, -1_001, "PAYPAL"), // another amount
            line(7, -1_000, "PAYPAL"),  // three days before: outside
            line(11, -1_000, "SHOP"),   // not eligible
            line(12, 2_000, "PAYPAL"),  // the withdrawal
        ];
        let result = match_paypal(
            &lines,
            &[true, true, true, false, true],
            &rows,
            &movements,
            WINDOW,
            november(),
        );
        assert_eq!(result.matched, vec![(4, 1)]);
        assert_eq!(result.missing_from_bank, vec![0]);
    }

    #[test]
    fn the_merchant_name_breaks_ties_and_leftovers_are_sorted_out() {
        let rows = [
            row(5, at(10, 0), PayPalKind::Payment, eur(-1_299), "Netflix"),
            row(5, at(11, 0), PayPalKind::Payment, eur(-1_299), "Spotify"),
            row(6, at(9, 0), PayPalKind::Refund, eur(800), "Shop"),
        ];
        let movements = pair_paypal_funding(&rows);
        let lines = [line(6, -1_299, "PAYPAL *SPOTIFY")];
        let result = match_paypal(&lines, &[true], &rows, &movements, WINDOW, november());
        assert_eq!(result.matched, vec![(0, 1)]);
        // A payment and a refund with no bank line stayed on the balance.
        assert_eq!(result.balance_only, vec![0, 2]);
        assert!(result.missing_from_bank.is_empty());
    }

    #[test]
    fn a_later_bank_line_beats_one_as_far_before() {
        let rows = [row(10, at(10, 0), PayPalKind::Funding, eur(1_000), "")];
        let movements = pair_paypal_funding(&rows);
        let lines = [line(9, -1_000, "PAYPAL"), line(11, -1_000, "PAYPAL")];
        let result = match_paypal(&lines, &[true, true], &rows, &movements, WINDOW, november());
        assert_eq!(result.matched, vec![(1, 0)]);
    }

    fn november() -> DateRange {
        DateRange::new(date(1), date(30)).unwrap()
    }

    #[test]
    fn a_curly_apostrophe_still_means_a_withdrawal() {
        let kind = classify_paypal(
            "Trasferimento avviato dall\u{2019}utente",
            "Completata",
            eur(-5_000),
        );
        assert_eq!(kind, PayPalKind::Withdrawal);
    }

    #[test]
    fn exact_amounts_pair_before_the_single_payment_fallback() {
        // The first funding (10.00) must not take the only payment (20.00)
        // through the fallback while the second funding matches it exactly.
        let rows = [
            row(5, at(10, 0), PayPalKind::Funding, eur(1_000), ""),
            row(5, at(10, 0), PayPalKind::Funding, eur(2_000), ""),
            row(5, at(10, 0), PayPalKind::Payment, eur(-2_000), "Shop"),
        ];
        let movements = pair_paypal_funding(&rows);
        assert_eq!(
            movements,
            vec![
                PayPalMovement {
                    movement: 0,
                    merchant: 0,
                    kind: PayPalKind::Funding
                },
                PayPalMovement {
                    movement: 1,
                    merchant: 2,
                    kind: PayPalKind::Funding
                },
            ]
        );
    }

    #[test]
    fn a_hold_at_the_same_moment_is_never_paired() {
        let rows = [
            row(5, at(10, 0), PayPalKind::Other, eur(-1_299), "Hold"),
            row(5, at(10, 0), PayPalKind::Funding, eur(1_299), ""),
        ];
        let movements = pair_paypal_funding(&rows);
        assert_eq!(
            movements,
            vec![PayPalMovement {
                movement: 1,
                merchant: 1,
                kind: PayPalKind::Funding
            }]
        );
    }

    #[test]
    fn credits_receive_refunds_and_never_pay() {
        let rows = [
            row(10, at(9, 0), PayPalKind::Refund, eur(800), "Shop"),
            row(10, at(10, 0), PayPalKind::Payment, eur(-1_000), "Shop"),
            row(10, at(11, 0), PayPalKind::Withdrawal, eur(-3_000), ""),
        ];
        let movements = pair_paypal_funding(&rows);
        let lines = [
            line(11, 800, "PAYPAL"),    // the refund
            line(11, 1_000, "PAYPAL"),  // a credit cannot be a payment
            line(11, -3_000, "PAYPAL"), // a debit cannot be a withdrawal
        ];
        let result = match_paypal(&lines, &[true; 3], &rows, &movements, WINDOW, november());
        assert_eq!(result.matched, vec![(0, 0)]);
    }

    #[test]
    fn another_currency_never_matches() {
        let rows = [row(10, at(9, 0), PayPalKind::Funding, eur(1_000), "")];
        let movements = pair_paypal_funding(&rows);
        let mut dollars = line(10, 0, "PAYPAL");
        dollars.amount = usd(-1_000);
        let result = match_paypal(&[dollars], &[true], &rows, &movements, WINDOW, november());
        assert!(result.matched.is_empty());
    }

    #[test]
    fn the_window_edges_are_inclusive() {
        let rows = [row(10, at(9, 0), PayPalKind::Funding, eur(1_000), "")];
        let movements = pair_paypal_funding(&rows);
        let matched = |day| {
            match_paypal(
                &[line(day, -1_000, "PAYPAL")],
                &[true],
                &rows,
                &movements,
                WINDOW,
                november(),
            )
            .matched
            .len()
        };
        assert_eq!((matched(8), matched(17)), (1, 1));
        assert_eq!((matched(7), matched(18)), (0, 0));
    }

    #[test]
    fn leftovers_outside_the_statement_range_are_not_reported() {
        let rows = [
            row(3, at(9, 0), PayPalKind::Funding, eur(1_000), ""),
            row(20, at(9, 0), PayPalKind::Payment, eur(-500), "Shop"),
        ];
        let movements = pair_paypal_funding(&rows);
        let range = DateRange::new(date(10), date(30)).unwrap();
        let result = match_paypal(&[], &[], &rows, &movements, WINDOW, range);
        assert!(result.missing_from_bank.is_empty());
        assert_eq!(result.balance_only, vec![1]);
    }

    #[test]
    fn a_line_without_an_eligibility_flag_is_not_eligible() {
        let rows = [row(10, at(9, 0), PayPalKind::Funding, eur(1_000), "")];
        let movements = pair_paypal_funding(&rows);
        let result = match_paypal(
            &[line(10, -1_000, "PAYPAL")],
            &[],
            &rows,
            &movements,
            WINDOW,
            november(),
        );
        assert!(result.matched.is_empty());
    }
}
