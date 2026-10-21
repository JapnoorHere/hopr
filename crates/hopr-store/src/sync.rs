//! Sync v1 (Y1-lite): end-to-end encrypted sync through a folder the user already syncs
//! (OneDrive, Dropbox, a share). Each device writes only its own sealed snapshot
//! (`devices/<id>.hsync`), so file-sync tools never see write conflicts; every device
//! reads the others' and merges record by record, newest change winning, with
//! tombstones for deletions.
//!
//! The snapshot key is a "sync vault" made from the sync passphrase: its header lives in
//! `hopr-sync.json` in the folder, so any device that knows the passphrase opens it.
//! Secrets travel decrypted inside the sealed snapshot and are re-encrypted under each
//! device's own vault when applied.

use std::collections::HashMap;
use std::path::Path;

use hopr_vault::{KdfParams, UnlockedVault, VaultHeader};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::sqlite::SqliteRow;
use sqlx::{Column, Row, TypeInfo, ValueRef};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::{Result, Store, StoreError, now};

const HEADER_FILE: &str = "hopr-sync.json";
const FORMAT: &str = "hopr-sync/1";

/// One row (or a deletion) as it travels between devices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRecord {
    /// "group", "key", "host", "snippet", "secret", "tunnel".
    pub kind: String,
    /// Row id (secret name for secrets).
    pub id: String,
    /// When it last changed (RFC 3339).
    pub updated_at: String,
    /// A deletion.
    pub deleted: bool,
    /// Column values plus `secrets` / `tags` (absent for deletions).
    pub data: Option<Map<String, Value>>,
}

/// One device's sealed contribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// Device id.
    pub device_id: String,
    /// Name shown to the user ("Work laptop").
    pub device_name: String,
    /// When it was written.
    pub written_at: String,
    /// Everything it has.
    pub records: Vec<SyncRecord>,
}

#[derive(Serialize, Deserialize)]
struct Header {
    format: String,
    vault: VaultHeader,
}

/// What a sync did.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncOutcome {
    /// Rows created or updated here.
    pub applied: u32,
    /// Rows deleted here.
    pub deleted: u32,
    /// Records written for the other devices.
    pub pushed: u32,
    /// Other devices seen: (name, written at).
    pub devices: Vec<(String, String)>,
    /// What couldn't be applied and why.
    pub warnings: Vec<String>,
}

/// Table layout of a syncable kind.
struct Kind {
    name: &'static str,
    table: &'static str,
    pk: &'static str,
    /// Encrypted columns, carried decrypted under `secrets` instead.
    secret_cols: &'static [&'static str],
}

/// In apply order (parents first); deletions run in reverse.
const KINDS: [Kind; 6] = [
    Kind {
        name: "group",
        table: "groups",
        pk: "id",
        secret_cols: &[],
    },
    Kind {
        name: "key",
        table: "ssh_keys",
        pk: "id",
        secret_cols: &["private_key_enc"],
    },
    Kind {
        name: "host",
        table: "hosts",
        pk: "id",
        secret_cols: &[],
    },
    Kind {
        name: "snippet",
        table: "snippets",
        pk: "id",
        secret_cols: &[],
    },
    Kind {
        name: "secret",
        table: "snippet_secrets",
        pk: "name",
        secret_cols: &["value_enc"],
    },
    Kind {
        name: "tunnel",
        table: "tunnels",
        pk: "id",
        secret_cols: &[],
    },
];

