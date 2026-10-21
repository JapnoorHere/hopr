//! Saved workspaces (R6): named layouts, one autosaved "last session", and an optional
//! startup workspace. The layout is opaque JSON owned by the UI.

use hopr_core::model::Workspace;
use sqlx::FromRow;

use crate::{Result, Store, StoreError, new_id, now};

#[derive(FromRow)]
struct Row {
    id: String,
    name: String,
    layout_json: String,
    is_autosave: bool,
    is_startup: bool,
    updated_at: String,
}

impl From<Row> for Workspace {
    fn from(r: Row) -> Self {
        Workspace {
            id: r.id,
            name: r.name,
            layout_json: r.layout_json,
            autosave: r.is_autosave,
            startup: r.is_startup,
            updated_at: r.updated_at,
        }
    }
}

const SELECT: &str =
    "SELECT id, name, layout_json, is_autosave, is_startup, updated_at FROM workspaces";

impl Store {
    /// Every workspace: the autosave first, then by name.
    pub async fn list_workspaces(&self) -> Result<Vec<Workspace>> {
        let rows: Vec<Row> = sqlx::query_as(&format!(
            "{SELECT} ORDER BY is_autosave DESC, name COLLATE NOCASE"
        ))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Save a named workspace, replacing one with the same name.
    pub async fn save_workspace(&self, name: &str, layout_json: &str) -> Result<Workspace> {
        let ts = now();
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT id FROM workspaces WHERE name = ? AND is_autosave = 0")
                .bind(name)
                .fetch_optional(&self.pool)
                .await?;
        let id = match existing {
            Some((id,)) => {
                sqlx::query("UPDATE workspaces SET layout_json = ?, updated_at = ? WHERE id = ?")
                    .bind(layout_json)
                    .bind(&ts)
                    .bind(&id)
                    .execute(&self.pool)
                    .await?;
                id
            }
            None => {
                let id = new_id();
                sqlx::query(
                    "INSERT INTO workspaces (id, name, layout_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(name)
                .bind(layout_json)
                .bind(&ts)
                .bind(&ts)
                .execute(&self.pool)
                .await?;
                id
            }
        };
        self.get_workspace(&id).await
    }

    /// Overwrite the single autosaved "Last session".
    pub async fn autosave_workspace(&self, layout_json: &str) -> Result<()> {
        let ts = now();
        let res = sqlx::query(
            "UPDATE workspaces SET layout_json = ?, updated_at = ? WHERE is_autosave = 1",
        )
        .bind(layout_json)
        .bind(&ts)
        .execute(&self.pool)
        .await?;
        if res.rows_affected() == 0 {
            sqlx::query(
                "INSERT INTO workspaces (id, name, layout_json, is_autosave, created_at, updated_at)
                 VALUES (?, 'Last session', ?, 1, ?, ?)",
            )
            .bind(new_id())
            .bind(layout_json)
            .bind(&ts)
            .bind(&ts)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    /// One workspace.
    pub async fn get_workspace(&self, id: &str) -> Result<Workspace> {
        let row: Option<Row> = sqlx::query_as(&format!("{SELECT} WHERE id = ?"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(Into::into).ok_or(StoreError::NotFound("workspace"))
    }

    /// Delete a named workspace (the autosave can't be deleted, only overwritten).
    pub async fn delete_workspace(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM workspaces WHERE id = ? AND is_autosave = 0")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Make `id` the startup workspace (or clear it with `None`).
    pub async fn set_startup_workspace(&self, id: Option<&str>) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE workspaces SET is_startup = 0")
            .execute(&mut *tx)
            .await?;
        if let Some(id) = id {
            sqlx::query("UPDATE workspaces SET is_startup = 1 WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::test_util::store_and_vault;

    #[tokio::test]
    async fn named_autosave_and_startup() {
        let (store, _v) = store_and_vault().await;
        store.autosave_workspace(r#"{"tabs":[1]}"#).await.unwrap();
        store.autosave_workspace(r#"{"tabs":[1,2]}"#).await.unwrap();
        let ops = store.save_workspace("Ops", r#"{"tabs":[]}"#).await.unwrap();
        let again = store
            .save_workspace("Ops", r#"{"tabs":[3]}"#)
            .await
            .unwrap();
        assert_eq!(ops.id, again.id, "same name replaces");
        let all = store.list_workspaces().await.unwrap();
        assert_eq!(all.len(), 2);
        assert!(all[0].autosave);
        assert_eq!(all[0].layout_json, r#"{"tabs":[1,2]}"#);

        store.set_startup_workspace(Some(&ops.id)).await.unwrap();
        assert!(store.get_workspace(&ops.id).await.unwrap().startup);
        store.set_startup_workspace(None).await.unwrap();
        assert!(!store.get_workspace(&ops.id).await.unwrap().startup);

        store.delete_workspace(&all[0].id).await.unwrap();
        store.delete_workspace(&ops.id).await.unwrap();
        let left = store.list_workspaces().await.unwrap();
        assert_eq!(left.len(), 1, "the autosave stays");
    }
}
