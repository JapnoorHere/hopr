//! Hopr vault (ADR 0005).
//!
//! * master password → Argon2id → KEK
//! * random 256-bit DEK, stored wrapped by the KEK, and separately by a recovery key
//! * every secret field: XChaCha20-Poly1305 under the DEK, random 24-byte nonce, with
//!   associated data naming the field and row so ciphertexts can't be swapped around
//! * the unlocked DEK lives in a `SecretBox` and is zeroized when dropped (lock)

#![warn(missing_docs)]

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use secrecy::{ExposeSecret, SecretBox, SecretString};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::Zeroizing;

const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const SALT_LEN: usize = 16;
const RECOVERY_BYTES: usize = 20; // 160 bits → 32 base32 chars
const AAD_DEK: &[u8] = b"hopr:v1:dek";
const AAD_DEK_RECOVERY: &[u8] = b"hopr:v1:dek-recovery";
const AAD_CHECK: &[u8] = b"hopr:v1:check";
const CHECK_PLAINTEXT: &[u8] = b"hopr-vault-ok";

/// Vault failures. Messages are safe to show; none include secret material.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VaultError {
    /// Password or recovery key did not unwrap the DEK.
    #[error("incorrect password")]
    WrongPassword,
    /// Recovery key is not in the expected format.
    #[error("that doesn't look like a recovery key")]
    MalformedRecoveryKey,
    /// Ciphertext failed authentication (tampered, truncated, or wrong field).
    #[error("stored secret could not be decrypted")]
    Corrupt,
    /// The OS random number generator failed.
    #[error("random number generator unavailable")]
    Rng,
    /// Argon2 rejected the parameters.
    #[error("invalid key-derivation parameters")]
    Kdf,
}

/// Argon2id cost parameters (persisted in the vault header).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    /// Memory in KiB.
    pub m_cost_kib: u32,
    /// Iterations.
    pub t_cost: u32,
    /// Parallelism.
    pub p_cost: u32,
}

impl KdfParams {
    /// Production default (HOPR_PLAN §6): 64 MiB, 3 passes, 1 lane.
    pub const DEFAULT: Self = Self {
        m_cost_kib: 64 * 1024,
        t_cost: 3,
        p_cost: 1,
    };
    /// Cheap parameters for tests only.
    pub const TEST: Self = Self {
        m_cost_kib: 256,
        t_cost: 1,
        p_cost: 1,
    };
}

/// Everything persisted about a vault. Contains no plaintext secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultHeader {
    /// Format version.
    pub version: u32,
    /// KDF parameters for the master password.
    pub kdf: KdfParams,
    /// Argon2 salt.
    pub salt: Vec<u8>,
    /// DEK encrypted under the password KEK.
    pub dek_wrapped: Vec<u8>,
    /// HKDF salt for the recovery key.
    pub recovery_salt: Vec<u8>,
    /// DEK encrypted under the recovery KEK.
    pub recovery_wrapped: Vec<u8>,
    /// Known plaintext encrypted under the DEK, to verify a DEK from the OS keychain.
    pub check: Vec<u8>,
}

/// An unlocked vault. Dropping it (lock) zeroizes the DEK.
pub struct UnlockedVault {
    dek: SecretBox<[u8; KEY_LEN]>,
}

/// Result of creating a vault: header to persist, the open vault, and the recovery key
/// to show the user exactly once.
pub struct NewVault {
    /// Persist this.
    pub header: VaultHeader,
    /// Ready to use.
    pub vault: UnlockedVault,
    /// Show once, never store.
    pub recovery_key: SecretString,
}

fn random<const N: usize>() -> Result<[u8; N], VaultError> {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).map_err(|_| VaultError::Rng)?;
    Ok(buf)
}

fn derive_kek(
    password: &SecretString,
    salt: &[u8],
    kdf: KdfParams,
) -> Result<Zeroizing<[u8; KEY_LEN]>, VaultError> {
    let params = Params::new(kdf.m_cost_kib, kdf.t_cost, kdf.p_cost, Some(KEY_LEN))
        .map_err(|_| VaultError::Kdf)?;
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.expose_secret().as_bytes(), salt, out.as_mut())
        .map_err(|_| VaultError::Kdf)?;
    Ok(out)
}

fn recovery_kek(recovery: &[u8], salt: &[u8]) -> Zeroizing<[u8; KEY_LEN]> {
    // The recovery key is 160 bits of entropy, so HKDF (not a slow KDF) is sufficient.
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    let hk = Hkdf::<Sha256>::new(Some(salt), recovery);
    // Expanding 32 bytes from SHA-256 HKDF cannot fail.
    let _ = hk.expand(b"hopr:v1:recovery-kek", out.as_mut());
    out
}

