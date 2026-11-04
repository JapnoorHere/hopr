//! The sync folder: its header, opening it with the passphrase, and reading and writing encrypted snapshots.

use std::path::Path;

use hopr_vault::{KdfParams, UnlockedVault};
use secrecy::SecretString;

use crate::{Result, StoreError};

use super::*;

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

pub(super) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub(super) fn snapshot_aad(device_id: &str) -> Vec<u8> {
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
