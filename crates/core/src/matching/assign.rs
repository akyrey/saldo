//! The ranked assignment every matcher shares (L11, L14).

use std::cmp::Reverse;
use std::collections::BTreeSet;

/// One way a line could match a candidate. Lower `score` is better.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pair {
    /// Position of the statement line.
    pub line: usize,
    /// Position of the candidate (a split, a group, an order, a PayPal row).
    pub candidate: usize,
    pub score: i64,
}

/// Picks pairs so that no line is matched twice and no *resource* is used
/// twice. `uses[c]` lists the resources candidate `c` takes: for ledger
/// candidates the splits (a group takes all its splits, so a group and one
/// of its own splits never both match); for orders or PayPal rows simply
/// `vec![c]`.
///
/// `pinned` pairs (remembered links) are taken first, in order; one that
/// clashes with an earlier pin is dropped. Then the pairs are sorted by
/// score, a candidate taking more resources first on a tie (a whole group
/// beats one of its splits), then line and candidate position, and taken
/// greedily. The result is sorted by line.
pub fn assign(
    pairs: &[Pair],
    uses: &[Vec<usize>],
    pinned: &[(usize, usize)],
) -> Vec<(usize, usize)> {
    let mut taken = Taken::default();
    for &(line, candidate) in pinned {
        taken.take(line, candidate, uses);
    }

    let mut ranked = pairs.to_vec();
    // Sorting by a tuple compares field by field, like PHP's
    // `[$a->score, ...] <=> [$b->score, ...]`. `Reverse` flips one field,
    // so more resources sort first.
    ranked.sort_by_key(|p| {
        let size = uses.get(p.candidate).map_or(0, Vec::len);
        (p.score, Reverse(size), p.line, p.candidate)
    });
    for pair in ranked {
        taken.take(pair.line, pair.candidate, uses);
    }

    let mut result = taken.pairs;
    result.sort_unstable();
    result
}

/// What has been assigned so far.
#[derive(Default)]
struct Taken {
    lines: BTreeSet<usize>,
    resources: BTreeSet<usize>,
    pairs: Vec<(usize, usize)>,
}

