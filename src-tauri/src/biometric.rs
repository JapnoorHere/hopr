//! Biometric unlock (X3). On Windows this is Windows Hello (face, fingerprint or PIN)
//! gating the vault key kept in Credential Manager; elsewhere it reports unavailable.

/// Settings key: unlocking with the remembered key requires a biometric check.
pub const BIOMETRIC_KEY: &str = "vault.biometric";

#[cfg(windows)]
mod imp {
    use windows::Security::Credentials::UI::{
        UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
    };
    use windows::core::HSTRING;

    pub fn available() -> bool {
        UserConsentVerifier::CheckAvailabilityAsync()
            .and_then(|op| op.join())
            .is_ok_and(|a| a == UserConsentVerifierAvailability::Available)
    }

    pub fn verify(message: &str) -> Result<bool, String> {
        let op = UserConsentVerifier::RequestVerificationAsync(&HSTRING::from(message))
            .map_err(|e| e.to_string())?;
        let result = op.join().map_err(|e| e.to_string())?;
        Ok(result == UserConsentVerificationResult::Verified)
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn available() -> bool {
        false
    }

    pub fn verify(_message: &str) -> Result<bool, String> {
        Err("Biometric unlock isn't available on this system".into())
    }
}

/// Whether a biometric check can run here (Windows Hello set up).
pub async fn available() -> bool {
    tokio::task::spawn_blocking(imp::available)
        .await
        .unwrap_or(false)
}

/// Ask the user to verify. `Ok(false)` means they cancelled or failed.
pub async fn verify(message: String) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || imp::verify(&message))
        .await
        .map_err(|e| e.to_string())?
}
