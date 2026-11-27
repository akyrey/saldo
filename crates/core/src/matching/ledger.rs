//! Statement lines against the splits already in Saldo (L11 to L15).

use std::collections::BTreeMap;

use chrono::NaiveDate;

use crate::currency::CurrencyCode;
use crate::ids::{AccountId, GroupId, SplitId};
use crate::kind::SplitKind;
use crate::matching::assign::{Pair, assign};
use crate::matching::lag::LagWindow;
use crate::matching::text::{jaccard, normalise, tokens};
use crate::money::Money;
use crate::period::DateRange;

/// How many score points one day apart costs; a full description match
/// (`PER_MILLE`) is worth `SIMILARITY_WEIGHT_DAYS` days.
pub const DAY_COST: i64 = 1000;
/// A perfect description match outweighs three days of date distance:
/// banks and the ledger often disagree on the date by a day or two.
pub const SIMILARITY_WEIGHT_DAYS: i64 = 3;

/// A statement line as the matchers see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The date the transaction gets (after the profile's date rules).
    pub date: NaiveDate,
    /// Other dates the ledger may have used: the accounting date when a date
    /// rule moved the line, the dates of the Amazon orders it pays (L8, L9).
    pub other_dates: Vec<NaiveDate>,
    /// Signed for the account: negative when money left it.
    pub amount: Money,
    pub text: String,
    /// A window of its own instead of the symmetric `window_days`: a
    /// transfer found through a transfer rule, such as a Satispay top-up the
    /// bank books up to a week later (decision 10). The lag is this line's
    /// date minus the candidate's.
    pub window: Option<LagWindow>,
}

impl Line {
    fn dates(&self) -> impl Iterator<Item = NaiveDate> + '_ {
        std::iter::once(self.date).chain(self.other_dates.iter().copied())
    }
}

/// A split as stored, for building candidates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerSplit {
    pub id: SplitId,
    pub group: GroupId,
    pub kind: SplitKind,
    pub date: NaiveDate,
    /// Positive, as stored.
    pub amount: Money,
    pub source: AccountId,
    pub destination: AccountId,
    pub description: String,
}

/// Something a line can match: one split, or a group's splits taken together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The splits this candidate stands for (one, or a whole group).
    pub splits: Vec<SplitId>,
    pub kind: SplitKind,
    pub date: NaiveDate,
    /// Signed for the account, like [`Line::amount`]: equal signed amounts
    /// also mean the same direction, so a debit only matches a withdrawal
    /// from the account or a transfer out of it (L13).
    pub amount: Money,
    /// The distinct descriptions, joined with " / " for a group.
    pub description: String,
}

/// One split per candidate for every split touching `account`, followed by
/// one candidate per group of two or more such splits that share kind,
/// currency, date and side, with the summed amount (L14). Opening balances
/// and reconciliations are never candidates. Splits keep their input order;
/// groups follow in the order they first appear.
pub fn ledger_candidates(splits: &[LedgerSplit], account: AccountId) -> Vec<Candidate> {
    let mut candidates: Vec<Candidate> = splits
        .iter()
        .filter_map(|split| single(split, account))
        .collect();

    // Group order = first appearance, so the output is stable.
    let mut order: Vec<GroupId> = Vec::new();
    let mut groups: BTreeMap<GroupId, Vec<&LedgerSplit>> = BTreeMap::new();
    for split in splits {
        let members = groups.entry(split.group).or_default();
        if members.is_empty() {
            order.push(split.group);
        }
        members.push(split);
    }
    for group in order {
        if let Some(combined) = groups.get(&group).and_then(|m| combine(m, account)) {
            candidates.push(combined);
        }
    }
    candidates
}

/// The candidate for one split, or `None` when it does not touch the
/// account or is an opening balance or reconciliation.
fn single(split: &LedgerSplit, account: AccountId) -> Option<Candidate> {
    if matches!(
        split.kind,
        SplitKind::OpeningBalance | SplitKind::Reconciliation
    ) {
        return None;
    }
    let amount = if split.source == account {
        split.amount.negated()
    } else if split.destination == account {
        split.amount
    } else {
        return None;
    };
    Some(Candidate {
        splits: vec![split.id],
        kind: split.kind,
        date: split.date,
        amount,
        description: split.description.clone(),
    })
}

