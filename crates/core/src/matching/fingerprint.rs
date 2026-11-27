//! A stable identity for a statement line whose bank gives no ID (L17).

use std::collections::HashMap;

use chrono::NaiveDate;
use sha2::{Digest, Sha256};

use crate::ids::AccountId;
use crate::matching::text::normalise;
use crate::money::Money;

/// SHA-256 (hex) of `account | date | amount | currency | normalised text |
/// occurrence`. `occurrence` counts the identical lines before this one in
/// the file (see [`occurrences`]), so two identical coffees on one day get
/// different fingerprints while an overlapping export of whole days gives
/// the same ones again.
pub fn fingerprint(
    account: AccountId,
    date: NaiveDate,
    amount: Money,
    text: &str,
    occurrence: u32,
) -> String {
    let base = format!(
        "{account}|{date}|{}|{}|{}|{occurrence}",
        amount.minor(),
        amount.currency(),
        normalise(text)
    );
    let digest = Sha256::digest(base.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// For each line (`date`, `amount`, `text`), how many earlier lines in the
/// slice are identical once the text is normalised.
pub fn occurrences(lines: &[(NaiveDate, Money, &str)]) -> Vec<u32> {
    // Like PHP's `$seen[$key] = ($seen[$key] ?? 0) + 1`: the `entry` API
    // finds the slot for a key, inserting 0 the first time.
    let mut seen: HashMap<(NaiveDate, Money, String), u32> = HashMap::new();
    lines
        .iter()
        .map(|(date, amount, text)| {
            let count = seen.entry((*date, *amount, normalise(text))).or_insert(0);
            let before = *count;
            *count += 1;
            before
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{CHECKING, SAVINGS, date, eur, id};
    use proptest::prelude::*;

    #[test]
    fn the_fingerprint_format_never_changes() {
        // Fingerprints are stored: changing the format would turn every
        // re-import into new lines. This value is SHA-256 of
        // "00000000-0000-0000-0000-00000000000a|2026-11-03|-450|EUR|coffee bar|0".
        assert_eq!(
            fingerprint(id(CHECKING), date(3), eur(-450), " Coffee  BAR", 0),
            "506a97d730127b5b3944ebc8325eea3fa672100971d300c9911cccfaea746afd"
        );
    }

    #[test]
    fn a_fingerprint_is_64_hex_characters() {
        let print = fingerprint(id(CHECKING), date(3), eur(-450), "Coffee", 0);
        assert_eq!(print.len(), 64);
        assert!(print.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn whitespace_and_case_do_not_change_the_fingerprint() {
        let a = fingerprint(id(CHECKING), date(3), eur(-450), "  COFFEE  bar", 0);
        let b = fingerprint(id(CHECKING), date(3), eur(-450), "coffee bar", 0);
        assert_eq!(a, b);
    }

    #[test]
    fn every_field_changes_the_fingerprint() {
        let base = fingerprint(id(CHECKING), date(3), eur(-450), "coffee", 0);
        let others = [
            fingerprint(id(SAVINGS), date(3), eur(-450), "coffee", 0),
            fingerprint(id(CHECKING), date(4), eur(-450), "coffee", 0),
            fingerprint(id(CHECKING), date(3), eur(-451), "coffee", 0),
            fingerprint(
                id(CHECKING),
                date(3),
                crate::test_support::usd(-450),
                "coffee",
                0,
            ),
            fingerprint(id(CHECKING), date(3), eur(-450), "tea", 0),
            fingerprint(id(CHECKING), date(3), eur(-450), "coffee", 1),
        ];
        for other in others {
            assert_ne!(base, other);
        }
    }

    #[test]
    fn identical_lines_are_counted_in_order() {
        let lines = [
            (date(3), eur(-450), "Coffee"),
            (date(3), eur(-450), "COFFEE "),
            (date(3), eur(-900), "Coffee"),
            (date(4), eur(-450), "Coffee"),
            (date(3), eur(-450), "coffee"),
        ];
        assert_eq!(occurrences(&lines), vec![0, 1, 0, 0, 2]);
    }

    proptest! {
        // Shuffling whole days (an export that starts earlier or later, or
        // lists days in another order) keeps every line's fingerprint.
        #[test]
        fn fingerprints_do_not_depend_on_the_order_of_days(
            days in proptest::collection::vec(
                proptest::collection::vec((1i64..4, prop::sample::select(vec!["a", "b"])), 1..4),
                1..6,
            ),
            rotate in 0usize..6,
        ) {
            let lines_of = |order: &[usize]| -> Vec<(NaiveDate, Money, &str)> {
                order
                    .iter()
                    .flat_map(|&d| {
                        days[d].iter().map(move |(amount, text)| {
                            (date(1 + d as u32), eur(-*amount), *text)
                        })
                    })
                    .collect()
            };
            let forward: Vec<usize> = (0..days.len()).collect();
            let mut shuffled = forward.clone();
            shuffled.rotate_left(rotate % days.len());

            let prints = |lines: Vec<(NaiveDate, Money, &str)>| {
                let counts = occurrences(&lines);
                let mut out: Vec<String> = lines
                    .iter()
                    .zip(counts)
                    .map(|((d, m, t), n)| fingerprint(id(CHECKING), *d, *m, t, n))
                    .collect();
                out.sort();
                out
            };
            prop_assert_eq!(prints(lines_of(&forward)), prints(lines_of(&shuffled)));
        }

        // Identical lines on one day never share a fingerprint.
        #[test]
        fn identical_lines_get_distinct_fingerprints(count in 1usize..8) {
            let lines: Vec<(NaiveDate, Money, &str)> =
                (0..count).map(|_| (date(5), eur(-450), "coffee")).collect();
            let counts = occurrences(&lines);
            let mut prints: Vec<String> = counts
                .iter()
                .map(|n| fingerprint(id(CHECKING), date(5), eur(-450), "coffee", *n))
                .collect();
            prints.sort();
            prints.dedup();
            prop_assert_eq!(prints.len(), count);
        }
    }
}
