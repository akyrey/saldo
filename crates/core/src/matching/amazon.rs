//! Bank lines paying Amazon, matched with the order history export
//! (L19 to L22).

use std::collections::BTreeMap;

use chrono::NaiveDate;

use crate::matching::assign::{Pair, assign};
use crate::matching::lag::{EARLY_PENALTY, LagWindow};
use crate::matching::ledger::Line;
use crate::money::Money;
use crate::period::DateRange;

/// One row of the order export: one item of an order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmazonItem {
    pub order_id: String,
    /// The order date, in the household's time zone (L10).
    pub date: NaiveDate,
    /// What the item cost, positive.
    pub amount: Money,
    pub product: String,
    pub cancelled: bool,
    /// Paid entirely by gift card: never reaches a bank (L22).
    pub gift_card_only: bool,
}

/// The items of one order that reached a bank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmazonOrder {
    pub order_id: String,
    /// The earliest item date.
    pub date: NaiveDate,
    /// The sum of the items, positive.
    pub total: Money,
    /// Positions of the items in the input.
    pub items: Vec<usize>,
}

/// Groups items by order ID, leaving out cancelled and gift-card items, and
/// orders mixing currencies (Amazon never does; such a file is not trusted).
/// Sorted by date, then order ID.
pub fn amazon_orders(items: &[AmazonItem]) -> Vec<AmazonOrder> {
    let mut groups: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, item) in items.iter().enumerate() {
        if !item.cancelled && !item.gift_card_only {
            groups
                .entry(item.order_id.as_str())
                .or_default()
                .push(index);
        }
    }
    let mut orders: Vec<AmazonOrder> = groups
        .into_iter()
        .filter_map(|(order_id, indices)| order(order_id, indices, items))
        .collect();
    orders.sort_by(|a, b| (a.date, &a.order_id).cmp(&(b.date, &b.order_id)));
    orders
}

/// One order from its items, or `None` when they mix currencies or overflow.
fn order(order_id: &str, indices: Vec<usize>, items: &[AmazonItem]) -> Option<AmazonOrder> {
    let first = items.get(*indices.first()?)?;
    let currency = first.amount.currency();
    let mut total = Money::zero(currency);
    let mut date = first.date;
    for &index in &indices {
        let item = items.get(index)?;
        if item.amount.currency() != currency {
            return None;
        }
        total = total.checked_add(item.amount).ok()?;
        date = date.min(item.date);
    }
    Some(AmazonOrder {
        order_id: order_id.to_string(),
        date,
        total,
        items: indices,
    })
}

/// The shape of an order ID: digit groups of 3, 7 and 7 joined by hyphens.
const ORDER_ID_GROUPS: [usize; 3] = [3, 7, 7];
const ORDER_ID_LEN: usize = 19;

/// The standard order IDs (`ddd-ddddddd-ddddddd`) in a bank text, in order,
/// without repeats. Marketplace references such as `Mkt*AB1CD2EF3G` are not
/// order IDs (L20).
pub fn find_order_ids(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut found: Vec<String> = Vec::new();
    for start in 0..bytes.len() {
        let end = start + ORDER_ID_LEN;
        let digit_before = start > 0 && bytes[start - 1].is_ascii_digit();
        let digit_after = bytes.get(end).is_some_and(u8::is_ascii_digit);
        if digit_before || digit_after {
            continue;
        }
        // `get` returns `None` past the end or off a character boundary.
        let Some(candidate) = text.get(start..end) else {
            continue;
        };
        if is_order_id(candidate) && !found.iter().any(|id| id == candidate) {
            found.push(candidate.to_string());
        }
    }
    found
}

fn is_order_id(text: &str) -> bool {
    let groups: Vec<&str> = text.split('-').collect();
    groups.len() == ORDER_ID_GROUPS.len()
        && groups
            .iter()
            .zip(ORDER_ID_GROUPS)
            .all(|(group, len)| group.len() == len && group.bytes().all(|b| b.is_ascii_digit()))
}