fn seal(key: &[u8; KEY_LEN], aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, VaultError> {
    let nonce: [u8; NONCE_LEN] = random()?;
    let cipher = XChaCha20Poly1305::new(key.into());
    let ct = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| VaultError::Corrupt)?;
    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

fn open(key: &[u8; KEY_LEN], aad: &[u8], sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    if sealed.len() < NONCE_LEN + 16 {
        return Err(VaultError::Corrupt);
    }
    let (nonce, ct) = sealed.split_at(NONCE_LEN);
    XChaCha20Poly1305::new(key.into())
        .decrypt(XNonce::from_slice(nonce), Payload { msg: ct, aad })
        .map(Zeroizing::new)
        .map_err(|_| VaultError::Corrupt)
}

fn dek_from(bytes: &[u8]) -> Result<UnlockedVault, VaultError> {
    let arr: [u8; KEY_LEN] = bytes.try_into().map_err(|_| VaultError::Corrupt)?;
    Ok(UnlockedVault {
        dek: SecretBox::new(Box::new(arr)),
    })
}

/// Format a recovery key as `XXXX-XXXX-…` (base32, 8 groups).
fn format_recovery(bytes: &[u8]) -> String {
    let b32 = data_encoding::BASE32_NOPAD.encode(bytes);
    b32.as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("-")
}

fn parse_recovery(text: &str) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    let cleaned: String = text
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase();
    let bytes = data_encoding::BASE32_NOPAD
        .decode(cleaned.as_bytes())
        .map_err(|_| VaultError::MalformedRecoveryKey)?;
    if bytes.len() != RECOVERY_BYTES {
        return Err(VaultError::MalformedRecoveryKey);
    }
    Ok(Zeroizing::new(bytes))
}

/// Create a new vault protected by `password`.
pub fn create(password: &SecretString, kdf: KdfParams) -> Result<NewVault, VaultError> {
    let dek = Zeroizing::new(random::<KEY_LEN>()?);
    let salt: [u8; SALT_LEN] = random()?;
    let kek = derive_kek(password, &salt, kdf)?;

    let recovery = Zeroizing::new(random::<RECOVERY_BYTES>()?);
    let recovery_salt: [u8; SALT_LEN] = random()?;
    let rkek = recovery_kek(recovery.as_ref(), &recovery_salt);

    let header = VaultHeader {
        version: 1,
        kdf,
        salt: salt.to_vec(),
        dek_wrapped: seal(&kek, AAD_DEK, dek.as_ref())?,
        recovery_salt: recovery_salt.to_vec(),
        recovery_wrapped: seal(&rkek, AAD_DEK_RECOVERY, dek.as_ref())?,
        check: seal(&dek, AAD_CHECK, CHECK_PLAINTEXT)?,
    };
    Ok(NewVault {
        header,
        vault: dek_from(dek.as_ref())?,
        recovery_key: SecretString::from(format_recovery(recovery.as_ref())),
    })
}

/// Unlock with the master password.
pub fn unlock(header: &VaultHeader, password: &SecretString) -> Result<UnlockedVault, VaultError> {
    let kek = derive_kek(password, &header.salt, header.kdf)?;
    let dek = open(&kek, AAD_DEK, &header.dek_wrapped).map_err(|_| VaultError::WrongPassword)?;
    dek_from(&dek)
}

/// Unlock with the recovery key (used to set a new password).
pub fn unlock_with_recovery(
    header: &VaultHeader,
    recovery_key: &SecretString,
) -> Result<UnlockedVault, VaultError> {
    let recovery = parse_recovery(recovery_key.expose_secret())?;
    let rkek = recovery_kek(&recovery, &header.recovery_salt);
    let dek = open(&rkek, AAD_DEK_RECOVERY, &header.recovery_wrapped)
        .map_err(|_| VaultError::WrongPassword)?;
    dek_from(&dek)
}

/// Rebuild an unlocked vault from DEK bytes held in the OS keychain, verifying them
/// against the header's check value.
pub fn unlock_with_dek(header: &VaultHeader, dek: &[u8]) -> Result<UnlockedVault, VaultError> {
    let vault = dek_from(dek)?;
    open(vault.dek.expose_secret(), AAD_CHECK, &header.check)
        .map_err(|_| VaultError::WrongPassword)?;
    Ok(vault)
}

