//! Finding tags whose names differ only in case ("Food", "food", "FOOD").

use std::collections::BTreeMap;

use serde::Serialize;

use super::TagService;
use crate::ctx::HouseholdCtx;
use crate::error::AppError;

/// A tag with the number of splits that carry it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct TagUsageView {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub split_count: i64,
}

/// Two or more tags with the same name ignoring case, most used first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct TagDuplicateGroup {
    pub tags: Vec<TagUsageView>,
}

/// The form two names are compared in: trimmed and lowercased. Rust's
/// `to_lowercase` follows Unicode, so "CITTÀ" and "città" match (SQLite's
/// `lower()` and `NOCASE` only fold ASCII letters).
pub fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

impl TagService {
    /// Every group of tags whose names differ only in case, ordered by name.
    pub async fn case_duplicates(
        &self,
        ctx: &HouseholdCtx,
    ) -> Result<Vec<TagDuplicateGroup>, AppError> {
        let household_id = ctx.household_id.to_string();
        // The `"name!: type"` aliases tell SQLx's compile-time check the
        // column's nullability and type, which it cannot infer through a JOIN.
        let usage = sqlx::query_as!(
            TagUsageView,
            r#"SELECT t.id AS "id!", t.name AS "name!", t.description,
                      count(st.split_id) AS "split_count!: i64"
               FROM tags t LEFT JOIN split_tags st ON st.tag_id = t.id
               WHERE t.household_id = ?
               GROUP BY t.id"#,
            household_id,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(group_by_case(usage))
    }
}

/// Groups tags by [`name_key`], keeping only groups with more than one tag.
fn group_by_case(tags: Vec<TagUsageView>) -> Vec<TagDuplicateGroup> {
    // A BTreeMap keeps its keys sorted, like a PHP array after `ksort`.
    let mut by_key: BTreeMap<String, Vec<TagUsageView>> = BTreeMap::new();
    for tag in tags {
        by_key.entry(name_key(&tag.name)).or_default().push(tag);
    }
    by_key
        .into_values()
        .filter(|group| group.len() > 1)
        .map(|group| TagDuplicateGroup {
            tags: most_used_first(group),
        })
        .collect()
}

fn most_used_first(mut tags: Vec<TagUsageView>) -> Vec<TagUsageView> {
    tags.sort_by(|a, b| {
        b.split_count
            .cmp(&a.split_count)
            .then_with(|| a.name.cmp(&b.name))
    });
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(name: &str, split_count: i64) -> TagUsageView {
        TagUsageView {
            id: name.to_string(),
            name: name.to_string(),
            description: None,
            split_count,
        }
    }

    #[test]
    fn keys_ignore_case_and_surrounding_spaces() {
        assert_eq!(name_key(" Città "), name_key("CITTÀ"));
        assert_ne!(name_key("Food"), name_key("Foods"));
    }

    #[test]
    fn single_names_are_not_duplicates() {
        let groups = group_by_case(vec![usage("Rent", 3), usage("Food", 1), usage("FOOD", 4)]);
        assert_eq!(
            groups,
            vec![TagDuplicateGroup {
                tags: vec![usage("FOOD", 4), usage("Food", 1)]
            }]
        );
    }

    #[test]
    fn equal_use_falls_back_to_name_order() {
        let groups = group_by_case(vec![usage("b", 0), usage("B", 0)]);
        assert_eq!(groups[0].tags, vec![usage("B", 0), usage("b", 0)]);
    }
}