/// The summed candidate of a group whose splits all touch the account on the
/// same side with the same kind, currency and date; `None` otherwise or for
/// a single split.
fn combine(members: &[&LedgerSplit], account: AccountId) -> Option<Candidate> {
    if members.len() < 2 {
        return None;
    }
    let parts: Vec<Candidate> = members
        .iter()
        .map(|split| single(split, account))
        .collect::<Option<Vec<_>>>()?;
    let first = parts.first()?;
    let alike = parts.iter().all(|p| {
        p.kind == first.kind
            && p.date == first.date
            && p.amount.currency() == first.amount.currency()
            && p.amount.is_positive() == first.amount.is_positive()
    });
    if !alike {
        return None;
    }
    let mut total = Money::zero(first.amount.currency());
    let mut descriptions: Vec<&str> = Vec::new();
    for part in &parts {
        // An overflow cannot be a real bank line; leave the group out.
        total = total.checked_add(part.amount).ok()?;
        let text = part.description.trim();
        if !text.is_empty() && !descriptions.contains(&text) {
            descriptions.push(text);
        }
    }
    Some(Candidate {
        splits: parts
            .iter()
            .flat_map(|p| p.splits.iter().copied())
            .collect(),
        kind: first.kind,
        date: first.date,
        amount: total,
        description: descriptions.join(" / "),
    })
}

/// Something about the existing splits that the statement explains (L15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// Left over, with the same amount and kind as matched candidate `of`
    /// within the window: most likely a second copy of it.
    DuplicateOf { candidate: usize, of: usize },
    /// Left over and identical (date, amount, description) to each other.
    Identical { candidates: Vec<usize> },
    /// Left over: in the statement's range but not on the statement.
    NotOnStatement { candidate: usize },
}

/// What [`match_ledger`] found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LedgerMatch {
    /// `(line, candidate)`, sorted by line.
    pub matched: Vec<(usize, usize)>,
    pub unmatched_lines: Vec<usize>,
    pub findings: Vec<Finding>,
}

/// Matches lines with candidates one-to-one. A pair needs the same signed
/// amount and a candidate date at most `window_days` from one of the line's
/// dates; it scores `days apart × DAY_COST − SIMILARITY_WEIGHT_DAYS ×
/// jaccard(texts)`. `pinned` pairs (remembered links) win. Leftover
/// candidates dated inside `range` become findings; the ones outside it were
/// only loaded for the window's margin.
pub fn match_ledger(
    lines: &[Line],
    candidates: &[Candidate],
    window_days: u32,
    range: DateRange,
    pinned: &[(usize, usize)],
) -> LedgerMatch {
    let uses = resources(candidates);
    let candidate_tokens: Vec<_> = candidates.iter().map(|c| tokens(&c.description)).collect();
    let mut pairs = Vec::new();
    for (l, line) in lines.iter().enumerate() {
        let line_tokens = tokens(&line.text);
        for (c, candidate) in candidates.iter().enumerate() {
            if candidate.amount != line.amount {
                continue;
            }
            let Some(days) = distance(line, candidate.date, window_days) else {
                continue;
            };
            let similarity = jaccard(&line_tokens, &candidate_tokens[c]);
            pairs.push(Pair {
                line: l,
                candidate: c,
                score: days * DAY_COST - SIMILARITY_WEIGHT_DAYS * i64::from(similarity),
            });
        }
    }
    // A pin naming a line this statement does not have is stale; `assign`
    // drops unknown candidates itself.
    let pinned: Vec<(usize, usize)> = pinned
        .iter()
        .copied()
        .filter(|(l, _)| *l < lines.len())
        .collect();
    let matched = assign(&pairs, &uses, &pinned);

    let matched_lines: Vec<usize> = matched.iter().map(|(l, _)| *l).collect();
    let unmatched_lines = (0..lines.len())
        .filter(|l| !matched_lines.contains(l))
        .collect();
    let findings = findings(candidates, &uses, &matched, window_days, range);
    LedgerMatch {
        matched,
        unmatched_lines,
        findings,
    }
}

/// The smallest distance in days between `date` and any of the line's
/// dates, within the line's own window or `window_days` either way; `None`
/// when no date is inside.
fn distance(line: &Line, date: NaiveDate, window_days: u32) -> Option<i64> {
    match line.window {
        Some(window) => line.dates().filter_map(|d| window.distance(d, date)).min(),
        None => line
            .dates()
            .map(|d| (d - date).num_days().abs())
            .filter(|days| *days <= i64::from(window_days))
            .min(),
    }
}