/// How an order was matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmazonMatchKind {
    /// The bank text names the order: amount and date are not checked.
    OrderId,
    /// Same amount, within the window.
    Exact,
    /// Closest amount within the tolerance, within the window.
    Approximate,
}

/// One matched order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmazonMatch {
    pub order: usize,
    /// One or more lines (an order charged per shipment, a refund).
    pub lines: Vec<usize>,
    pub kind: AmazonMatchKind,
    /// What the lines charged minus the order total; zero for `Exact`.
    pub amount_diff: Money,
}

/// What [`match_amazon`] found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AmazonResult {
    /// Sorted by first line.
    pub matches: Vec<AmazonMatch>,
    pub unmatched_orders: Vec<usize>,
}

/// Matches lines with orders in three passes, each on what is left:
/// 1. every line naming an order ID joins that order's match, whatever the
///    amount and date (debits add to the charge, refunds reduce it);
/// 2. a debit of exactly the order total within `window` of the order date;
/// 3. when `tolerance_minor` is above zero, the closest total within it,
///    marked approximate.
///
/// Passes 2 and 3 only use lines marked in `eligible` (the Amazon patterns
/// hit them, L18) that name no order ID at all: a line naming an order that
/// is not in the export (or was cancelled) is that order, not another one
/// with the same amount. They are one-to-one. Unmatched orders are reported
/// only when dated inside `range`, since orders are stored for years.
pub fn match_amazon(
    lines: &[Line],
    eligible: &[bool],
    orders: &[AmazonOrder],
    window: LagWindow,
    tolerance_minor: i64,
    range: DateRange,
) -> AmazonResult {
    let mut matches = by_order_id(lines, orders);
    let mut line_used: Vec<bool> = lines
        .iter()
        .map(|line| !find_order_ids(&line.text).is_empty())
        .collect();
    let mut order_used: Vec<bool> = vec![false; orders.len()];
    mark(&matches, &mut line_used, &mut order_used);

    let exact = by_amount(lines, eligible, orders, window, &line_used, &order_used, 0);
    mark(&exact, &mut line_used, &mut order_used);
    matches.extend(exact);
    if tolerance_minor > 0 {
        let approximate = by_amount(
            lines,
            eligible,
            orders,
            window,
            &line_used,
            &order_used,
            tolerance_minor,
        );
        mark(&approximate, &mut line_used, &mut order_used);
        matches.extend(approximate);
    }

    matches.sort_by_key(|m| m.lines.first().copied());
    let unmatched_orders = (0..orders.len())
        .filter(|&o| !order_used[o] && range.contains(orders[o].date))
        .collect();
    AmazonResult {
        matches,
        unmatched_orders,
    }
}

fn mark(matches: &[AmazonMatch], line_used: &mut [bool], order_used: &mut [bool]) {
    for m in matches {
        order_used[m.order] = true;
        for &line in &m.lines {
            line_used[line] = true;
        }
    }
}

/// Pass 1: lines naming a known order ID, grouped by that order (L19).
fn by_order_id(lines: &[Line], orders: &[AmazonOrder]) -> Vec<AmazonMatch> {
    let mut lines_of: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (l, line) in lines.iter().enumerate() {
        let known = find_order_ids(&line.text).into_iter().find_map(|id| {
            orders
                .iter()
                .position(|o| o.order_id == id && o.total.currency() == line.amount.currency())
        });
        if let Some(order) = known {
            lines_of.entry(order).or_default().push(l);
        }
    }
    lines_of
        .into_iter()
        .filter_map(|(order, matched)| {
            let total = orders[order].total;
            // Debits are negative, so subtracting them adds to the charge.
            let charged = matched
                .iter()
                .try_fold(Money::zero(total.currency()), |sum, l| {
                    sum.checked_sub(lines[*l].amount)
                })
                .ok()?;
            let amount_diff = charged.checked_sub(total).ok()?;
            Some(AmazonMatch {
                order,
                lines: matched,
                kind: AmazonMatchKind::OrderId,
                amount_diff,
            })
        })
        .collect()
}

