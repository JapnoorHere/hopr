# ADR 0005: Vault crypto design

**Status:** Accepted · 2026-10-16

## Decision
- Master password → **Argon2id** (64 MiB, t=3, p=1, calibrated to about 500 ms on first run) → KEK.
- A random 256-bit **DEK** encrypts secret fields with **XChaCha20-Poly1305** (random 24-byte nonce per field).
- The DEK is stored wrapped by the KEK, and separately wrapped by a **recovery key** shown once at vault creation.
- Changing the password re-wraps the DEK. Data is not re-encrypted.
- The DEK is held in `secrecy::SecretBox` in memory and zeroized on lock.
- Optional "remember on this device" or biometrics store the KEK in the OS keychain (`keyring`).

## Consequences
Disk theft yields only ciphertext. A lost password plus a lost recovery key means the data is unrecoverable, and the UI says so plainly.
