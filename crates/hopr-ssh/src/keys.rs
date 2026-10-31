//! Key generation and import (HOPR_PLAN §9.14). Imported keys are decrypted once and
//! re-encoded as unencrypted OpenSSH, because the vault (not a passphrase) protects them.

use std::sync::Arc;

use hopr_core::model::{CertificateInfo, KeyAlgorithm};
use hopr_core::{AppError, ErrorCode};
use russh::keys::ssh_key::LineEnding;
use russh::keys::ssh_key::certificate::CertType;
use russh::keys::ssh_key::private::{KeypairData, RsaKeypair};
use russh::keys::{Algorithm, Certificate, EcdsaCurve, HashAlg, PrivateKey, decode_secret_key};
use secrecy::{ExposeSecret, SecretString};

/// A key ready to store.
pub struct ParsedKey {
    /// e.g. "ssh-ed25519".
    pub algorithm: String,
    /// "SHA256:…".
    pub fingerprint: String,
    /// OpenSSH public key line (with comment).
    pub public_key: String,
    /// Comment.
    pub comment: String,
    /// Unencrypted OpenSSH private key.
    pub private_openssh: SecretString,
}

fn internal(e: impl std::fmt::Display) -> AppError {
    AppError::internal("Key operation failed", e)
}

fn describe(key: &PrivateKey) -> Result<ParsedKey, AppError> {
    let private = key.to_openssh(LineEnding::LF).map_err(internal)?;
    Ok(ParsedKey {
        algorithm: key.algorithm().as_str().to_owned(),
        fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
        public_key: key.public_key().to_openssh().map_err(internal)?,
        comment: key.comment().to_string(),
        private_openssh: SecretString::from(private.to_string()),
    })
}

/// Generate a new key pair.
pub fn generate(algorithm: KeyAlgorithm, comment: &str) -> Result<ParsedKey, AppError> {
    let mut rng = rand::rng();
    let mut key = match algorithm {
        KeyAlgorithm::Ed25519 => PrivateKey::random(&mut rng, Algorithm::Ed25519),
        KeyAlgorithm::EcdsaP256 => PrivateKey::random(
            &mut rng,
            Algorithm::Ecdsa {
                curve: EcdsaCurve::NistP256,
            },
        ),
        KeyAlgorithm::Rsa3072 | KeyAlgorithm::Rsa4096 => {
            let bits = if algorithm == KeyAlgorithm::Rsa3072 {
                3072
            } else {
                4096
            };
            RsaKeypair::random(&mut rng, bits)
                .and_then(|kp| PrivateKey::new(KeypairData::from(kp), ""))
        }
    }
    .map_err(internal)?;
    key.set_comment(comment.trim());
    describe(&key)
}