/// For each candidate, the indices of its splits in one shared numbering, so
/// [`assign`] can tell when two candidates share a split.
fn resources(candidates: &[Candidate]) -> Vec<Vec<usize>> {
    let mut numbers: BTreeMap<SplitId, usize> = BTreeMap::new();
    candidates
        .iter()
        .map(|candidate| {
            candidate
                .splits
                .iter()
                .map(|split| {
                    let next = numbers.len();
                    *numbers.entry(*split).or_insert(next)
                })
                .collect()
        })
        .collect()
}

/// Classifies the leftover candidates in `range` (L15).
fn findings(
    candidates: &[Candidate],
    uses: &[Vec<usize>],
    matched: &[(usize, usize)],
    window_days: u32,
    range: DateRange,
) -> Vec<Finding> {
    let used: Vec<usize> = matched
        .iter()
        .flat_map(|(_, c)| uses[*c].iter().copied())
        .collect();
    let untouched = |c: usize| uses[c].iter().all(|r| !used.contains(r));
    // A leftover group is reported as a whole, so its own splits are not.
    let in_leftover_group = |c: usize| {
        uses[c].len() == 1
            && (0..candidates.len())
                .any(|g| uses[g].len() > 1 && untouched(g) && uses[g].contains(&uses[c][0]))
    };
    let leftovers: Vec<usize> = (0..candidates.len())
        .filter(|&c| untouched(c) && range.contains(candidates[c].date) && !in_leftover_group(c))
        .collect();

    let mut found: Vec<(usize, Finding)> = Vec::new();
    // Money has no ordering (amounts in different currencies do not compare),
    // so the key spells out the minor units and the currency.
    let mut identical: BTreeMap<(NaiveDate, i64, CurrencyCode, String), Vec<usize>> =
        BTreeMap::new();
    for &c in &leftovers {
        match duplicate_of(c, candidates, matched, window_days) {
            Some(of) => found.push((c, Finding::DuplicateOf { candidate: c, of })),
            None => {
                let candidate = &candidates[c];
                let key = (
                    candidate.date,
                    candidate.amount.minor(),
                    candidate.amount.currency(),
                    normalise(&candidate.description),
                );
                identical.entry(key).or_default().push(c);
            }
        }
    }
    for group in identical.into_values() {
        match group.as_slice() {
            [only] => found.push((*only, Finding::NotOnStatement { candidate: *only })),
            [first, ..] => found.push((
                *first,
                Finding::Identical {
                    candidates: group.clone(),
                },
            )),
            [] => {}
        }
    }
    found.sort_by_key(|(first, _)| *first);
    found.into_iter().map(|(_, finding)| finding).collect()
}

