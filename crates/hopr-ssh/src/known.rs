//! Host key verification against the trusted-keys store (TOFU, HOPR_PLAN §9.15).
//! Understands plain entries and OpenSSH hashed patterns (`|1|salt|hash`).

use data_encoding::BASE64;
use hmac::{Hmac, Mac};
use hopr_core::model::KnownHost;
use sha1::Sha1;

/// Outcome of comparing a presented key with what is trusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyStatus {
    /// Exactly this key is trusted for this host.
    Trusted,
    /// Nothing is trusted for this host yet.
    Unknown,
    /// Other keys are trusted for this host: possible man-in-the-middle.
    Changed {
        /// Fingerprints of the previously trusted keys.
        previous: Vec<String>,
    },
}

/// The name OpenSSH uses for `host:port` in known_hosts.
pub fn openssh_name(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_ascii_lowercase()
    } else {
        format!("[{}]:{port}", host.to_ascii_lowercase())
    }
}

fn hashed_matches(pattern: &str, name: &str) -> bool {
    let Some(rest) = pattern.strip_prefix("|1|") else {
        return false;
    };
    let Some((salt_b64, hash_b64)) = rest.split_once('|') else {
        return false;
    };
    let (Ok(salt), Ok(hash)) = (
        BASE64.decode(salt_b64.as_bytes()),
        BASE64.decode(hash_b64.as_bytes()),
    ) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha1>::new_from_slice(&salt) else {
        return false;
    };
    mac.update(name.as_bytes());
    mac.verify_slice(&hash).is_ok()
}

/// Whether a stored entry applies to `host:port`.
pub fn entry_matches(entry: &KnownHost, host: &str, port: u16) -> bool {
    if entry.host_pattern.starts_with("|1|") {
        hashed_matches(&entry.host_pattern, &openssh_name(host, port))
    } else {
        entry.port == port && entry.host_pattern.eq_ignore_ascii_case(host)
    }
}

/// Compare a presented key (`key_type` + base64 blob) with trusted entries.
pub fn check(
    host: &str,
    port: u16,
    key_type: &str,
    key_b64: &str,
    entries: &[KnownHost],
) -> HostKeyStatus {
    let mut previous = Vec::new();
    for e in entries.iter().filter(|e| entry_matches(e, host, port)) {
        if e.key_type == key_type && e.key_b64 == key_b64 {
            return HostKeyStatus::Trusted;
        }
        previous.push(e.fingerprint.clone());
    }
    if previous.is_empty() {
        HostKeyStatus::Unknown
    } else {
        HostKeyStatus::Changed { previous }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(pattern: &str, port: u16, key: &str) -> KnownHost {
        KnownHost {
            id: "1".into(),
            host_pattern: pattern.into(),
            port,
            key_type: "ssh-ed25519".into(),
            key_b64: key.into(),
            fingerprint: format!("SHA256:{key}"),
            first_seen_at: String::new(),
            last_seen_at: String::new(),
            source: "tofu".into(),
        }
    }

    fn hashed(name: &str) -> String {
        let salt = b"0123456789abcdefghij";
        let mut mac = Hmac::<Sha1>::new_from_slice(salt).unwrap();
        mac.update(name.as_bytes());
        format!(
            "|1|{}|{}",
            BASE64.encode(salt),
            BASE64.encode(&mac.finalize().into_bytes())
        )
    }

    #[test]
    fn unknown_trusted_changed() {
        let e = vec![entry("prod-db", 22, "AAA")];
        assert_eq!(
            check("other", 22, "ssh-ed25519", "AAA", &e),
            HostKeyStatus::Unknown
        );
        assert_eq!(
            check("PROD-DB", 22, "ssh-ed25519", "AAA", &e),
            HostKeyStatus::Trusted
        );
        assert_eq!(
            check("prod-db", 22, "ssh-ed25519", "BBB", &e),
            HostKeyStatus::Changed {
                previous: vec!["SHA256:AAA".into()]
            }
        );
        assert_eq!(
            check("prod-db", 2222, "ssh-ed25519", "BBB", &e),
            HostKeyStatus::Unknown
        );
    }

    #[test]
    fn hashed_entries_match_name_and_port() {
        let e = vec![
            entry(&hashed("prod-db"), 0, "AAA"),
            entry(&hashed("[prod-db]:2222"), 0, "CCC"),
        ];
        assert_eq!(
            check("prod-db", 22, "ssh-ed25519", "AAA", &e),
            HostKeyStatus::Trusted
        );
        assert_eq!(
            check("prod-db", 2222, "ssh-ed25519", "CCC", &e),
            HostKeyStatus::Trusted
        );
        assert!(matches!(
            check("prod-db", 22, "ssh-ed25519", "ZZZ", &e),
            HostKeyStatus::Changed { .. }
        ));
        assert_eq!(
            check("other", 22, "ssh-ed25519", "AAA", &e),
            HostKeyStatus::Unknown
        );
    }
}