/// Unencrypted PuTTY Ed25519 keys whose secret starts with a byte ≥ 0x80. ssh-key 0.7.0-rc
/// keeps the mpint's sign byte, sees 33 bytes and refuses them (about half of all keys).
/// Verifies the MAC (v2: HMAC-SHA1, v3: HMAC-SHA256) and that the secret gives the stated
/// public key.
fn ppk_ed25519_unencrypted(text: &str) -> Option<PrivateKey> {
    use hmac::{Hmac, Mac};
    use russh::keys::ssh_key::private::{Ed25519Keypair, Ed25519PrivateKey};
    use sha1::Digest;

    fn take_string(b: &[u8]) -> Option<(&[u8], &[u8])> {
        let len = usize::try_from(u32::from_be_bytes(b.get(..4)?.try_into().ok()?)).ok()?;
        let end = 4usize.checked_add(len)?;
        Some((b.get(4..end)?, b.get(end..)?))
    }
    fn put_string(buf: &mut Vec<u8>, b: &[u8]) {
        buf.extend_from_slice(&u32::try_from(b.len()).unwrap_or(u32::MAX).to_be_bytes());
        buf.extend_from_slice(b);
    }

    let mut lines = text.lines().map(str::trim_end);
    let header = lines.next()?;
    let (version, alg) = header
        .strip_prefix("PuTTY-User-Key-File-")?
        .split_once(": ")?;
    if alg != "ssh-ed25519" {
        return None;
    }
    let mut encryption = "";
    let mut comment = "";
    let mut public = Vec::new();
    let mut private = Vec::new();
    let mut mac_hex = "";
    while let Some(line) = lines.next() {
        let (k, v) = line.split_once(": ")?;
        match k {
            "Encryption" => encryption = v,
            "Comment" => comment = v,
            "Public-Lines" | "Private-Lines" => {
                let n: usize = v.parse().ok()?;
                let b64: String = lines.by_ref().take(n).collect();
                let bytes = data_encoding::BASE64.decode(b64.as_bytes()).ok()?;
                if k == "Public-Lines" {
                    public = bytes;
                } else {
                    private = bytes;
                }
            }
            "Private-MAC" => mac_hex = v,
            _ => {}
        }
    }
    if encryption != "none" {
        return None;
    }
    let mut mac_input = Vec::new();
    put_string(&mut mac_input, alg.as_bytes());
    put_string(&mut mac_input, encryption.as_bytes());
    put_string(&mut mac_input, comment.as_bytes());
    put_string(&mut mac_input, &public);
    put_string(&mut mac_input, &private);
    let mac = data_encoding::HEXLOWER_PERMISSIVE
        .decode(mac_hex.as_bytes())
        .ok()?;
    let ok = match version {
        "2" => {
            let key = sha1::Sha1::digest(b"putty-private-key-file-mac-key");
            let mut h = Hmac::<sha1::Sha1>::new_from_slice(&key).ok()?;
            h.update(&mac_input);
            h.verify_slice(&mac).is_ok()
        }
        "3" => {
            let mut h = Hmac::<sha2::Sha256>::new_from_slice(&[]).ok()?;
            h.update(&mac_input);
            h.verify_slice(&mac).is_ok()
        }
        _ => false,
    };
    if !ok {
        return None;
    }
    let (_, rest) = take_string(&public)?; // "ssh-ed25519"
    let (pub_bytes, _) = take_string(rest)?;
    let (mpint, _) = take_string(&private)?;
    let secret = match mpint {
        [0, rest @ ..] => rest,
        other => other,
    };
    if secret.len() > 32 {
        return None;
    }
    let mut seed = [0u8; 32];
    seed[32 - secret.len()..].copy_from_slice(secret);
    let pair = Ed25519Keypair::from(Ed25519PrivateKey::from_bytes(&seed));
    if pair.public.as_ref() != pub_bytes {
        return None;
    }
    let mut key = PrivateKey::from(pair);
    key.set_comment(comment);
    Some(key)
}

/// Import a private key (OpenSSH, PEM/PKCS#8 or PuTTY .ppk), decrypting it if needed.
pub fn import(text: &str, passphrase: Option<&str>) -> Result<ParsedKey, AppError> {
    let passphrase = passphrase.filter(|p| !p.is_empty());
    let decoded = match decode_secret_key(text, passphrase) {
        Err(e) if text.trim_start().starts_with("PuTTY-User-Key-File-") => {
            ppk_ed25519_unencrypted(text.trim_start()).ok_or(e)
        }
        other => other,
    };
    match decoded {
        Ok(key) => describe(&key),
        Err(russh::keys::Error::KeyIsEncrypted) => Err(AppError::new(
            ErrorCode::PassphraseRequired,
            "This key is protected by a passphrase",
        )),
        Err(e) if passphrase.is_some() => {
            Err(AppError::invalid("Wrong passphrase, or the key is damaged")
                .with_detail(e.to_string()))
        }
        Err(e) => {
            Err(AppError::invalid("That doesn't look like a private key")
                .with_detail(e.to_string()))
        }
    }
}