/// Passes 2 (`tolerance_minor == 0`, exact) and 3 (approximate): eligible
/// unused debits against unused orders, one-to-one.
fn by_amount(
    lines: &[Line],
    eligible: &[bool],
    orders: &[AmazonOrder],
    window: LagWindow,
    line_used: &[bool],
    order_used: &[bool],
    tolerance_minor: i64,
) -> Vec<AmazonMatch> {
    let max_distance = i64::from(window.early_days) * EARLY_PENALTY + i64::from(window.late_days);
    let mut pairs = Vec::new();
    for (l, line) in lines.iter().enumerate() {
        if line_used[l] || !eligible.get(l).copied().unwrap_or(false) || line.amount.minor() >= 0 {
            continue;
        }
        for (o, order) in orders.iter().enumerate() {
            if order_used[o] || order.total.currency() != line.amount.currency() {
                continue;
            }
            let charged = line.amount.minor().saturating_neg();
            let difference = charged.saturating_sub(order.total.minor()).saturating_abs();
            let Some(distance) = window.distance(line.date, order.date) else {
                continue;
            };
            if difference > tolerance_minor {
                continue;
            }
            // Amount first, then date (L21): one minor unit costs more than
            // the largest distance the window allows.
            pairs.push(Pair {
                line: l,
                candidate: o,
                score: difference
                    .saturating_mul(max_distance + 1)
                    .saturating_add(distance),
            });
        }
    }
    let uses: Vec<Vec<usize>> = (0..orders.len()).map(|o| vec![o]).collect();
    let kind = if tolerance_minor == 0 {
        AmazonMatchKind::Exact
    } else {
        AmazonMatchKind::Approximate
    };
    assign(&pairs, &uses, &[])
        .into_iter()
        .map(|(l, o)| {
            let charged = Money::new(
                lines[l].amount.minor().saturating_neg(),
                orders[o].total.currency(),
            );
            let amount_diff = Money::new(
                charged.minor().saturating_sub(orders[o].total.minor()),
                orders[o].total.currency(),
            );
            AmazonMatch {
                order: o,
                lines: vec![l],
                kind,
                amount_diff,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{date, eur, usd};

    const WINDOW: LagWindow = LagWindow {
        early_days: 2,
        late_days: 14,
    };

    fn item(order: &str, day: u32, amount: Money, product: &str) -> AmazonItem {
        AmazonItem {
            order_id: order.to_string(),
            date: date(day),
            amount,
            product: product.to_string(),
            cancelled: false,
            gift_card_only: false,
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

    const A: &str = "405-1234567-1234567";
    const B: &str = "171-7654321-7654321";

    #[test]
    fn items_are_grouped_into_orders_without_cancelled_or_gift_card_items() {
        let mut cancelled = item(A, 3, eur(1_000), "Cancelled cable");
        cancelled.cancelled = true;
        let mut gift = item("000-0000000-0000000", 1, eur(500), "Gift card book");
        gift.gift_card_only = true;
        let items = [
            item(B, 4, eur(2_000), "Lamp"),
            item(A, 3, eur(1_500), "Book"),
            cancelled,
            item(A, 2, eur(999), "Pen"),
            gift,
        ];
        assert_eq!(
            amazon_orders(&items),
            vec![
                AmazonOrder {
                    order_id: A.to_string(),
                    date: date(2),
                    total: eur(2_499),
                    items: vec![1, 3]
                },
                AmazonOrder {
                    order_id: B.to_string(),
                    date: date(4),
                    total: eur(2_000),
                    items: vec![0]
                },
            ]
        );
    }

    #[test]
    fn an_order_mixing_currencies_is_left_out() {
        let items = [item(A, 2, eur(1_000), "x"), item(A, 2, usd(1_000), "y")];
        assert!(amazon_orders(&items).is_empty());
    }

    #[test]
    fn order_ids_are_found_in_bank_text() {
        assert_eq!(
            find_order_ids(&format!("AMAZON EU SARL {A} PAGAMENTO")),
            vec![A]
        );
        assert_eq!(find_order_ids(&format!("{A},{B} {A}")), vec![A, B]);
        assert!(find_order_ids("Amazon Mkt*AB1CD2EF3G").is_empty());
        assert!(find_order_ids("1405-1234567-1234567").is_empty());
        assert!(find_order_ids("405-1234567-12345678").is_empty());
        assert!(find_order_ids("405-123456-1234567").is_empty());
    }

    #[test]
    fn the_order_id_wins_over_amount_and_date_and_collects_every_charge() {
        let orders = amazon_orders(&[item(A, 1, eur(5_000), "Shoes")]);
        // Charged in two shipments weeks later, one partly by gift card,
        // and one refund naming the order.
        let lines = [
            line(20, -3_000, &format!("AMAZON {A}")),
            line(25, -1_500, &format!("AMAZON {A}")),
            line(28, 500, &format!("RIMBORSO AMAZON {A}")),
        ];
        let result = match_amazon(
            &lines,
            &[false, false, false],
            &orders,
            WINDOW,
            0,
            november(),
        );
        assert_eq!(
            result.matches,
            vec![AmazonMatch {
                order: 0,
                lines: vec![0, 1, 2],
                kind: AmazonMatchKind::OrderId,
                amount_diff: eur(-1_000),
            }]
        );
        assert!(result.unmatched_orders.is_empty());
    }

    #[test]
    fn without_an_id_the_exact_amount_in_the_window_matches_one_to_one() {
        let orders = amazon_orders(&[
            item(A, 2, eur(2_499), "Book"),
            item(B, 3, eur(2_499), "Book"),
        ]);
        let lines = [
            line(4, -2_499, "AMZN Mktp IT"),
            line(6, -2_499, "AMZN Mktp IT"),
            line(30, -2_499, "AMZN Mktp IT"), // too late
            line(4, -2_499, "SHOP"),          // not eligible
        ];
        let result = match_amazon(
            &lines,
            &[true, true, true, false],
            &orders,
            WINDOW,
            0,
            november(),
        );
        let pairs: Vec<(usize, Vec<usize>)> = result
            .matches
            .iter()
            .map(|m| (m.order, m.lines.clone()))
            .collect();
        // Each line takes the order dated closest before it: line 0 (4th)
        // is one day after order B, line 1 (6th) four days after order A.
        assert_eq!(pairs, vec![(1, vec![0]), (0, vec![1])]);
        assert!(
            result
                .matches
                .iter()
                .all(|m| m.kind == AmazonMatchKind::Exact)
        );
    }

    #[test]
    fn the_closest_amount_within_the_tolerance_is_approximate() {
        let orders = amazon_orders(&[
            item(A, 2, eur(2_000), "Lamp"),
            item(B, 2, eur(2_150), "Lamp"),
        ]);
        let lines = [line(5, -2_100, "AMAZON")];
        let result = match_amazon(&lines, &[true], &orders, WINDOW, 200, november());
        // Same date, so orders sort by ID: B (171-…, 21.50) is order 0.
        assert_eq!(
            result.matches,
            vec![AmazonMatch {
                order: 0,
                lines: vec![0],
                kind: AmazonMatchKind::Approximate,
                amount_diff: eur(-50),
            }]
        );
        assert_eq!(result.unmatched_orders, vec![1]);
    }

    #[test]
    fn an_exact_match_is_taken_before_approximate_ones() {
        let orders = amazon_orders(&[item(A, 2, eur(2_000), "Lamp")]);
        let lines = [line(5, -2_010, "AMAZON"), line(6, -2_000, "AMAZON")];
        let result = match_amazon(&lines, &[true, true], &orders, WINDOW, 200, november());
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].lines, vec![1]);
        assert_eq!(result.matches[0].kind, AmazonMatchKind::Exact);
    }

    #[test]
    fn no_approximate_pass_without_a_tolerance() {
        let orders = amazon_orders(&[item(A, 2, eur(2_000), "Lamp")]);
        let result = match_amazon(
            &[line(5, -2_001, "AMAZON")],
            &[true],
            &orders,
            WINDOW,
            0,
            november(),
        );
        assert!(result.matches.is_empty());
        assert_eq!(result.unmatched_orders, vec![0]);
    }

    fn november() -> DateRange {
        DateRange::new(date(1), date(30)).unwrap()
    }

    #[test]
    fn a_closer_amount_beats_a_closer_date_in_the_approximate_pass() {
        // L21: the closest amount wins. Order A is 1 cent off but 12 days
        // before; order B is 2 cents off on the same day.
        let orders = amazon_orders(&[
            item(A, 1, eur(2_001), "Lamp"),
            item(B, 13, eur(2_002), "Lamp"),
        ]);
        let result = match_amazon(
            &[line(13, -2_000, "AMAZON")],
            &[true],
            &orders,
            WINDOW,
            5,
            november(),
        );
        assert_eq!(orders[result.matches[0].order].order_id, A);
    }

    #[test]
    fn the_tolerance_edge_is_inclusive() {
        let orders = amazon_orders(&[item(A, 2, eur(2_000), "Lamp")]);
        let at = |minor| {
            match_amazon(
                &[line(5, minor, "AMAZON")],
                &[true],
                &orders,
                WINDOW,
                200,
                november(),
            )
        };
        assert_eq!(at(-2_200).matches.len(), 1);
        assert_eq!(at(-2_201).matches.len(), 0);
        assert_eq!(at(-2_200).matches[0].amount_diff, eur(200));
    }

    #[test]
    fn a_line_naming_an_unknown_order_is_never_matched_by_amount() {
        // The order is not in the export (or was cancelled): the ID says this
        // charge is another order, so it must not take B by amount (L19).
        let mut cancelled = item(A, 2, eur(2_499), "Cancelled");
        cancelled.cancelled = true;
        let orders = amazon_orders(&[item(B, 2, eur(2_499), "Book"), cancelled]);
        let lines = [
            line(4, -2_499, "AMAZON 111-2223333-4445556"),
            line(4, -2_499, &format!("AMAZON {A}")),
        ];
        let result = match_amazon(&lines, &[true, true], &orders, WINDOW, 100, november());
        assert!(result.matches.is_empty());
    }

    #[test]
    fn a_line_naming_an_unknown_and_a_known_order_joins_the_known_one() {
        let orders = amazon_orders(&[item(A, 2, eur(2_499), "Book")]);
        let lines = [line(4, -2_499, &format!("AMAZON 111-2223333-4445556 {A}"))];
        let result = match_amazon(&lines, &[false], &orders, WINDOW, 0, november());
        assert_eq!(result.matches[0].kind, AmazonMatchKind::OrderId);
    }

    #[test]
    fn an_order_matched_by_id_is_not_offered_again() {
        let orders = amazon_orders(&[item(A, 2, eur(2_499), "Book")]);
        let lines = [
            line(4, -2_499, &format!("AMAZON {A}")),
            line(5, -2_499, "AMAZON"),
        ];
        let result = match_amazon(&lines, &[true, true], &orders, WINDOW, 0, november());
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].lines, vec![0]);
    }

    #[test]
    fn a_line_in_another_currency_does_not_join_an_order() {
        let orders = amazon_orders(&[item(A, 2, eur(2_499), "Book")]);
        let mut dollars = line(4, 0, &format!("AMAZON {A}"));
        dollars.amount = usd(-2_499);
        let result = match_amazon(&[dollars], &[true], &orders, WINDOW, 0, november());
        assert!(result.matches.is_empty());
    }

    #[test]
    fn unmatched_orders_outside_the_range_are_not_reported() {
        let orders = amazon_orders(&[item(A, 2, eur(2_499), "Book"), item(B, 20, eur(999), "Pen")]);
        let range = DateRange::new(date(10), date(30)).unwrap();
        let result = match_amazon(&[], &[], &orders, WINDOW, 0, range);
        assert_eq!(result.unmatched_orders, vec![1]);
    }

    #[test]
    fn order_ids_next_to_other_characters() {
        assert_eq!(find_order_ids(&format!("\u{20ac}{A}\u{e8}")), vec![A]);
        assert_eq!(find_order_ids(&format!("x{A}")), vec![A]);
        assert!(find_order_ids("").is_empty());
    }
}