impl UnlockedVault {
    /// Encrypt one secret field. `aad` must name the field and row, e.g. `b"host.password:<id>"`.
    pub fn encrypt(&self, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, VaultError> {
        seal(self.dek.expose_secret(), aad, plaintext)
    }

    /// Decrypt one secret field encrypted with the same `aad`.
    pub fn decrypt(&self, aad: &[u8], sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, VaultError> {
        open(self.dek.expose_secret(), aad, sealed)
    }

    /// Decrypt a UTF-8 secret into a `SecretString`.
    pub fn decrypt_string(&self, aad: &[u8], sealed: &[u8]) -> Result<SecretString, VaultError> {
        let bytes = self.decrypt(aad, sealed)?;
        let s = std::str::from_utf8(&bytes).map_err(|_| VaultError::Corrupt)?;
        Ok(SecretString::from(s.to_owned()))
    }

    /// Re-wrap the DEK under a new master password. Data is not re-encrypted.
    pub fn rewrap(
        &self,
        header: &VaultHeader,
        new_password: &SecretString,
    ) -> Result<VaultHeader, VaultError> {
        let salt: [u8; SALT_LEN] = random()?;
        let kek = derive_kek(new_password, &salt, header.kdf)?;
        Ok(VaultHeader {
            salt: salt.to_vec(),
            dek_wrapped: seal(&kek, AAD_DEK, self.dek.expose_secret())?,
            ..header.clone()
        })
    }

    /// DEK bytes for the OS keychain ("remember on this device"). Handle with care.
    pub fn export_dek(&self) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(self.dek.expose_secret().to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pw(s: &str) -> SecretString {
        SecretString::from(s.to_owned())
    }

    #[test]
    fn create_unlock_roundtrip() {
        let nv = create(&pw("correct horse"), KdfParams::TEST).unwrap();
        let sealed = nv.vault.encrypt(b"host.password:1", b"s3cret").unwrap();
        let v = unlock(&nv.header, &pw("correct horse")).unwrap();
        assert_eq!(&*v.decrypt(b"host.password:1", &sealed).unwrap(), b"s3cret");
    }

    #[test]
    fn wrong_password_is_rejected() {
        let nv = create(&pw("a"), KdfParams::TEST).unwrap();
        assert_eq!(
            unlock(&nv.header, &pw("b")).err(),
            Some(VaultError::WrongPassword)
        );
    }

    #[test]
    fn aad_binds_ciphertext_to_its_field() {
        let nv = create(&pw("a"), KdfParams::TEST).unwrap();
        let sealed = nv.vault.encrypt(b"host.password:1", b"x").unwrap();
        assert_eq!(
            nv.vault.decrypt(b"host.password:2", &sealed).err(),
            Some(VaultError::Corrupt)
        );
    }

    #[test]
    fn tampering_is_detected() {
        let nv = create(&pw("a"), KdfParams::TEST).unwrap();
        let mut sealed = nv.vault.encrypt(b"f", b"payload").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 1;
        assert_eq!(
            nv.vault.decrypt(b"f", &sealed).err(),
            Some(VaultError::Corrupt)
        );
        assert_eq!(
            nv.vault.decrypt(b"f", &sealed[..10]).err(),
            Some(VaultError::Corrupt)
        );
    }

    #[test]
    fn recovery_key_unlocks_and_tolerates_formatting() {
        let nv = create(&pw("a"), KdfParams::TEST).unwrap();
        let rk = nv.recovery_key.expose_secret().to_owned();
        assert_eq!(rk.split('-').count(), 8);
        let sealed = nv.vault.encrypt(b"f", b"data").unwrap();
        let messy = format!("  {} ", rk.to_lowercase().replace('-', " "));
        let v = unlock_with_recovery(&nv.header, &pw(&messy)).unwrap();
        assert_eq!(&*v.decrypt(b"f", &sealed).unwrap(), b"data");
        assert_eq!(
            unlock_with_recovery(&nv.header, &pw("nope")).err(),
            Some(VaultError::MalformedRecoveryKey)
        );
    }

    #[test]
    fn rewrap_changes_password_without_touching_data() {
        let nv = create(&pw("old"), KdfParams::TEST).unwrap();
        let sealed = nv.vault.encrypt(b"f", b"data").unwrap();
        let header = nv.vault.rewrap(&nv.header, &pw("new")).unwrap();
        assert!(unlock(&header, &pw("old")).is_err());
        let v = unlock(&header, &pw("new")).unwrap();
        assert_eq!(&*v.decrypt(b"f", &sealed).unwrap(), b"data");
        // The recovery key still works after a password change.
        assert_eq!(header.recovery_wrapped, nv.header.recovery_wrapped);
    }

    #[test]
    fn keychain_dek_is_verified() {
        let nv = create(&pw("a"), KdfParams::TEST).unwrap();
        let dek = nv.vault.export_dek();
        assert!(unlock_with_dek(&nv.header, &dek).is_ok());
        assert!(unlock_with_dek(&nv.header, &[0u8; 32]).is_err());
    }

    #[test]
    fn header_contains_no_plaintext() {
        let nv = create(&pw("hunter2-password"), KdfParams::TEST).unwrap();
        let json = format!("{:?}", nv.header);
        assert!(!json.contains("hunter2"));
    }
}