/// Load a stored (unencrypted OpenSSH) key for authentication.
pub fn load(private_openssh: &SecretString) -> Result<Arc<PrivateKey>, AppError> {
    decode_secret_key(private_openssh.expose_secret(), None)
        .map(Arc::new)
        .map_err(|e| AppError::internal("A stored key could not be read", e))
}

/// What deploying a public key changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeployResult {
    /// The key was appended to `authorized_keys`.
    Added,
    /// The key was already authorized; nothing changed except permissions.
    AlreadyPresent,
}

/// Remote command for a deploy. The script arrives on stdin, so it runs under `sh` whatever
/// the account's login shell is (fish, csh, …).
pub const DEPLOY_COMMAND: &str = "sh -s";

const ADDED: &str = "HOPR_DEPLOY_ADDED";
const PRESENT: &str = "HOPR_DEPLOY_PRESENT";

/// POSIX script that authorizes `public_key` on the server (C17): creates `~/.ssh` (700)
/// and `authorized_keys` (600), and appends the key only if it isn't there yet.
pub fn deploy_script(public_key: &str) -> Result<String, AppError> {
    let bad = || AppError::invalid("That isn't a valid public key");
    russh::keys::PublicKey::from_openssh(public_key.trim()).map_err(|_| bad())?;
    let mut parts = public_key.split_whitespace();
    let (Some(kind), Some(blob)) = (parts.next(), parts.next()) else {
        return Err(bad());
    };
    let kind_ok = kind
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "@.-".contains(c));
    let blob_ok = blob
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "+/=".contains(c));
    if !kind_ok || !blob_ok {
        return Err(bad());
    }
    // The comment is cosmetic; keep it shell-inert.
    let comment: String = parts
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "@._-+: ".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let line = if comment.is_empty() {
        format!("{kind} {blob}")
    } else {
        format!("{kind} {blob} {comment}")
    };
    Ok(format!(
        r#"umask 077
mkdir -p "$HOME/.ssh" && chmod 700 "$HOME/.ssh" || exit 11
f="$HOME/.ssh/authorized_keys"
touch "$f" && chmod 600 "$f" || exit 12
if grep -qF '{kind} {blob}' "$f"; then
  echo {PRESENT}
else
  if [ -s "$f" ] && [ -n "$(tail -c 1 "$f")" ]; then echo >> "$f"; fi
  printf '%s\n' '{line}' >> "$f" && echo {ADDED}
fi
"#
    ))
}

/// Interpret the output of [`deploy_script`].
pub fn deploy_outcome(
    exit_status: Option<u32>,
    stdout: &str,
    stderr: &str,
) -> Result<DeployResult, AppError> {
    if stdout.contains(PRESENT) {
        return Ok(DeployResult::AlreadyPresent);
    }
    if stdout.contains(ADDED) {
        return Ok(DeployResult::Added);
    }
    let why = match exit_status {
        Some(11) => "Couldn't create ~/.ssh on the server".to_owned(),
        Some(12) => "Couldn't write ~/.ssh/authorized_keys on the server".to_owned(),
        Some(127) => "The server has no POSIX shell (sh); add the key by hand".to_owned(),
        _ => "The server didn't confirm the key was added".to_owned(),
    };
    Err(AppError::new(ErrorCode::Internal, why).with_detail(stderr.trim().to_owned()))
}

