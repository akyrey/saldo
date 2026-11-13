//! Mapping between IDs in an external system (Firefly III) and local rows.
//! This is what makes imports idempotent (ARCHITECTURE.md 9.1).

use std::collections::HashMap;

use sqlx::SqliteConnection;

use crate::ctx::HouseholdCtx;
use crate::error::AppError;

/// The `source` value for Firefly III.
pub const FIREFLY: &str = "firefly-iii";

/// All references of one household and source, held in memory for the run.
/// Writes go to the database and the map together; an unchanged mapping is
/// not written at all, so a repeated import leaves the table untouched.
#[derive(Debug, Clone, Default)]
pub struct ExternalRefs {
    household_id: String,
    source: String,
    // Key: (entity type, external ID) → local ID. A tuple as a map key, like
    // a composite primary key.
    map: HashMap<(String, String), String>,
}

impl ExternalRefs {
    pub async fn load(
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        source: &str,
    ) -> Result<Self, AppError> {
        let household_id = ctx.household_id.to_string();
        let rows = sqlx::query!(
            "SELECT entity_type, external_id, local_id FROM external_refs WHERE household_id = ? AND source = ?",
            household_id,
            source,
        )
        .fetch_all(&mut *conn)
        .await?;
        let map = rows
            .into_iter()
            .map(|r| ((r.entity_type, r.external_id), r.local_id))
            .collect();
        Ok(Self {
            household_id,
            source: source.to_string(),
            map,
        })
    }

    /// The local ID for an external one, if it was imported before.
    pub fn resolve(&self, entity_type: &str, external_id: &str) -> Option<&str> {
        self.map
            .get(&(entity_type.to_string(), external_id.to_string()))
            .map(String::as_str)
    }

    /// Every (external ID, local ID) pair of one entity type.
    pub fn entries(&self, entity_type: &str) -> Vec<(String, String)> {
        self.map
            .iter()
            .filter(|((kind, _), _)| kind == entity_type)
            .map(|((_, external), local)| (external.clone(), local.clone()))
            .collect()
    }

    /// Records (or changes) a mapping. Does nothing if it is already current.
    pub async fn upsert(
        &mut self,
        conn: &mut SqliteConnection,
        entity_type: &str,
        external_id: &str,
        local_id: &str,
        now: &str,
    ) -> Result<(), AppError> {
        if self.resolve(entity_type, external_id) == Some(local_id) {
            return Ok(());
        }
        sqlx::query!(
            "INSERT INTO external_refs (household_id, source, entity_type, external_id, local_id, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (household_id, source, entity_type, external_id)
             DO UPDATE SET local_id = excluded.local_id, updated_at = excluded.updated_at",
            self.household_id,
            self.source,
            entity_type,
            external_id,
            local_id,
            now,
        )
        .execute(&mut *conn)
        .await?;
        self.map.insert(
            (entity_type.to_string(), external_id.to_string()),
            local_id.to_string(),
        );
        Ok(())
    }

    /// Forgets a mapping (the external object no longer exists).
    pub async fn remove(
        &mut self,
        conn: &mut SqliteConnection,
        entity_type: &str,
        external_id: &str,
    ) -> Result<(), AppError> {
        sqlx::query!(
            "DELETE FROM external_refs
             WHERE household_id = ? AND source = ? AND entity_type = ? AND external_id = ?",
            self.household_id,
            self.source,
            entity_type,
            external_id,
        )
        .execute(&mut *conn)
        .await?;
        self.map
            .remove(&(entity_type.to_string(), external_id.to_string()));
        Ok(())
    }
}
