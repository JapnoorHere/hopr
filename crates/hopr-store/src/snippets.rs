//! Snippets (R1) and the named secrets `{{secret:x}}` resolves to (R10). Secret values are
//! encrypted under the vault key and only ever decrypted in the backend.

use hopr_core::model::{RunMode, Snippet, SnippetInput};
use hopr_vault::UnlockedVault;
use secrecy::SecretString;
use sqlx::FromRow;

use crate::{Result, Store, StoreError, new_id, now};

pub(crate) fn aad(name: &str) -> Vec<u8> {
    format!("snippet.secret:{name}").into_bytes()
}

#[derive(FromRow)]
struct Row {
    id: String,
    name: String,
    folder: String,
    tags_json: String,
    description: String,
    body: String,
    run_mode: String,
    updated_at: String,
}

impl From<Row> for Snippet {
    fn from(r: Row) -> Self {
        Snippet {
            id: r.id,
            name: r.name,
            folder: r.folder,
            tags: serde_json::from_str(&r.tags_json).unwrap_or_default(),
            description: r.description,
            body: r.body,
            run_mode: RunMode::parse(&r.run_mode),
            updated_at: r.updated_at,
        }
    }
}

const SELECT: &str =
    "SELECT id, name, folder, tags_json, description, body, run_mode, updated_at FROM snippets";

/// Secret names: letters, digits, `_`, `-`, `.`.
pub fn valid_secret_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

impl Store {
    /// Every snippet, by folder then name.
    pub async fn list_snippets(&self) -> Result<Vec<Snippet>> {
        let rows: Vec<Row> = sqlx::query_as(&format!(
            "{SELECT} ORDER BY folder COLLATE NOCASE, name COLLATE NOCASE"
        ))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Create (no `id`) or update a snippet. The input must be normalized.
    pub async fn save_snippet(&self, id: Option<&str>, s: &SnippetInput) -> Result<Snippet> {
        let ts = now();
        let tags = serde_json::to_string(&s.tags)?;
        let id = match id {
            Some(id) => {
                let n = sqlx::query(
                    "UPDATE snippets SET name = ?, folder = ?, tags_json = ?, description = ?, body = ?, run_mode = ?, updated_at = ?
                     WHERE id = ?",
                )
                .bind(&s.name)
                .bind(&s.folder)
                .bind(&tags)
                .bind(&s.description)
                .bind(&s.body)
                .bind(s.run_mode.as_str())
                .bind(&ts)
                .bind(id)
                .execute(&self.pool)
                .await?
                .rows_affected();
                if n == 0 {
                    return Err(StoreError::NotFound("snippet"));
                }
                id.to_owned()
            }
            None => {
                let id = new_id();
                sqlx::query(
                    "INSERT INTO snippets (id, name, folder, tags_json, description, body, run_mode, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(&s.name)
                .bind(&s.folder)
                .bind(&tags)
                .bind(&s.description)
                .bind(&s.body)
                .bind(s.run_mode.as_str())
                .bind(&ts)
                .bind(&ts)
                .execute(&self.pool)
                .await?;
                id
            }
        };
        let row: Row = sqlx::query_as(&format!("{SELECT} WHERE id = ?"))
            .bind(&id)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.into())
    }

    /// Delete a snippet.
    pub async fn delete_snippet(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM snippets WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Names of the stored secrets (never their values).
    pub async fn secret_names(&self) -> Result<Vec<String>> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT name FROM snippet_secrets ORDER BY name COLLATE NOCASE")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|(n,)| n).collect())
    }

    /// Store (or replace) a named secret.
    pub async fn set_secret(&self, vault: &UnlockedVault, name: &str, value: &str) -> Result<()> {
        let sealed = vault.encrypt(&aad(name), value.as_bytes())?;
        sqlx::query(
            "INSERT INTO snippet_secrets (name, value_enc, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(name) DO UPDATE SET value_enc = excluded.value_enc, updated_at = excluded.updated_at",
        )
        .bind(name)
        .bind(sealed)
        .bind(now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// A secret's value, if it exists.
    pub async fn get_secret(
        &self,
        vault: &UnlockedVault,
        name: &str,
    ) -> Result<Option<SecretString>> {
        let row: Option<(Vec<u8>,)> =
            sqlx::query_as("SELECT value_enc FROM snippet_secrets WHERE name = ?")
                .bind(name)
                .fetch_optional(&self.pool)
                .await?;
        row.map(|(c,)| vault.decrypt_string(&aad(name), &c).map_err(Into::into))
            .transpose()
    }

    /// Forget a secret.
    pub async fn delete_secret(&self, name: &str) -> Result<()> {
        sqlx::query("DELETE FROM snippet_secrets WHERE name = ?")
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::store_and_vault;
    use secrecy::ExposeSecret;

    fn input(name: &str, folder: &str) -> SnippetInput {
        SnippetInput {
            name: name.into(),
            folder: folder.into(),
            tags: vec!["DB".into()],
            description: String::new(),
            body: "psql -h {{host}} -U {{user:postgres}}".into(),
            run_mode: RunMode::PasteEnter,
        }
    }

    #[tokio::test]
    async fn snippets_roundtrip_sorted_by_folder() {
        let (store, _) = store_and_vault().await;
        let a = store
            .save_snippet(None, &input("restart", "Ops").normalized().unwrap())
            .await
            .unwrap();
        store
            .save_snippet(
                None,
                &input("psql", " DB / Postgres ").normalized().unwrap(),
            )
            .await
            .unwrap();
        let list = store.list_snippets().await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].folder, "DB/Postgres");
        assert_eq!(list[0].tags, ["db"]);
        assert_eq!(list[1].run_mode, RunMode::PasteEnter);
        let changed = SnippetInput {
            run_mode: RunMode::Confirm,
            ..input("restart", "Ops")
        };
        let u = store.save_snippet(Some(&a.id), &changed).await.unwrap();
        assert_eq!(u.run_mode, RunMode::Confirm);
        store.delete_snippet(&a.id).await.unwrap();
        assert_eq!(store.list_snippets().await.unwrap().len(), 1);
        assert!(input("", "x").normalized().is_err());
    }

    #[tokio::test]
    async fn secrets_are_encrypted_and_listed_by_name_only() {
        let (store, vault) = store_and_vault().await;
        store
            .set_secret(&vault, "db_pass", "hunter2")
            .await
            .unwrap();
        store.set_secret(&vault, "db_pass", "s3cret").await.unwrap();
        assert_eq!(store.secret_names().await.unwrap(), ["db_pass"]);
        let v = store.get_secret(&vault, "db_pass").await.unwrap().unwrap();
        assert_eq!(v.expose_secret(), "s3cret");
        // Not stored in plaintext.
        let (raw,): (Vec<u8>,) = sqlx::query_as("SELECT value_enc FROM snippet_secrets")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert!(!raw.windows(6).any(|w| w == b"s3cret"));
        store.delete_secret("db_pass").await.unwrap();
        assert!(store.get_secret(&vault, "db_pass").await.unwrap().is_none());
        assert!(valid_secret_name("api.token-2"));
        assert!(!valid_secret_name("bad name"));
    }
}
