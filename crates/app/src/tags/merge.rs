//! Merging tags: every reference to a source tag moves to the target tag, then
//! the source is deleted. A tag ID is stored in seven places: `split_tags`,
//! recurrence splits (JSON), transaction templates (JSON), rule triggers and
//! actions, attachments, and Firefly import references.

use std::collections::HashSet;

use saldo_core::TagId;
use serde::Serialize;
use serde_json::Value;
use sqlx::SqliteConnection;

use super::{TagService, TagView, find};
use crate::clock::format_timestamp;
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::AppError;
use crate::events::{NewEvent, append_event};

/// Fold `sources` into `into`.
#[derive(Debug, Clone)]
pub struct TagMerge {
    pub into: TagId,
    pub sources: Vec<TagId>,
}

/// The data of a `tag.merged` event, one per source tag.
#[derive(Serialize)]
struct Merged<'a> {
    source: &'a TagView,
    into: &'a TagView,
    /// Splits that carried the source tag.
    splits: i64,
}

impl TagService {
    /// Runs every merge in one transaction and returns the target tags. The
    /// target keeps its name and takes a source's description if it has none.
    /// One `tag.merged` event per source covers all the moved references.
    pub async fn merge(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        merges: Vec<TagMerge>,
    ) -> Result<Vec<TagView>, AppError> {
        check(&merges)?;
        let mut tx = crate::db::begin(&self.pool).await?;
        let mut targets = Vec::with_capacity(merges.len());
        for merge in &merges {
            targets.push(self.merge_one(&mut tx, ctx, origin, merge).await?);
        }
        tx.commit().await?;
        Ok(targets)
    }

    async fn merge_one(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        merge: &TagMerge,
    ) -> Result<TagView, AppError> {
        let target_id = merge.into.to_string();
        find(&mut *conn, ctx, &target_id).await?;
        for source_id in &merge.sources {
            let source = find(&mut *conn, ctx, &source_id.to_string()).await?;
            let splits = move_references(&mut *conn, ctx, &source.id, &target_id).await?;
            self.absorb(&mut *conn, ctx, &source, &target_id).await?;
            let target = find(&mut *conn, ctx, &target_id).await?;
            let data = Merged {
                source: &source,
                into: &target,
                splits,
            };
            let event = NewEvent::new(&*self.clock, "tag", "merged", source.id.clone(), &data)?;
            append_event(&mut *conn, ctx, origin, event).await?;
        }
        find(&mut *conn, ctx, &target_id).await
    }

    /// Gives the target the source's description if it has none, then
    /// deletes the source (its `split_tags` rows go with it).
    async fn absorb(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        source: &TagView,
        target_id: &str,
    ) -> Result<(), AppError> {
        let (household_id, now) = (
            ctx.household_id.to_string(),
            format_timestamp(self.clock.now()),
        );
        sqlx::query!(
            "UPDATE tags SET description = coalesce(description, ?), updated_at = ?
             WHERE household_id = ? AND id = ?",
            source.description,
            now,
            household_id,
            target_id,
        )
        .execute(&mut *conn)
        .await?;
        sqlx::query!(
            "DELETE FROM tags WHERE household_id = ? AND id = ?",
            household_id,
            source.id,
        )
        .execute(&mut *conn)
        .await?;
        Ok(())
    }
}

/// Each merge names at least one source, and no tag appears twice (which
/// also rules out merging a tag into itself).
fn check(merges: &[TagMerge]) -> Result<(), AppError> {
    if merges.is_empty() {
        return Err(AppError::invalid("merges", "name at least one merge"));
    }
    let mut seen = HashSet::new();
    for merge in merges {
        if merge.sources.is_empty() {
            return Err(AppError::invalid(
                "sources",
                "name at least one tag to merge",
            ));
        }
        // `insert` returns false when the value was already in the set.
        let all_new = std::iter::once(&merge.into)
            .chain(&merge.sources)
            .all(|id| seen.insert(*id));
        if !all_new {
            return Err(AppError::invalid(
                "sources",
                "each tag may appear only once",
            ));
        }
    }
    Ok(())
}

/// Points every reference to `source` at `target`; returns how many splits
/// carried `source`.
async fn move_references(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    source: &str,
    target: &str,
) -> Result<i64, AppError> {
    let splits = move_split_tags(&mut *conn, source, target).await?;
    move_recurrence_tags(&mut *conn, ctx, source, target).await?;
    move_template_tags(&mut *conn, ctx, source, target).await?;
    move_plain_references(&mut *conn, ctx, source, target).await?;
    Ok(splits)
}

/// The source tag was checked to belong to the household, so its
/// `split_tags` rows do too. `OR IGNORE` skips splits that have both tags.
async fn move_split_tags(
    conn: &mut SqliteConnection,
    source: &str,
    target: &str,
) -> Result<i64, AppError> {
    let splits = sqlx::query_scalar!("SELECT count(*) FROM split_tags WHERE tag_id = ?", source)
        .fetch_one(&mut *conn)
        .await?;
    sqlx::query!(
        "INSERT OR IGNORE INTO split_tags (split_id, tag_id)
         SELECT split_id, ? FROM split_tags WHERE tag_id = ?",
        target,
        source,
    )
    .execute(&mut *conn)
    .await?;
    Ok(splits)
}

