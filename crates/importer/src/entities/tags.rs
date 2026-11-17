//! `/v1/tags` → `tags`.
//!
//! Saldo's tag names are unique ignoring case, Firefly's are not: a Firefly tag
//! whose name matches an existing tag in another case is mapped to that tag,
//! and a tag recased or merged in Saldo keeps Saldo's spelling.

use saldo_app::ctx::Origin;
use saldo_app::error::AppError;
use saldo_app::tags::{self, NewTag, TagUpdate, TagView, name_key};
use saldo_core::TagId;

use crate::engine::{ImportCx, ImportError, State};
use crate::firefly::models::Tag;
use crate::report::ImportReport;

const ENTITY: &str = "tags";
const REF: &str = "tag";

/// What importing one Firefly tag did.
enum Outcome {
    Created,
    Updated,
    Unchanged,
}

pub async fn import_tags(cx: &ImportCx, report: &mut ImportReport) -> Result<(), ImportError> {
    let items = cx.firefly.list::<Tag>("/v1/tags", &[]).await?;
    let mut guard = cx.state.lock().await;
    // Borrow the two fields separately so both can be used mutably at once.
    let state = &mut *guard;
    for item in items {
        let name = item.attributes.tag.trim().to_string();
        let description = item.attributes.description;
        match import_one(cx, state, &item.id, &name, description).await {
            Ok((local_id, outcome)) => {
                state
                    .refs
                    .upsert(&mut state.tx, REF, &item.id, &local_id, &cx.now())
                    .await?;
                match outcome {
                    Outcome::Created => report.created(ENTITY),
                    Outcome::Updated => report.updated(ENTITY),
                    Outcome::Unchanged => report.unchanged(ENTITY),
                }
            }
            Err(error) => report.skipped(ENTITY, format!("{name:?}: {}", super::skippable(error)?)),
        }
    }
    Ok(())
}

/// Updates the tag this Firefly tag was imported as, or reuses a tag with the
/// same name ignoring case, or creates one. Returns the local ID.
async fn import_one(
    cx: &ImportCx,
    state: &mut State,
    firefly_id: &str,
    name: &str,
    description: Option<String>,
) -> Result<(String, Outcome), AppError> {
    let service = &cx.app.tags;
    let known = state
        .refs
        .resolve(REF, firefly_id)
        .and_then(|id| id.parse::<TagId>().ok());
    let current = match known {
        Some(id) => service.get_in(&mut state.tx, &cx.ctx, id).await.ok(),
        None => tags::find_by_name_ignoring_case(&mut state.tx, &cx.ctx, name).await?,
    };
    let Some(view) = current else {
        let input = NewTag {
            name: name.to_string(),
            description,
        };
        let created = service
            .create_in(&mut state.tx, &cx.ctx, Origin::Import, input)
            .await?;
        return Ok((created.id, Outcome::Created));
    };
    let Some(update) = changes(&view, name, description) else {
        return Ok((view.id, Outcome::Unchanged));
    };
    let id = view
        .id
        .parse()
        .map_err(|_| AppError::Internal(format!("bad id {}", view.id)))?;
    let updated = service
        .update_in(&mut state.tx, &cx.ctx, Origin::Import, id, update)
        .await?;
    Ok((updated.id, Outcome::Updated))
}

/// What to change on `view` to match Firefly, or `None` for nothing. A name
/// that differs only in case was recased or merged in Saldo and is kept.
fn changes(view: &TagView, name: &str, description: Option<String>) -> Option<TagUpdate> {
    let same_name = name_key(&view.name) == name_key(name);
    if same_name && (view.name != name || view.description == description) {
        return None;
    }
    Some(TagUpdate {
        name: Some(name.to_string()),
        description: Some(description),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(name: &str, description: Option<&str>) -> TagView {
        TagView {
            id: "t".into(),
            name: name.into(),
            description: description.map(Into::into),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn only_real_differences_are_changes() {
        assert!(changes(&view("food", None), "food", None).is_none());
        assert!(changes(&view("Food", None), "food", Some("x".into())).is_none());
        let described = changes(&view("food", None), "food", Some("x".into())).unwrap();
        assert_eq!(described.description, Some(Some("x".into())));
        let renamed = changes(&view("food", None), "meals", None).unwrap();
        assert_eq!(renamed.name.as_deref(), Some("meals"));
    }
}