/// Parse a stored time; unparseable ones count as the oldest.
fn ts(s: &str) -> OffsetDateTime {
    OffsetDateTime::parse(s, &Rfc3339).unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

/// Whether `candidate` is strictly newer than `current` (times compared, not text).
pub fn newer(candidate: &str, current: Option<&str>) -> bool {
    current.is_none_or(|c| ts(candidate) > ts(c))
}

/// Keep the newest record per (kind, id) across devices.
pub fn newest(records: impl IntoIterator<Item = SyncRecord>) -> Vec<SyncRecord> {
    let mut best: HashMap<(String, String), SyncRecord> = HashMap::new();
    for r in records {
        let key = (r.kind.clone(), r.id.clone());
        match best.get(&key) {
            Some(b) if !newer(&r.updated_at, Some(&b.updated_at)) => {}
            _ => {
                best.insert(key, r);
            }
        }
    }
    best.into_values().collect()
}

fn row_json(row: &SqliteRow, skip: &[&str]) -> Map<String, Value> {
    let mut m = Map::new();
    for col in row.columns() {
        let name = col.name();
        if skip.contains(&name) {
            continue;
        }
        let i = col.ordinal();
        let Ok(raw) = row.try_get_raw(i) else {
            continue;
        };
        let v = if raw.is_null() {
            Value::Null
        } else {
            match raw.type_info().name() {
                "INTEGER" | "BOOLEAN" => row
                    .try_get::<i64, _>(i)
                    .map(Value::from)
                    .unwrap_or(Value::Null),
                "REAL" => row
                    .try_get::<f64, _>(i)
                    .map(Value::from)
                    .unwrap_or(Value::Null),
                "BLOB" => continue, // never synced as-is
                _ => row
                    .try_get::<String, _>(i)
                    .map(Value::from)
                    .unwrap_or(Value::Null),
            }
        };
        m.insert(name.to_owned(), v);
    }
    m
}

fn text(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str).map(str::to_owned)
}

// ---------- the folder ----------

/// Open the folder's sync vault with `passphrase`, creating it (and the folder layout) if
/// this is the first device. A wrong passphrase is an error.
pub fn open_folder(
    folder: &Path,
    passphrase: &SecretString,
    kdf: KdfParams,
) -> Result<UnlockedVault> {
    std::fs::create_dir_all(folder.join("devices"))?;
    let path = folder.join(HEADER_FILE);
    if path.exists() {
        let header: Header = serde_json::from_slice(&std::fs::read(&path)?)?;
        if header.format != FORMAT {
            return Err(StoreError::InUse(format!(
                "This sync folder uses an unknown format ({})",
                header.format
            )));
        }
        return Ok(hopr_vault::unlock(&header.vault, passphrase)?);
    }
    let created = hopr_vault::create(passphrase, kdf)?;
    let header = Header {
        format: FORMAT.into(),
        vault: created.header,
    };
    write_atomic(&path, &serde_json::to_vec_pretty(&header)?)?;
    Ok(created.vault)
}

/// Reopen the folder's sync vault from its saved key (no passphrase needed again).
pub fn open_folder_with_key(folder: &Path, dek: &[u8]) -> Result<UnlockedVault> {
    let header: Header = serde_json::from_slice(&std::fs::read(folder.join(HEADER_FILE))?)?;
    Ok(hopr_vault::unlock_with_dek(&header.vault, dek)?)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn snapshot_aad(device_id: &str) -> Vec<u8> {
    format!("hopr.sync.v1:{device_id}").into_bytes()
}

/// Seal and write this device's snapshot.
pub fn write_snapshot(folder: &Path, key: &UnlockedVault, snap: &Snapshot) -> Result<()> {
    let json = zeroize::Zeroizing::new(serde_json::to_vec(snap)?);
    let sealed = key.encrypt(&snapshot_aad(&snap.device_id), &json)?;
    write_atomic(
        &folder
            .join("devices")
            .join(format!("{}.hsync", snap.device_id)),
        &sealed,
    )
}

/// Every other device's snapshot. Unreadable files are reported, not fatal.
pub fn read_snapshots(
    folder: &Path,
    key: &UnlockedVault,
    except_device: &str,
) -> Result<(Vec<Snapshot>, Vec<String>)> {
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder.join("devices")) else {
        return Ok((out, warnings));
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(device) = name.strip_suffix(".hsync") else {
            continue;
        };
        if device == except_device {
            continue;
        }
        let parsed = std::fs::read(e.path())
            .map_err(StoreError::from)
            .and_then(|b| {
                key.decrypt(&snapshot_aad(device), &b)
                    .map_err(StoreError::from)
            })
            .and_then(|plain| serde_json::from_slice::<Snapshot>(&plain).map_err(StoreError::from));
        match parsed {
            Ok(s) if s.device_id == device => out.push(s),
            Ok(_) => warnings.push(format!("{name}: device id doesn't match its file")),
            Err(err) => warnings.push(format!("{name}: couldn't be read ({err})")),
        }
    }
    Ok((out, warnings))
}