impl Taken {
    /// Records the pair unless its line or one of its resources is taken, or
    /// the candidate is unknown.
    fn take(&mut self, line: usize, candidate: usize, uses: &[Vec<usize>]) {
        let Some(resources) = uses.get(candidate) else {
            return;
        };
        if self.lines.contains(&line) || resources.iter().any(|r| self.resources.contains(r)) {
            return;
        }
        self.lines.insert(line);
        self.resources.extend(resources.iter().copied());
        self.pairs.push((line, candidate));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn pair(line: usize, candidate: usize, score: i64) -> Pair {
        Pair {
            line,
            candidate,
            score,
        }
    }

    fn singles(count: usize) -> Vec<Vec<usize>> {
        (0..count).map(|c| vec![c]).collect()
    }

    #[test]
    fn the_best_pair_wins_and_the_loser_takes_the_next_best() {
        // Two equal payments a month apart: each line gets its own candidate,
        // not the same one twice.
        let pairs = [
            pair(0, 0, 0),
            pair(0, 1, 30_000),
            pair(1, 0, 29_000),
            pair(1, 1, 1_000),
        ];
        assert_eq!(assign(&pairs, &singles(2), &[]), vec![(0, 0), (1, 1)]);
    }

    #[test]
    fn not_first_fit_but_best_first() {
        // First fit would give line 0 candidate 0 and leave line 1 without
        // a match; ranking gives both a match.
        let pairs = [pair(0, 0, 2_000), pair(0, 1, 3_000), pair(1, 0, 0)];
        assert_eq!(assign(&pairs, &singles(2), &[]), vec![(0, 1), (1, 0)]);
    }

    #[test]
    fn a_candidate_sharing_a_resource_with_a_taken_one_is_skipped() {
        // Candidate 0 is a group of splits 0 and 1; candidates 1 and 2 are
        // those splits. Line 0 takes the group, so line 1 cannot take split 1.
        let uses = vec![vec![0, 1], vec![0], vec![1]];
        let pairs = [pair(0, 0, 0), pair(1, 2, 0)];
        assert_eq!(assign(&pairs, &uses, &[]), vec![(0, 0)]);
    }

    #[test]
    fn splits_of_a_group_can_match_separate_lines() {
        let uses = vec![vec![0, 1], vec![0], vec![1]];
        let pairs = [pair(0, 1, 0), pair(1, 2, 0)];
        assert_eq!(assign(&pairs, &uses, &[]), vec![(0, 1), (1, 2)]);
    }

    #[test]
    fn on_a_tie_the_candidate_using_more_resources_wins() {
        let uses = vec![vec![0], vec![0, 1]];
        let pairs = [pair(0, 0, 500), pair(0, 1, 500)];
        assert_eq!(assign(&pairs, &uses, &[]), vec![(0, 1)]);
    }

    #[test]
    fn ties_are_broken_by_position() {
        let pairs = [pair(1, 1, 0), pair(0, 1, 0), pair(0, 0, 0), pair(1, 0, 0)];
        assert_eq!(assign(&pairs, &singles(2), &[]), vec![(0, 0), (1, 1)]);
    }

    #[test]
    fn pinned_pairs_come_first_and_clashing_pins_are_dropped() {
        let pairs = [pair(0, 0, 0), pair(1, 1, 0)];
        let pinned = [(0, 1), (1, 1), (5, 9)];
        // (0, 1) is pinned; (1, 1) clashes with it; (5, 9) names an unknown
        // candidate and is dropped too. Line 1 still gets nothing better
        // than candidate 1, which is taken.
        assert_eq!(assign(&pairs, &singles(2), &pinned), vec![(0, 1)]);
    }

    proptest! {
        #[test]
        fn no_line_or_resource_is_used_twice_and_pins_are_kept(
            raw in proptest::collection::vec((0usize..6, 0usize..6, -5i64..5), 0..30),
            pins in proptest::collection::vec((0usize..6, 0usize..6), 0..3),
        ) {
            // Candidate c uses resources c/2 and c%3, so candidates overlap.
            let uses: Vec<Vec<usize>> = (0..6).map(|c| vec![c / 2, 3 + c % 3]).collect();
            let pairs: Vec<Pair> = raw.iter().map(|&(l, c, s)| pair(l, c, s)).collect();
            let result = assign(&pairs, &uses, &pins);

            let mut lines = BTreeSet::new();
            let mut resources = BTreeSet::new();
            for (line, candidate) in &result {
                prop_assert!(lines.insert(*line));
                for r in &uses[*candidate] {
                    prop_assert!(resources.insert(*r));
                }
            }
            // The first pin always survives.
            if let Some(first) = pins.first() {
                prop_assert!(result.contains(first));
            }
        }

        // The same pairs in any order give the same result, so a re-run of
        // an import proposes the same matches.
        #[test]
        fn the_result_does_not_depend_on_the_order_of_the_pairs(
            raw in proptest::collection::vec((0usize..6, 0usize..6, -5i64..5), 0..30),
            rotate in 0usize..30,
        ) {
            let uses: Vec<Vec<usize>> = (0..6).map(|c| vec![c / 2, 3 + c % 3]).collect();
            let pairs: Vec<Pair> = raw.iter().map(|&(l, c, s)| pair(l, c, s)).collect();
            let mut shuffled = pairs.clone();
            shuffled.reverse();
            if !shuffled.is_empty() {
                let by = rotate % shuffled.len();
                shuffled.rotate_left(by);
            }
            prop_assert_eq!(assign(&pairs, &uses, &[]), assign(&shuffled, &uses, &[]));
        }
    }
}