/// Rewrites the JSON array of tag IDs, dropping a duplicate target.
async fn move_recurrence_tags(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    source: &str,
    target: &str,
) -> Result<(), AppError> {
    let household_id = ctx.household_id.to_string();
    sqlx::query!(
        "UPDATE recurrence_splits SET tags = (
             SELECT json_group_array(DISTINCT CASE WHEN value = ? THEN ? ELSE value END)
             FROM json_each(recurrence_splits.tags))
         WHERE household_id = ?
           AND EXISTS (SELECT 1 FROM json_each(recurrence_splits.tags) WHERE value = ?)",
        source,
        target,
        household_id,
        source,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Templates keep tag IDs inside each split object, so the JSON is rewritten
/// in Rust rather than in SQL.
async fn move_template_tags(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    source: &str,
    target: &str,
) -> Result<(), AppError> {
    let household_id = ctx.household_id.to_string();
    let rows = sqlx::query!(
        "SELECT id, splits FROM transaction_templates
         WHERE household_id = ? AND instr(splits, ?) > 0",
        household_id,
        source,
    )
    .fetch_all(&mut *conn)
    .await?;
    for row in rows {
        let splits: Value = serde_json::from_str(&row.splits)
            .map_err(|e| AppError::Internal(format!("corrupt template splits: {e}")))?;
        let rewritten = replace_in_splits(splits, source, target).to_string();
        sqlx::query!(
            "UPDATE transaction_templates SET splits = ? WHERE household_id = ? AND id = ?",
            rewritten,
            household_id,
            row.id,
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Rule values, attachments and import references hold the ID as plain text.
async fn move_plain_references(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    source: &str,
    target: &str,
) -> Result<(), AppError> {
    let household_id = ctx.household_id.to_string();
    sqlx::query!(
        "UPDATE rule_triggers SET value = ? WHERE kind = 'tag_is' AND value = ?
           AND rule_id IN (SELECT id FROM rules WHERE household_id = ?)",
        target,
        source,
        household_id,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "UPDATE rule_actions SET value = ? WHERE kind IN ('add_tag', 'remove_tag') AND value = ?
           AND rule_id IN (SELECT id FROM rules WHERE household_id = ?)",
        target,
        source,
        household_id,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "UPDATE attachments SET attachable_id = ?
         WHERE household_id = ? AND attachable_type = 'tag' AND attachable_id = ?",
        target,
        household_id,
        source,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "UPDATE external_refs SET local_id = ?
         WHERE household_id = ? AND entity_type = 'tag' AND local_id = ?",
        target,
        household_id,
        source,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// A copy of a template's splits with `source` replaced by `target` in each
/// split's `tag_ids`, without duplicates.
fn replace_in_splits(splits: Value, source: &str, target: &str) -> Value {
    match splits {
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|split| replace_in_split(split, source, target))
                .collect(),
        ),
        other => other,
    }
}

fn replace_in_split(split: Value, source: &str, target: &str) -> Value {
    let Value::Object(mut fields) = split else {
        return split;
    };
    if let Some(Value::Array(ids)) = fields.remove("tag_ids") {
        let mut replaced: Vec<Value> = Vec::with_capacity(ids.len());
        for id in ids {
            let id = if id == source {
                Value::from(target)
            } else {
                id
            };
            if !replaced.contains(&id) {
                replaced.push(id);
            }
        }
        fields.insert("tag_ids".to_string(), Value::Array(replaced));
    }
    Value::Object(fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn tag(n: u128) -> TagId {
        TagId(Uuid::from_u128(n))
    }

    #[test]
    fn templates_get_the_target_once_and_keep_other_fields() {
        let splits = json!([
            { "tag_ids": ["s", "t", "x"], "amount": "3" },
            { "tag_ids": ["x"] },
            { "amount": "1" }
        ]);
        assert_eq!(
            replace_in_splits(splits, "s", "t"),
            json!([
                { "tag_ids": ["t", "x"], "amount": "3" },
                { "tag_ids": ["x"] },
                { "amount": "1" }
            ])
        );
        assert_eq!(replace_in_splits(json!({}), "s", "t"), json!({}));
    }

    #[test]
    fn merges_need_sources_and_distinct_tags() {
        let merge = |into, sources| TagMerge { into, sources };
        assert!(
            check(&[
                merge(tag(1), vec![tag(2), tag(3)]),
                merge(tag(4), vec![tag(5)])
            ])
            .is_ok()
        );
        for bad in [
            vec![],
            vec![merge(tag(1), vec![])],
            vec![merge(tag(1), vec![tag(1)])],
            vec![merge(tag(1), vec![tag(2), tag(2)])],
            vec![merge(tag(1), vec![tag(2)]), merge(tag(2), vec![tag(3)])],
        ] {
            assert!(
                matches!(check(&bad), Err(AppError::Invalid { .. })),
                "{bad:?}"
            );
        }
    }
}