// ---------- the database ----------

impl Store {
    async fn columns(&self, table: &str) -> Result<Vec<String>> {
        let rows: Vec<(i64, String, String, i64, Option<String>, i64)> =
            sqlx::query_as(&format!("PRAGMA table_info({table})"))
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|r| r.1).collect())
    }

    /// Everything this device has, plus its deletions, as sync records (secrets decrypted).
    pub async fn export_records(&self, vault: &UnlockedVault) -> Result<Vec<SyncRecord>> {
        let mut out = Vec::new();
        for k in &KINDS {
            let rows = sqlx::query(&format!("SELECT * FROM {}", k.table))
                .fetch_all(&self.pool)
                .await?;
            for row in rows {
                let mut data = row_json(&row, k.secret_cols);
                let id = text(data.get(k.pk)).unwrap_or_default();
                let updated_at = text(data.get("updated_at")).unwrap_or_else(now);
                let mut secrets = Map::new();
                match k.name {
                    "key" => {
                        let enc: Vec<u8> = row.try_get("private_key_enc")?;
                        let pk = vault.decrypt_string(&crate::keys::aad_private(&id), &enc)?;
                        secrets.insert("private".into(), Value::from(pk.expose_secret()));
                    }
                    "secret" => {
                        let enc: Vec<u8> = row.try_get("value_enc")?;
                        let v = vault.decrypt_string(&crate::snippets::aad(&id), &enc)?;
                        secrets.insert("value".into(), Value::from(v.expose_secret()));
                    }
                    "host" => {
                        let tags: Vec<(String,)> = sqlx::query_as(
                            "SELECT tag FROM host_tags WHERE host_id = ? ORDER BY tag",
                        )
                        .bind(&id)
                        .fetch_all(&self.pool)
                        .await?;
                        data.insert(
                            "tags".into(),
                            Value::from(tags.into_iter().map(|(t,)| t).collect::<Vec<_>>()),
                        );
                        let c = self.host_credentials(vault, &id).await?;
                        for (name, v) in [
                            ("password", c.password),
                            ("totp_seed", c.totp_seed),
                            ("proxy_password", c.proxy_password),
                        ] {
                            if let Some(v) = v {
                                secrets.insert(name.into(), Value::from(v.expose_secret()));
                            }
                        }
                    }
                    _ => {}
                }
                if !secrets.is_empty() {
                    data.insert("secrets".into(), Value::Object(secrets));
                }
                out.push(SyncRecord {
                    kind: k.name.into(),
                    id,
                    updated_at,
                    deleted: false,
                    data: Some(data),
                });
            }
        }
        let tombs: Vec<(String, String, String)> =
            sqlx::query_as("SELECT kind, id, deleted_at FROM sync_tombstones")
                .fetch_all(&self.pool)
                .await?;
        for (kind, id, at) in tombs {
            out.push(SyncRecord {
                kind,
                id,
                updated_at: at,
                deleted: true,
                data: None,
            });
        }
        Ok(out)
    }

    /// When this device last changed (or deleted) a record.
    async fn local_time(&self, k: &Kind, id: &str) -> Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(&format!(
            "SELECT updated_at FROM {} WHERE {} = ?",
            k.table, k.pk
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        if let Some((t,)) = row {
            return Ok(Some(t));
        }
        let tomb: Option<(String,)> =
            sqlx::query_as("SELECT deleted_at FROM sync_tombstones WHERE kind = ? AND id = ?")
                .bind(k.name)
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(tomb.map(|(t,)| t))
    }

    /// Upsert one row from its column values. Only real columns of the table are used, so
    /// a record can't name anything else.
    async fn upsert_row(
        &self,
        k: &Kind,
        data: &Map<String, Value>,
        extra: &[(&str, Vec<u8>)],
        overrides: &[(&str, Value)],
    ) -> Result<()> {
        let real = self.columns(k.table).await?;
        let mut cols: Vec<String> = Vec::new();
        let mut vals: Vec<Value> = Vec::new();
        for c in &real {
            if k.secret_cols.contains(&c.as_str()) {
                continue;
            }
            if let Some((_, v)) = overrides.iter().find(|(n, _)| n == c) {
                cols.push(c.clone());
                vals.push(v.clone());
            } else if let Some(v) = data.get(c) {
                cols.push(c.clone());
                vals.push(v.clone());
            }
        }
        let mut all_cols = cols;
        all_cols.extend(extra.iter().map(|(n, _)| (*n).to_owned()));
        let placeholders = vec!["?"; all_cols.len()].join(", ");
        let updates = all_cols
            .iter()
            .filter(|c| c.as_str() != k.pk)
            .map(|c| format!("{c} = excluded.{c}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({placeholders}) ON CONFLICT({}) DO UPDATE SET {updates}",
            k.table,
            all_cols.join(", "),
            k.pk
        );
        let mut q = sqlx::query(&sql);
        for v in vals {
            q = match v {
                Value::Null => q.bind(None::<String>),
                Value::Bool(b) => q.bind(b),
                Value::Number(n) => match n.as_i64() {
                    Some(i) => q.bind(i),
                    None => q.bind(n.as_f64().unwrap_or_default()),
                },
                Value::String(s) => q.bind(s),
                other => q.bind(other.to_string()),
            };
        }
        for (_, bytes) in extra {
            q = q.bind(bytes.clone());
        }
        q.execute(&self.pool).await?;
        Ok(())
    }

    async fn apply_one(&self, vault: &UnlockedVault, k: &Kind, r: &SyncRecord) -> Result<()> {
        let data = r.data.clone().unwrap_or_default();
        let secret = |name: &str| {
            data.get("secrets")
                .and_then(|s| s.get(name))
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        match k.name {
            "key" => {
                let private = secret("private").ok_or(StoreError::NotFound("key material"))?;
                let enc = vault.encrypt(&crate::keys::aad_private(&r.id), private.as_bytes())?;
                self.upsert_row(k, &data, &[("private_key_enc", enc)], &[])
                    .await?;
            }
            "secret" => {
                let value = secret("value").ok_or(StoreError::NotFound("secret value"))?;
                let enc = vault.encrypt(&crate::snippets::aad(&r.id), value.as_bytes())?;
                self.upsert_row(k, &data, &[("value_enc", enc)], &[])
                    .await?;
            }
            "host" => {
                // Jump hosts may arrive later: they are linked in a second pass.
                self.upsert_row(k, &data, &[], &[("jump_host_id", Value::Null)])
                    .await?;
                sqlx::query("DELETE FROM host_tags WHERE host_id = ?")
                    .bind(&r.id)
                    .execute(&self.pool)
                    .await?;
                for t in data
                    .get("tags")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let Some(t) = t.as_str() {
                        sqlx::query("INSERT OR IGNORE INTO host_tags (host_id, tag) VALUES (?, ?)")
                            .bind(&r.id)
                            .bind(t)
                            .execute(&self.pool)
                            .await?;
                    }
                }
                let seal = |aad: Vec<u8>, v: Option<String>| -> Result<Option<Vec<u8>>> {
                    v.map(|v| vault.encrypt(&aad, v.as_bytes()).map_err(Into::into))
                        .transpose()
                };
                sqlx::query(
                    "INSERT INTO host_secrets (host_id, password_enc, totp_seed_enc, proxy_password_enc) VALUES (?, ?, ?, ?)
                     ON CONFLICT(host_id) DO UPDATE SET password_enc = excluded.password_enc,
                       totp_seed_enc = excluded.totp_seed_enc, proxy_password_enc = excluded.proxy_password_enc",
                )
                .bind(&r.id)
                .bind(seal(crate::hosts::aad_password(&r.id), secret("password"))?)
                .bind(seal(crate::hosts::aad_totp(&r.id), secret("totp_seed"))?)
                .bind(seal(crate::hosts::aad_proxy(&r.id), secret("proxy_password"))?)
                .execute(&self.pool)
                .await?;
            }
            _ => self.upsert_row(k, &data, &[], &[]).await?,
        }
        // A newer record supersedes an older local deletion.
        sqlx::query("DELETE FROM sync_tombstones WHERE kind = ? AND id = ?")
            .bind(k.name)
            .bind(&r.id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Apply records that are newer than what this device has. Returns what changed.
    pub async fn apply_records(
        &self,
        vault: &UnlockedVault,
        records: &[SyncRecord],
    ) -> Result<SyncOutcome> {
        let mut outcome = SyncOutcome::default();
        let mut jumps: Vec<(String, String)> = Vec::new();
        // Upserts, parents first.
        for k in &KINDS {
            for r in records.iter().filter(|r| r.kind == k.name && !r.deleted) {
                let local = self.local_time(k, &r.id).await?;
                if !newer(&r.updated_at, local.as_deref()) {
                    continue;
                }
                match self.apply_one(vault, k, r).await {
                    Ok(()) => {
                        outcome.applied += 1;
                        if k.name == "host"
                            && let Some(j) = r
                                .data
                                .as_ref()
                                .and_then(|d| d.get("jump_host_id"))
                                .and_then(Value::as_str)
                        {
                            jumps.push((r.id.clone(), j.to_owned()));
                        }
                    }
                    Err(e) => outcome.warnings.push(format!("{} {}: {e}", k.name, r.id)),
                }
            }
        }
        for (id, jump) in jumps {
            let _ = sqlx::query(
                "UPDATE hosts SET jump_host_id = ? WHERE id = ? AND EXISTS (SELECT 1 FROM hosts WHERE id = ?)",
            )
            .bind(&jump)
            .bind(&id)
            .bind(&jump)
            .execute(&self.pool)
            .await;
        }
        // Deletions, children first.
        for k in KINDS.iter().rev() {
            for r in records.iter().filter(|r| r.kind == k.name && r.deleted) {
                let row: Option<(String,)> = sqlx::query_as(&format!(
                    "SELECT updated_at FROM {} WHERE {} = ?",
                    k.table, k.pk
                ))
                .bind(&r.id)
                .fetch_optional(&self.pool)
                .await?;
                let Some((local,)) = row else { continue };
                if !newer(&r.updated_at, Some(&local)) {
                    continue; // changed here after it was deleted there: keep it
                }
                let res = sqlx::query(&format!("DELETE FROM {} WHERE {} = ?", k.table, k.pk))
                    .bind(&r.id)
                    .execute(&self.pool)
                    .await;
                match res {
                    Ok(_) => outcome.deleted += 1,
                    Err(e) => outcome
                        .warnings
                        .push(format!("{} {} not deleted: {e}", k.name, r.id)),
                }
            }
        }
        Ok(outcome)
    }

    /// One sync round: merge the other devices' snapshots, then publish this device's.
    pub async fn sync_folder(
        &self,
        vault: &UnlockedVault,
        key: &UnlockedVault,
        folder: &Path,
        device_id: &str,
        device_name: &str,
    ) -> Result<SyncOutcome> {
        let (snaps, mut warnings) = read_snapshots(folder, key, device_id)?;
        let devices = snaps
            .iter()
            .map(|s| (s.device_name.clone(), s.written_at.clone()))
            .collect();
        let incoming = newest(snaps.into_iter().flat_map(|s| s.records));
        let mut outcome = self.apply_records(vault, &incoming).await?;
        let mine = self.export_records(vault).await?;
        outcome.pushed = u32::try_from(mine.len()).unwrap_or(u32::MAX);
        write_snapshot(
            folder,
            key,
            &Snapshot {
                device_id: device_id.to_owned(),
                device_name: device_name.to_owned(),
                written_at: now(),
                records: mine,
            },
        )?;
        outcome.devices = devices;
        warnings.append(&mut outcome.warnings);
        outcome.warnings = warnings;
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NewKey;
    use hopr_core::model::{
        AuthKind, Environment, HostInput, RunMode, SecretChange, SnippetInput, TunnelInput,
        TunnelKind,
    };

    async fn device() -> (Store, UnlockedVault) {
        let store = Store::open_memory().await.unwrap();
        let nv =
            hopr_vault::create(&SecretString::from("local".to_owned()), KdfParams::TEST).unwrap();
        store.save_vault_header(&nv.header).await.unwrap();
        (store, nv.vault)
    }

    fn host(label: &str) -> HostInput {
        HostInput {
            label: label.into(),
            protocol: Default::default(),
            hostname: format!("{label}.corp"),
            port: 22,
            serial: None,
            proxy_command: None,
            username: "ops".into(),
            group_id: None,
            tags: vec!["web".into()],
            auth: AuthKind::Password,
            key_id: None,
            jump_host_id: None,
            agent_forwarding: false,
            proxy: None,
            proxy_password: SecretChange::Keep,
            favorite: false,
            environment: Environment::None,
            notes: String::new(),
            password: SecretChange::Set {
                value: format!("pw-{label}"),
            },
            totp_seed: SecretChange::Keep,
        }
    }

    fn key() -> NewKey {
        NewKey {
            label: "laptop".into(),
            algorithm: "ssh-ed25519".into(),
            fingerprint: "SHA256:test".into(),
            public_key: "ssh-ed25519 AAAA test".into(),
            comment: "test".into(),
            private_openssh: SecretString::from("private key material".to_owned()),
        }
    }

    #[tokio::test]
    async fn two_devices_converge_with_secrets_edits_and_deletions() {
        let dir = std::env::temp_dir().join(format!("hopr-sync-test-{}", crate::new_id()));
        let pass = SecretString::from("correct horse".to_owned());
        let (a, va) = device().await;
        let (b, vb) = device().await;
        let ka = open_folder(&dir, &pass, KdfParams::TEST).unwrap();
        let kb = open_folder(&dir, &pass, KdfParams::TEST).unwrap();
        assert!(
            open_folder(
                &dir,
                &SecretString::from("nope".to_owned()),
                KdfParams::TEST
            )
            .is_err()
        );

        // A: group, key, bastion + host behind it, snippet, secret, tunnel.
        let g = a.create_group("Prod").await.unwrap();
        let k = a.add_key(&va, key()).await.unwrap();
        let bastion = a.create_host(&va, host("bastion")).await.unwrap();
        let mut web = host("web");
        web.group_id = Some(g.id.clone());
        web.jump_host_id = Some(bastion.id.clone());
        let web = a.create_host(&va, web).await.unwrap();
        let s = a
            .save_snippet(
                None,
                &SnippetInput {
                    name: "uptime".into(),
                    folder: String::new(),
                    tags: vec![],
                    description: String::new(),
                    body: "uptime".into(),
                    run_mode: RunMode::PasteEnter,
                },
            )
            .await
            .unwrap();
        a.set_secret(&va, "token", "t0k").await.unwrap();
        a.save_tunnel(
            None,
            &TunnelInput {
                host_id: web.id.clone(),
                label: "pg".into(),
                kind: TunnelKind::Local,
                bind_addr: "127.0.0.1".into(),
                bind_port: 15432,
                dest_host: "localhost".into(),
                dest_port: 5432,
                auto_start: false,
            },
        )
        .await
        .unwrap();

        let out = a.sync_folder(&va, &ka, &dir, "dev-a", "A").await.unwrap();
        assert!(out.pushed >= 7, "{out:?}");
        // The file on disk is sealed: no hostnames or passwords in it.
        let raw = std::fs::read(dir.join("devices/dev-a.hsync")).unwrap();
        assert!(!raw.windows(8).any(|w| w == b"web.corp"));
        assert!(!raw.windows(6).any(|w| w == b"pw-web"));

        let got = b.sync_folder(&vb, &kb, &dir, "dev-b", "B").await.unwrap();
        assert!(got.warnings.is_empty(), "{:?}", got.warnings);
        assert_eq!(got.devices.len(), 1);
        assert_eq!(got.devices[0].0, "A");
        let hosts = b.list_hosts().await.unwrap();
        let bweb = hosts.iter().find(|h| h.id == web.id).unwrap();
        assert_eq!(bweb.group_id.as_deref(), Some(g.id.as_str()));
        assert_eq!(bweb.jump_host_id.as_deref(), Some(bastion.id.as_str()));
        assert_eq!(bweb.tags, ["web"]);
        // Secrets re-encrypted under B's own vault.
        let creds = b.host_credentials(&vb, &web.id).await.unwrap();
        assert_eq!(creds.password.unwrap().expose_secret(), "pw-web");
        assert_eq!(
            b.key_private(&vb, &k.id).await.unwrap().expose_secret(),
            "private key material"
        );
        assert_eq!(
            b.get_secret(&vb, "token")
                .await
                .unwrap()
                .unwrap()
                .expose_secret(),
            "t0k"
        );
        assert_eq!(b.list_tunnels().await.unwrap().len(), 1);
        assert_eq!(b.list_snippets().await.unwrap().len(), 1);

        // B renames the host and deletes the snippet; A picks both up.
        let mut renamed = host("web");
        renamed.label = "web-renamed".into();
        renamed.group_id = Some(g.id.clone());
        renamed.jump_host_id = Some(bastion.id.clone());
        renamed.password = SecretChange::Keep;
        b.update_host(&vb, &web.id, renamed).await.unwrap();
        b.delete_snippet(&s.id).await.unwrap();
        b.sync_folder(&vb, &kb, &dir, "dev-b", "B").await.unwrap();
        let back = a.sync_folder(&va, &ka, &dir, "dev-a", "A").await.unwrap();
        assert!(back.applied >= 1 && back.deleted >= 1, "{back:?}");
        let ah = a.list_hosts().await.unwrap();
        assert_eq!(
            ah.iter().find(|h| h.id == web.id).unwrap().label,
            "web-renamed"
        );
        assert!(a.list_snippets().await.unwrap().is_empty());
        // The password survived the rename (kept on B, synced back).
        let ac = a.host_credentials(&va, &web.id).await.unwrap();
        assert_eq!(ac.password.unwrap().expose_secret(), "pw-web");

        // Nothing new: another round changes nothing.
        let quiet = a.sync_folder(&va, &ka, &dir, "dev-a", "A").await.unwrap();
        assert_eq!((quiet.applied, quiet.deleted), (0, 0));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn newest_wins_by_time_not_text() {
        let r = |id: &str, t: &str| SyncRecord {
            kind: "host".into(),
            id: id.into(),
            updated_at: t.into(),
            deleted: false,
            data: None,
        };
        // Different fraction lengths must still compare by time.
        let picked = newest([
            r("h", "2026-10-18T10:00:00.5Z"),
            r("h", "2026-10-18T10:00:00.123456789Z"),
        ]);
        assert_eq!(picked[0].updated_at, "2026-10-18T10:00:00.5Z");
        assert!(newer(
            "2026-10-18T10:00:01Z",
            Some("2026-10-18T10:00:00.999999Z")
        ));
        assert!(!newer("2026-10-18T10:00:00Z", Some("2026-10-18T10:00:00Z")));
        assert!(newer("2026-10-18T10:00:00Z", None));
    }
}
