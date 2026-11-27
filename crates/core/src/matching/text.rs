//! Comparing descriptions: normalising text and measuring how many words
//! two texts share (L12).

// `BTreeSet` is a sorted set: like PHP's `array_unique` on a sorted array,
// or a Go `map[string]struct{}` that iterates in order. Sorted keeps every
// result the same from one run to the next.
use std::collections::BTreeSet;

/// Similarities are integers from 0 (nothing shared) to `PER_MILLE` (all),
/// so the matchers never need floats.
pub const PER_MILLE: u32 = 1000;

/// Words shorter than this ("di", "il", "a") are too common to tell
/// transactions apart.
const MIN_TOKEN_CHARS: usize = 3;

/// Lowercase, with runs of whitespace collapsed to one space and no space at
/// either end: "  PayPal   *Netflix " becomes "paypal *netflix".
pub fn normalise(text: &str) -> String {
    // `split_whitespace` is PHP's `preg_split('/\s+/', trim($s))`; `to_lowercase`
    // is Unicode-aware like `mb_strtolower`, not ASCII-only like `strtolower`.
    text.split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The distinct lowercase words of `text` with at least three letters or
/// digits; anything else separates words.
pub fn tokens(text: &str) -> BTreeSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() >= MIN_TOKEN_CHARS)
        .map(str::to_string)
        .collect()
}

/// Shared words over all words of both texts, in per-mille: good for two
/// descriptions of similar length. 0 when either is empty.
pub fn jaccard(a: &BTreeSet<String>, b: &BTreeSet<String>) -> u32 {
    let shared = a.intersection(b).count();
    let all = a.len() + b.len() - shared;
    per_mille(shared, all)
}

/// The share of `part`'s words found in `whole`, in per-mille: good for a
/// short merchant name inside a long bank description, where Jaccard would
/// understate the match. 0 when `part` is empty.
pub fn coverage(part: &BTreeSet<String>, whole: &BTreeSet<String>) -> u32 {
    per_mille(part.intersection(whole).count(), part.len())
}

/// `count / total` in per-mille, rounded down; 0 when `total` is 0.
fn per_mille(count: usize, total: usize) -> u32 {
    if total == 0 {
        return 0;
    }
    // Word counts are small, so the conversion never actually saturates.
    let ratio = count.saturating_mul(PER_MILLE as usize) / total;
    u32::try_from(ratio).unwrap_or(PER_MILLE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(words: &[&str]) -> BTreeSet<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn normalise_lowercases_and_collapses_whitespace() {
        assert_eq!(normalise("  PayPal   *Netflix \t"), "paypal *netflix");
        assert_eq!(normalise(""), "");
    }

    #[test]
    fn normalise_lowercases_beyond_ascii() {
        assert_eq!(normalise("CAFFÈ Società"), "caffè società");
    }

    #[test]
    fn tokens_keep_words_of_three_or_more_characters() {
        assert_eq!(
            tokens("PAGAMENTO POS 12/11 Caffè di Roma"),
            set(&["pagamento", "pos", "caffè", "roma"])
        );
    }

    #[test]
    fn tokens_split_on_punctuation_and_drop_repeats() {
        assert_eq!(
            tokens("Amazon.it*AB12CD amazon 405-1234567-1234567"),
            set(&["amazon", "ab12cd", "405", "1234567"])
        );
    }

    #[test]
    fn jaccard_counts_shared_words_over_all_words() {
        let a = set(&["paypal", "netflix", "europe"]);
        let b = set(&["netflix", "europe", "subscription"]);
        // 2 shared out of 4 distinct words.
        assert_eq!(jaccard(&a, &b), 500);
        assert_eq!(jaccard(&a, &a), PER_MILLE);
        assert_eq!(jaccard(&a, &BTreeSet::new()), 0);
        assert_eq!(jaccard(&BTreeSet::new(), &BTreeSet::new()), 0);
    }

    #[test]
    fn coverage_is_the_share_of_the_short_text_found_in_the_long_one() {
        let merchant = set(&["netflix", "international"]);
        let bank = set(&["paypal", "netflix", "europe", "sarl", "pagamento"]);
        assert_eq!(coverage(&merchant, &bank), 500);
        assert_eq!(coverage(&set(&["netflix"]), &bank), PER_MILLE);
        assert_eq!(coverage(&BTreeSet::new(), &bank), 0);
    }
}