/// Parse an OpenSSH certificate and check it belongs to `public_key` (an OpenSSH public
/// key line). Returns the certificate and what it says.
pub fn parse_certificate(
    text: &str,
    public_key: &str,
) -> Result<(Certificate, CertificateInfo), AppError> {
    let bad = |detail: String| {
        AppError::invalid("That isn't an OpenSSH certificate (…-cert-v01@openssh.com)")
            .with_detail(detail)
    };
    let cert = Certificate::from_openssh(text.trim()).map_err(|e| bad(e.to_string()))?;
    let key = russh::keys::PublicKey::from_openssh(public_key.trim()).map_err(internal)?;
    if cert.public_key() != key.key_data() {
        return Err(AppError::invalid(
            "This certificate was issued for a different key",
        ));
    }
    if cert.cert_type() != CertType::User {
        return Err(AppError::invalid(
            "This is a host certificate; log-ins need a user certificate",
        ));
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    #[allow(clippy::cast_precision_loss)] // Unix seconds fit in f64 exactly
    let info = CertificateInfo {
        key_id: cert.key_id().to_owned(),
        principals: cert.valid_principals().to_vec(),
        valid_after: cert.valid_after() as f64,
        valid_before: (cert.valid_before() != u64::MAX).then(|| cert.valid_before() as f64),
        ca_fingerprint: cert
            .signature_key()
            .fingerprint(HashAlg::Sha256)
            .to_string(),
        expired: cert.valid_before() < now,
    };
    Ok((cert, info))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PuTTY key file (PPK v2, unencrypted) built by hand for an Ed25519 key.
    fn ppk_v2(key: &PrivateKey, comment: &str) -> String {
        use hmac::{Hmac, Mac};
        use sha1::{Digest, Sha1};
        fn string(buf: &mut Vec<u8>, b: &[u8]) {
            buf.extend_from_slice(&u32::try_from(b.len()).unwrap().to_be_bytes());
            buf.extend_from_slice(b);
        }
        let KeypairData::Ed25519(kp) = key.key_data() else {
            panic!("ed25519 expected");
        };
        let mut public = Vec::new();
        string(&mut public, b"ssh-ed25519");
        string(&mut public, kp.public.as_ref());
        // The private scalar as an mpint: no leading zeros, 0x00 first if the top bit is set.
        let secret = kp.private.to_bytes();
        let trimmed: Vec<u8> = secret.iter().copied().skip_while(|b| *b == 0).collect();
        let mut mpint = Vec::new();
        if trimmed.first().is_some_and(|b| b & 0x80 != 0) {
            mpint.push(0);
        }
        mpint.extend_from_slice(&trimmed);
        let mut private = Vec::new();
        string(&mut private, &mpint);
        let mut mac_input = Vec::new();
        string(&mut mac_input, b"ssh-ed25519");
        string(&mut mac_input, b"none");
        string(&mut mac_input, comment.as_bytes());
        string(&mut mac_input, &public);
        string(&mut mac_input, &private);
        let mac_key = Sha1::digest(b"putty-private-key-file-mac-key");
        let mut mac = Hmac::<Sha1>::new_from_slice(&mac_key).unwrap();
        mac.update(&mac_input);
        let mac_hex: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let lines = |b: &[u8]| {
            let enc = data_encoding::BASE64.encode(b);
            let chunks: Vec<String> = enc
                .as_bytes()
                .chunks(64)
                .map(|c| String::from_utf8_lossy(c).into_owned())
                .collect();
            (chunks.len(), chunks.join("\n"))
        };
        let (pn, pl) = lines(&public);
        let (sn, sl) = lines(&private);
        format!(
            "PuTTY-User-Key-File-2: ssh-ed25519\nEncryption: none\nComment: {comment}\nPublic-Lines: {pn}\n{pl}\nPrivate-Lines: {sn}\n{sl}\nPrivate-MAC: {mac_hex}\n"
        )
    }

    #[test]
    fn imports_putty_ppk_keys_as_the_same_key() {
        // Cover secrets starting below and at/above 0x80 (the second needs the fallback).
        let mut seen = [false, false];
        for _ in 0..64 {
            let original = generate(KeyAlgorithm::Ed25519, "from-putty").unwrap();
            let key = decode_secret_key(original.private_openssh.expose_secret(), None).unwrap();
            let KeypairData::Ed25519(kp) = key.key_data() else {
                unreachable!()
            };
            let high = kp.private.to_bytes()[0] >= 0x80;
            if seen[usize::from(high)] {
                continue;
            }
            seen[usize::from(high)] = true;
            let imported = import(&ppk_v2(&key, "from-putty"), None).unwrap();
            assert_eq!(
                imported.fingerprint, original.fingerprint,
                "high bit {high}"
            );
            if seen == [true, true] {
                break;
            }
        }
        assert_eq!(seen, [true, true]);
        let original = generate(KeyAlgorithm::Ed25519, "from-putty").unwrap();
        let key = decode_secret_key(original.private_openssh.expose_secret(), None).unwrap();
        let ppk = ppk_v2(&key, "from-putty");
        let imported = import(&ppk, None).unwrap();
        assert_eq!(imported.fingerprint, original.fingerprint);
        assert_eq!(imported.comment, "from-putty");
        // Re-encoded as OpenSSH, so it loads like any stored key.
        assert!(
            imported
                .private_openssh
                .expose_secret()
                .starts_with("-----BEGIN OPENSSH PRIVATE KEY-----")
        );
        // A damaged MAC is refused.
        let bad = ppk.replace("Private-MAC: ", "Private-MAC: 00");
        assert!(import(&bad, None).is_err());
    }

    fn cert_for(key: &ParsedKey, kind: CertType) -> String {
        use russh::keys::ssh_key::certificate::Builder;
        let ca = generate(KeyAlgorithm::Ed25519, "ca").unwrap();
        let ca = decode_secret_key(ca.private_openssh.expose_secret(), None).unwrap();
        let public = russh::keys::PublicKey::from_openssh(&key.public_key).unwrap();
        let mut b =
            Builder::new(*b"0123456789abcdef0123456789abcdef", public, 0, u64::MAX).unwrap();
        b.key_id("ops-ann").unwrap();
        b.cert_type(kind).unwrap();
        b.valid_principal("ann").unwrap();
        b.valid_principal("deploy").unwrap();
        b.sign(&ca).unwrap().to_openssh().unwrap()
    }

    #[test]
    fn certificate_is_summarized_and_must_match_its_key() {
        let key = generate(KeyAlgorithm::Ed25519, "me").unwrap();
        let text = cert_for(&key, CertType::User);
        let (_, info) = parse_certificate(&text, &key.public_key).unwrap();
        assert_eq!(info.key_id, "ops-ann");
        assert_eq!(info.principals, ["ann", "deploy"]);
        assert_eq!(info.valid_before, None, "forever");
        assert!(!info.expired);
        assert!(info.ca_fingerprint.starts_with("SHA256:"));

        let other = generate(KeyAlgorithm::Ed25519, "other").unwrap();
        let e = parse_certificate(&text, &other.public_key).unwrap_err();
        assert!(e.message.contains("different key"), "{}", e.message);
        let host = cert_for(&key, CertType::Host);
        assert!(parse_certificate(&host, &key.public_key).is_err());
        assert!(parse_certificate("ssh-ed25519 AAAA nope", &key.public_key).is_err());
    }

    #[test]
    fn deploy_script_quotes_nothing_dangerous() {
        let k = generate(KeyAlgorithm::Ed25519, "it's me; rm -rf /").unwrap();
        let script = deploy_script(&k.public_key).unwrap();
        assert!(script.contains("it_s me_ rm -rf _"), "{script}");
        assert!(!script.contains("it's"));
        assert!(deploy_script("ssh-ed25519 not-a-key").is_err());
        assert!(deploy_script("").is_err());
    }

    #[test]
    fn deploy_outcome_reads_markers_and_exit_codes() {
        assert_eq!(
            deploy_outcome(Some(0), "HOPR_DEPLOY_ADDED\n", "").unwrap(),
            DeployResult::Added
        );
        assert_eq!(
            deploy_outcome(Some(0), "HOPR_DEPLOY_PRESENT\n", "").unwrap(),
            DeployResult::AlreadyPresent
        );
        let e = deploy_outcome(Some(127), "", "sh: not found").unwrap_err();
        assert!(e.message.contains("POSIX shell"));
    }

    /// Runs the real script under `sh` (Git's on Windows) in a throwaway HOME.
    #[test]
    fn deploy_script_is_idempotent_under_sh() {
        let Ok(sh) = which_sh() else {
            eprintln!("no sh on PATH; skipping");
            return;
        };
        let home = std::env::temp_dir().join(format!("hopr-deploy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".ssh")).unwrap();
        // An existing file without a trailing newline must not be merged into.
        std::fs::write(
            home.join(".ssh/authorized_keys"),
            "ssh-ed25519 AAAAexisting old",
        )
        .unwrap();
        let k = generate(KeyAlgorithm::Ed25519, "deploy test").unwrap();
        let script = deploy_script(&k.public_key).unwrap();
        let run = || {
            use std::io::Write;
            let mut child = std::process::Command::new(&sh)
                .arg("-s")
                .env("HOME", &home)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(script.as_bytes())
                .unwrap();
            let out = child.wait_with_output().unwrap();
            deploy_outcome(
                out.status.code().and_then(|c| u32::try_from(c).ok()),
                &String::from_utf8_lossy(&out.stdout),
                &String::from_utf8_lossy(&out.stderr),
            )
        };
        assert_eq!(run().unwrap(), DeployResult::Added);
        assert_eq!(run().unwrap(), DeployResult::AlreadyPresent);
        let text = std::fs::read_to_string(home.join(".ssh/authorized_keys")).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text:?}");
        assert_eq!(lines[0], "ssh-ed25519 AAAAexisting old");
        assert!(lines[1].starts_with(&k.public_key[..k.public_key.rfind(' ').unwrap()]));
        let _ = std::fs::remove_dir_all(&home);
    }

    fn which_sh() -> Result<std::path::PathBuf, ()> {
        let path = std::env::var_os("PATH").ok_or(())?;
        let names: &[&str] = if cfg!(windows) { &["sh.exe"] } else { &["sh"] };
        for dir in std::env::split_paths(&path) {
            for n in names {
                let p = dir.join(n);
                if p.is_file() {
                    return Ok(p);
                }
            }
        }
        Err(())
    }

    #[test]
    fn generate_then_reimport_roundtrips() {
        for alg in [KeyAlgorithm::Ed25519, KeyAlgorithm::EcdsaP256] {
            let k = generate(alg, "me@laptop").unwrap();
            assert!(k.public_key.ends_with("me@laptop"));
            assert!(k.fingerprint.starts_with("SHA256:"));
            let again = import(k.private_openssh.expose_secret(), None).unwrap();
            assert_eq!(again.fingerprint, k.fingerprint);
            assert!(load(&k.private_openssh).is_ok());
        }
    }

    #[test]
    fn rejects_garbage_and_reports_encrypted_keys() {
        assert_eq!(
            import("hello", None).err().map(|e| e.code),
            Some(ErrorCode::InvalidInput)
        );

        let k = generate(KeyAlgorithm::Ed25519, "").unwrap();
        let key = decode_secret_key(k.private_openssh.expose_secret(), None).unwrap();
        let encrypted = key.encrypt(&mut rand::rng(), "pass phrase").unwrap();
        let text = encrypted.to_openssh(LineEnding::LF).unwrap();
        assert_eq!(
            import(&text, None).err().map(|e| e.code),
            Some(ErrorCode::PassphraseRequired)
        );
        assert_eq!(
            import(&text, Some("wrong")).err().map(|e| e.code),
            Some(ErrorCode::InvalidInput)
        );
        assert_eq!(
            import(&text, Some("pass phrase")).unwrap().fingerprint,
            k.fingerprint
        );
    }
}
