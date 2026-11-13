//! `/v1/tags` → `tags`.

use saldo_app::ctx::Origin;
use saldo_app::tags::{NewTag, TagUpdate};
use saldo_core::TagId;

use crate::engine::{ImportCx, ImportError};
use crate::firefly::models::Tag;
use crate::report::ImportReport;

const ENTITY: &str = "tags";
const REF: &str = "tag";

pub async fn import_tags(cx: &ImportCx, report: &mut ImportReport) -> Result<(), ImportError> {
    let categories = cx.firefly.list::<Tag>("/v1/tags", &[]).await?;
    let mut guard = cx.state.lock().await;
    // Borrow the two fields separately so both can be used mutably at once.
    let state = &mut *guard;
    let service = &cx.app.tags;
    for item in categories {
        let name = item.attributes.tag.trim().to_string();
        let description = item.attributes.description;
        let existing = state
            .refs
            .resolve(REF, &item.id)
            .and_then(|id| id.parse::<TagId>().ok());
        let current = match existing {
            Some(id) => service.get_in(&mut state.tx, &cx.ctx, id).await.ok(),
            None => None,
        };
        let result = match current {
            Some(view) if view.name == name && view.description == description => {
                report.unchanged(ENTITY);
                continue;
            }
            Some(view) => {
                let update = TagUpdate {
                    name: Some(name.clone()),
                    description: Some(description),
                };
                let id = view
                    .id
                    .parse()
                    .map_err(|_| ImportError::Invalid(format!("bad id {}", view.id)))?;
                service
                    .update_in(&mut state.tx, &cx.ctx, Origin::Import, id, update)
                    .await
                    .map(|v| (v.id, false))
            }
            None => service
                .create_in(
                    &mut state.tx,
                    &cx.ctx,
                    Origin::Import,
                    NewTag {
                        name: name.clone(),
                        description,
                    },
                )
                .await
                .map(|v| (v.id, true)),
        };
        match result {
            Ok((local_id, created)) => {
                state
                    .refs
                    .upsert(&mut state.tx, REF, &item.id, &local_id, &cx.now())
                    .await?;
                if created {
                    report.created(ENTITY)
                } else {
                    report.updated(ENTITY)
                }
            }
            Err(error) => report.skipped(ENTITY, format!("{name:?}: {}", super::skippable(error)?)),
        }
    }
    Ok(())
}