/// The closest matched candidate with the same amount and kind within the
/// window, the lowest position on a tie.
fn duplicate_of(
    c: usize,
    candidates: &[Candidate],
    matched: &[(usize, usize)],
    window_days: u32,
) -> Option<usize> {
    let leftover = &candidates[c];
    matched
        .iter()
        .map(|(_, m)| *m)
        .filter(|&m| {
            let other = &candidates[m];
            other.amount == leftover.amount && other.kind == leftover.kind
        })
        .map(|m| ((candidates[m].date - leftover.date).num_days().abs(), m))
        .filter(|(days, _)| *days <= i64::from(window_days))
        .min()
        .map(|(_, m)| m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{CHECKING, GROCERIES, SALARY, SAVINGS, date, eur, id};
    use uuid::Uuid;

    fn split_id(n: u128) -> SplitId {
        SplitId(Uuid::from_u128(1_000 + n))
    }

    fn group_id(n: u128) -> GroupId {
        GroupId(Uuid::from_u128(2_000 + n))
    }

    /// A split of `group` moving `minor` from `from` to `to`.
    fn split(n: u128, group: u128, day: u32, minor: i64, from: u128, to: u128) -> LedgerSplit {
        let kind = match (from, to) {
            (CHECKING | SAVINGS, CHECKING | SAVINGS) => SplitKind::Transfer,
            (SALARY, _) => SplitKind::Deposit,
            _ => SplitKind::Withdrawal,
        };
        LedgerSplit {
            id: split_id(n),
            group: group_id(group),
            kind,
            date: date(day),
            amount: eur(minor),
            source: id(from),
            destination: id(to),
            description: format!("split {n}"),
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

    fn november() -> DateRange {
        DateRange::new(date(1), date(30)).unwrap()
    }

    fn candidate(day: u32, minor: i64, kind: SplitKind, splits: &[u128], text: &str) -> Candidate {
        Candidate {
            splits: splits.iter().map(|n| split_id(*n)).collect(),
            kind,
            date: date(day),
            amount: eur(minor),
            description: text.to_string(),
        }
    }

    #[test]
    fn candidates_are_signed_for_the_account() {
        let splits = [
            split(1, 1, 3, 2_000, CHECKING, GROCERIES),
            split(2, 2, 4, 150_000, SALARY, CHECKING),
            split(3, 3, 5, 10_000, CHECKING, SAVINGS),
            split(4, 4, 6, 5_000, SAVINGS, CHECKING),
            split(5, 5, 7, 1_000, SAVINGS, GROCERIES),
        ];
        let amounts: Vec<i64> = ledger_candidates(&splits, id(CHECKING))
            .iter()
            .map(|c| c.amount.minor())
            .collect();
        // The savings-only split does not touch the account.
        assert_eq!(amounts, vec![-2_000, 150_000, -10_000, 5_000]);
    }

    #[test]
    fn opening_balances_and_reconciliations_are_not_candidates() {
        let mut opening = split(1, 1, 1, 50_000, GROCERIES, CHECKING);
        opening.kind = SplitKind::OpeningBalance;
        let mut reconciliation = split(2, 2, 1, 100, CHECKING, GROCERIES);
        reconciliation.kind = SplitKind::Reconciliation;
        assert!(ledger_candidates(&[opening, reconciliation], id(CHECKING)).is_empty());
    }

    #[test]
    fn a_combinable_group_adds_a_summed_candidate_after_its_splits() {
        let mut second = split(2, 1, 3, 500, CHECKING, GROCERIES);
        second.description = "split 1".to_string();
        let splits = [
            split(1, 1, 3, 2_000, CHECKING, GROCERIES),
            second,
            split(3, 1, 3, 700, CHECKING, GROCERIES),
        ];
        let candidates = ledger_candidates(&splits, id(CHECKING));
        assert_eq!(candidates.len(), 4);
        assert_eq!(
            candidates[3],
            Candidate {
                splits: vec![split_id(1), split_id(2), split_id(3)],
                kind: SplitKind::Withdrawal,
                date: date(3),
                amount: eur(-3_200),
                description: "split 1 / split 3".to_string(),
            }
        );
    }

    #[test]
    fn groups_mixing_dates_kinds_or_sides_are_not_combined() {
        let splits = [
            // Different dates.
            split(1, 1, 3, 2_000, CHECKING, GROCERIES),
            split(2, 1, 4, 500, CHECKING, GROCERIES),
            // Different sides of the account.
            split(3, 2, 5, 2_000, CHECKING, GROCERIES),
            split(4, 2, 5, 500, SALARY, CHECKING),
            // Only one split touches the account.
            split(5, 3, 6, 2_000, CHECKING, GROCERIES),
            split(6, 3, 6, 500, SAVINGS, GROCERIES),
        ];
        assert_eq!(ledger_candidates(&splits, id(CHECKING)).len(), 5);
    }

    #[test]
    fn two_equal_monthly_payments_match_two_entries() {
        let candidates = [
            candidate(1, -1_299, SplitKind::Withdrawal, &[1], "Netflix"),
            candidate(30, -1_299, SplitKind::Withdrawal, &[2], "Netflix"),
        ];
        let lines = [
            line(2, -1_299, "NETFLIX.COM"),
            line(29, -1_299, "NETFLIX.COM"),
        ];
        let result = match_ledger(&lines, &candidates, 3, november(), &[]);
        assert_eq!(result.matched, vec![(0, 0), (1, 1)]);
        assert!(result.unmatched_lines.is_empty());
        assert!(result.findings.is_empty());
    }

    #[test]
    fn amount_direction_and_window_must_fit() {
        let candidates = [
            candidate(10, 2_000, SplitKind::Transfer, &[1], "From savings"),
            candidate(10, -2_000, SplitKind::Withdrawal, &[2], "Shop"),
            candidate(20, -5_000, SplitKind::Withdrawal, &[3], "Far away"),
        ];
        // A debit of 20.00 can only be the withdrawal, not the incoming
        // transfer; 50.00 four days away is outside a three-day window.
        let lines = [line(11, -2_000, "x"), line(16, -5_000, "y")];
        let result = match_ledger(&lines, &candidates, 3, november(), &[]);
        assert_eq!(result.matched, vec![(0, 1)]);
        assert_eq!(result.unmatched_lines, vec![1]);
    }

    #[test]
    fn the_closer_date_wins_and_similar_text_breaks_a_one_day_gap() {
        let candidates = [
            candidate(10, -2_000, SplitKind::Withdrawal, &[1], "Pharmacy"),
            candidate(11, -2_000, SplitKind::Withdrawal, &[2], "Bakery Rossi"),
        ];
        // Same day as the pharmacy, but the bakery's text matches fully.
        let lines = [line(10, -2_000, "BAKERY ROSSI")];
        let result = match_ledger(&lines, &candidates, 3, november(), &[]);
        assert_eq!(result.matched, vec![(0, 1)]);
    }

    #[test]
    fn a_line_matches_on_an_other_date() {
        let candidates = [candidate(2, -4_999, SplitKind::Withdrawal, &[1], "Amazon")];
        let mut charge = line(20, -4_999, "AMAZON");
        charge.other_dates = vec![date(1)];
        let result = match_ledger(&[charge], &candidates, 3, november(), &[]);
        assert_eq!(result.matched, vec![(0, 0)]);
    }

    #[test]
    fn a_group_matches_one_line_by_its_sum_and_its_splits_are_not_reported() {
        let splits = [
            split(1, 1, 3, 2_000, CHECKING, GROCERIES),
            split(2, 1, 3, 500, CHECKING, GROCERIES),
        ];
        let candidates = ledger_candidates(&splits, id(CHECKING));
        let result = match_ledger(
            &[line(3, -2_500, "AMAZON")],
            &candidates,
            3,
            november(),
            &[],
        );
        assert_eq!(result.matched, vec![(0, 2)]);
        assert!(result.findings.is_empty());
    }

    #[test]
    fn the_splits_of_a_group_can_match_separate_lines() {
        let splits = [
            split(1, 1, 3, 2_000, CHECKING, GROCERIES),
            split(2, 1, 3, 500, CHECKING, GROCERIES),
        ];
        let candidates = ledger_candidates(&splits, id(CHECKING));
        let lines = [line(3, -2_000, "SHOP A"), line(3, -500, "SHOP B")];
        let result = match_ledger(&lines, &candidates, 3, november(), &[]);
        assert_eq!(result.matched, vec![(0, 0), (1, 1)]);
    }

    #[test]
    fn a_group_never_matches_while_one_of_its_splits_matches_another_line() {
        // The group sums to 25.00 and split 1 is 20.00: a line of 20.00
        // matching split 1 leaves the group unusable for a line of 25.00.
        let splits = [
            split(1, 1, 3, 2_000, CHECKING, GROCERIES),
            split(2, 1, 3, 500, CHECKING, GROCERIES),
        ];
        let candidates = ledger_candidates(&splits, id(CHECKING));
        let lines = [line(3, -2_000, "split 1"), line(4, -2_500, "other")];
        let result = match_ledger(&lines, &candidates, 3, november(), &[]);
        assert_eq!(result.matched, vec![(0, 0)]);
        assert_eq!(result.unmatched_lines, vec![1]);
        // Split 2 is left over: not on the statement.
        assert_eq!(
            result.findings,
            vec![Finding::NotOnStatement { candidate: 1 }]
        );
    }

    #[test]
    fn a_pinned_link_overrides_scoring() {
        let candidates = [
            candidate(10, -2_000, SplitKind::Withdrawal, &[1], "Shop"),
            candidate(12, -2_000, SplitKind::Withdrawal, &[2], "Other shop"),
        ];
        let result = match_ledger(
            &[line(10, -2_000, "Shop")],
            &candidates,
            3,
            november(),
            &[(0, 1)],
        );
        assert_eq!(result.matched, vec![(0, 1)]);
    }

    #[test]
    fn a_leftover_copy_of_a_matched_entry_is_a_duplicate() {
        let candidates = [
            candidate(10, -2_000, SplitKind::Withdrawal, &[1], "Shop"),
            candidate(11, -2_000, SplitKind::Withdrawal, &[2], "typed by hand"),
        ];
        let result = match_ledger(&[line(10, -2_000, "Shop")], &candidates, 3, november(), &[]);
        assert_eq!(
            result.findings,
            vec![Finding::DuplicateOf {
                candidate: 1,
                of: 0
            }]
        );
    }

    #[test]
    fn identical_leftovers_are_duplicates_of_each_other_and_others_are_not_on_the_statement() {
        let candidates = [
            candidate(10, -2_000, SplitKind::Withdrawal, &[1], "Gym"),
            candidate(10, -2_000, SplitKind::Withdrawal, &[2], "GYM "),
            candidate(15, -700, SplitKind::Withdrawal, &[3], "Cinema"),
        ];
        let result = match_ledger(&[], &candidates, 3, november(), &[]);
        assert_eq!(
            result.findings,
            vec![
                Finding::Identical {
                    candidates: vec![0, 1]
                },
                Finding::NotOnStatement { candidate: 2 },
            ]
        );
    }

    #[test]
    fn leftovers_outside_the_range_are_not_reported() {
        let candidates = [candidate(30, -2_000, SplitKind::Withdrawal, &[1], "Shop")];
        let range = DateRange::new(date(1), date(27)).unwrap();
        let result = match_ledger(&[], &candidates, 3, range, &[]);
        assert!(result.findings.is_empty());
    }

    #[test]
    fn an_unmatched_group_is_reported_once_not_split_by_split() {
        let splits = [
            split(1, 1, 3, 2_000, CHECKING, GROCERIES),
            split(2, 1, 3, 500, CHECKING, GROCERIES),
        ];
        let candidates = ledger_candidates(&splits, id(CHECKING));
        let result = match_ledger(&[], &candidates, 3, november(), &[]);
        assert_eq!(
            result.findings,
            vec![Finding::NotOnStatement { candidate: 2 }]
        );
    }

    #[test]
    fn normalise_is_used_for_identical_descriptions() {
        assert_eq!(normalise("GYM "), normalise("gym"));
    }

    #[test]
    fn tokens_drive_similarity() {
        assert!(jaccard(&tokens("Bakery Rossi"), &tokens("BAKERY ROSSI")) > 0);
    }

    #[test]
    fn groups_mixing_kinds_or_currencies_are_not_combined() {
        let mut usd_split = split(2, 1, 3, 500, CHECKING, GROCERIES);
        usd_split.amount = crate::test_support::usd(500);
        let mut transfer = split(4, 2, 3, 500, CHECKING, GROCERIES);
        transfer.kind = SplitKind::Transfer;
        let splits = [
            split(1, 1, 3, 2_000, CHECKING, GROCERIES),
            usd_split,
            split(3, 2, 3, 2_000, CHECKING, GROCERIES),
            transfer,
        ];
        assert_eq!(ledger_candidates(&splits, id(CHECKING)).len(), 4);
    }

    #[test]
    fn the_window_edge_is_inclusive_and_zero_means_the_same_day() {
        let candidates = [candidate(10, -2_000, SplitKind::Withdrawal, &[1], "x")];
        let at = |day| [line(day, -2_000, "x")];
        assert_eq!(
            match_ledger(&at(13), &candidates, 3, november(), &[]).matched,
            vec![(0, 0)]
        );
        assert!(
            match_ledger(&at(14), &candidates, 3, november(), &[])
                .matched
                .is_empty()
        );
        assert!(
            match_ledger(&at(11), &candidates, 0, november(), &[])
                .matched
                .is_empty()
        );
        assert_eq!(
            match_ledger(&at(10), &candidates, 0, november(), &[]).matched,
            vec![(0, 0)]
        );
    }

    #[test]
    fn another_currency_never_matches() {
        let candidates = [candidate(10, -2_000, SplitKind::Withdrawal, &[1], "x")];
        let mut dollars = line(10, 0, "x");
        dollars.amount = crate::test_support::usd(-2_000);
        assert!(
            match_ledger(&[dollars], &candidates, 3, november(), &[])
                .matched
                .is_empty()
        );
    }

    #[test]
    fn a_line_with_its_own_window_uses_it_instead() {
        // A Satispay top-up the bank books five days after the Satispay row
        // (decision 10): outside ±3, inside −2 to +7.
        let candidates = [candidate(10, -2_000, SplitKind::Transfer, &[1], "Satispay")];
        let mut top_up = line(15, -2_000, "SATISPAY");
        assert!(
            match_ledger(&[top_up.clone()], &candidates, 3, november(), &[])
                .matched
                .is_empty()
        );
        top_up.window = Some(LagWindow {
            early_days: 2,
            late_days: 7,
        });
        assert_eq!(
            match_ledger(&[top_up.clone()], &candidates, 3, november(), &[]).matched,
            vec![(0, 0)]
        );
        // Three days *before* the candidate is outside that window.
        top_up.date = date(7);
        assert!(
            match_ledger(&[top_up], &candidates, 3, november(), &[])
                .matched
                .is_empty()
        );
    }

    #[test]
    fn pins_naming_an_unknown_line_or_candidate_are_dropped() {
        let candidates = [candidate(10, -2_000, SplitKind::Withdrawal, &[1], "x")];
        let lines = [line(10, -2_000, "x")];
        let result = match_ledger(&lines, &candidates, 3, november(), &[(5, 0), (0, 7)]);
        assert_eq!(result.matched, vec![(0, 0)]);
    }

    #[test]
    fn a_pin_is_kept_whatever_the_amount_and_date() {
        // A remembered link is the owner's decision.
        let candidates = [candidate(25, -9_999, SplitKind::Withdrawal, &[1], "x")];
        let result = match_ledger(
            &[line(2, -2_000, "x")],
            &candidates,
            3,
            november(),
            &[(0, 0)],
        );
        assert_eq!(result.matched, vec![(0, 0)]);
    }

    #[test]
    fn a_pinned_split_blocks_its_group() {
        let splits = [
            split(1, 1, 3, 2_000, CHECKING, GROCERIES),
            split(2, 1, 3, 500, CHECKING, GROCERIES),
        ];
        let candidates = ledger_candidates(&splits, id(CHECKING));
        let lines = [line(3, -2_500, "group"), line(3, -2_000, "pinned")];
        let result = match_ledger(&lines, &candidates, 3, november(), &[(1, 0)]);
        assert_eq!(result.matched, vec![(1, 0)]);
        assert_eq!(result.unmatched_lines, vec![0]);
    }

    #[test]
    fn a_leftover_is_a_duplicate_only_with_the_same_kind_and_inside_the_window() {
        let candidates = [
            candidate(10, -2_000, SplitKind::Withdrawal, &[1], "Shop"),
            candidate(13, -2_000, SplitKind::Withdrawal, &[2], "a"),
            candidate(14, -2_000, SplitKind::Withdrawal, &[3], "b"),
            candidate(10, -2_000, SplitKind::Transfer, &[4], "c"),
        ];
        let result = match_ledger(&[line(10, -2_000, "Shop")], &candidates, 3, november(), &[]);
        assert_eq!(
            result.findings,
            vec![
                Finding::DuplicateOf {
                    candidate: 1,
                    of: 0
                },
                Finding::NotOnStatement { candidate: 2 },
                Finding::NotOnStatement { candidate: 3 },
            ]
        );
    }

    #[test]
    fn same_text_and_amount_on_different_days_are_not_identical() {
        let candidates = [
            candidate(10, -2_000, SplitKind::Withdrawal, &[1], "Gym"),
            candidate(11, -2_000, SplitKind::Withdrawal, &[2], "Gym"),
        ];
        let result = match_ledger(&[], &candidates, 3, november(), &[]);
        assert_eq!(
            result.findings,
            vec![
                Finding::NotOnStatement { candidate: 0 },
                Finding::NotOnStatement { candidate: 1 },
            ]
        );
    }

    #[test]
    fn leftovers_on_the_range_edges_are_reported() {
        let candidates = [
            candidate(1, -100, SplitKind::Withdrawal, &[1], "first"),
            candidate(30, -200, SplitKind::Withdrawal, &[2], "last"),
        ];
        let result = match_ledger(&[], &candidates, 3, november(), &[]);
        assert_eq!(result.findings.len(), 2);
    }
}
