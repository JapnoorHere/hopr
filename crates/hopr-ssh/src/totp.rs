//! RFC 6238 TOTP (SHA-1, 30 s, 6 digits): the parameters every authenticator app uses.

use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};
use sha1::Sha1;
use zeroize::Zeroizing;

/// Decode a base32 seed as shown by servers (spaces, lowercase and padding tolerated).
pub fn decode_seed(seed: &str) -> Option<Zeroizing<Vec<u8>>> {
    let cleaned: String = seed
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '=')
        .collect::<String>()
        .to_uppercase();
    BASE32_NOPAD
        .decode(cleaned.as_bytes())
        .ok()
        .filter(|b| !b.is_empty())
        .map(Zeroizing::new)
}

/// HOTP value for a counter.
fn hotp(secret: &[u8], counter: u64, digits: u32) -> Option<String> {
    let mut mac = Hmac::<Sha1>::new_from_slice(secret).ok()?;
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = usize::from(digest.last()? & 0x0f);
    let bytes: [u8; 4] = digest.get(offset..offset + 4)?.try_into().ok()?;
    let code = (u32::from_be_bytes(bytes) & 0x7fff_ffff) % 10u32.pow(digits);
    Some(format!("{code:0width$}", width = digits as usize))
}

/// Code for `unix_time`.
pub fn code_at(seed: &str, unix_time: u64) -> Option<String> {
    hotp(&decode_seed(seed)?, unix_time / 30, 6)
}

/// Code for now.
pub fn current_code(seed: &str) -> Option<String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    code_at(seed, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6238_vectors() {
        // Secret "12345678901234567890"; RFC 6238 Appendix B (8 digits), last 6 shown here.
        let seed = BASE32_NOPAD.encode(b"12345678901234567890");
        let secret = decode_seed(&seed).unwrap();
        assert_eq!(hotp(&secret, 59 / 30, 8).unwrap(), "94287082");
        assert_eq!(hotp(&secret, 1_111_111_109 / 30, 8).unwrap(), "07081804");
        assert_eq!(code_at(&seed, 59).unwrap(), "287082");
    }

    #[test]
    fn tolerant_seed_parsing() {
        assert!(decode_seed("jbsw y3dp ehpk 3pxp").is_some());
        assert!(decode_seed("not base32!").is_none());
        assert!(decode_seed("").is_none());
    }
}
