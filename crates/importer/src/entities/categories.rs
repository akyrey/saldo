//! `/v1/categories` → `categories`.

use saldo_app::categories::{CategoryUpdate, NewCategory};
use saldo_app::ctx::Origin;
use saldo_core::CategoryId;

use crate::engine::{ImportCx, ImportError};
use crate::firefly::models::Category;
use crate::report::ImportReport;

const ENTITY: &str = "categories";
const REF: &str = "category";

pub async fn import_categories(
    cx: &ImportCx,
    report: &mut ImportReport,
) -> Result<(), ImportError> {
    let categories = cx.firefly.list::<Category>("/v1/categories", &[]).await?;
    let mut guard = cx.state.lock().await;
    // Borrow the two fields separately so both can be used mutably at once.
    let state = &mut *guard;
    let service = &cx.app.categories;
    for item in categories {
        let name = item.attributes.name.trim().to_string();
        let notes = item.attributes.notes;
        let existing = state
            .refs
            .resolve(REF, &item.id)
            .and_then(|id| id.parse::<CategoryId>().ok());
        let current = match existing {
            Some(id) => service.get_in(&mut state.tx, &cx.ctx, id).await.ok(),
            None => None,
        };
        let result = match current {
            Some(view) if view.name == name && view.notes == notes => {
                report.unchanged(ENTITY);
                continue;
            }
            Some(view) => {
                let update = CategoryUpdate {
                    name: Some(name.clone()),
                    notes: Some(notes),
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
                    NewCategory {
                        name: name.clone(),
                        notes,
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
