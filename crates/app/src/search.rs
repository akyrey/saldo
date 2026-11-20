//! Text search with SQL `LIKE`. In a `LIKE` pattern `%` and `_` are
//! wildcards, so a search for "50%" would also find "500". The pattern built
//! here escapes them; the query must say `LIKE ? ESCAPE '\'`.

/// The escape character named in every `ESCAPE '\'` clause.
const ESCAPE: char = '\\';

/// A `LIKE` pattern that finds `text` anywhere, character for character
/// (SQLite still ignores ASCII case). `None` for a missing or blank search.
pub(crate) fn contains_pattern(text: Option<&str>) -> Option<String> {
    let text = text.filter(|t| !t.trim().is_empty())?;
    let mut pattern = String::with_capacity(text.len() + 2);
    pattern.push('%');
    for c in text.chars() {
        if matches!(c, '%' | '_' | ESCAPE) {
            pattern.push(ESCAPE);
        }
        pattern.push(c);
    }
    pattern.push('%');
    Some(pattern)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_and_the_escape_character_are_escaped() {
        assert_eq!(contains_pattern(Some("50%")).as_deref(), Some(r"%50\%%"));
        assert_eq!(contains_pattern(Some("a_b")).as_deref(), Some(r"%a\_b%"));
        assert_eq!(contains_pattern(Some(r"C:\x")).as_deref(), Some(r"%C:\\x%"));
        assert_eq!(contains_pattern(Some("Rossi")).as_deref(), Some("%Rossi%"));
    }

    #[test]
    fn a_missing_or_blank_search_matches_everything() {
        assert_eq!(contains_pattern(None), None);
        assert_eq!(contains_pattern(Some("   ")), None);
    }
}
