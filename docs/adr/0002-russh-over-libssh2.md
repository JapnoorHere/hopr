# ADR 0002: russh over libssh2

**Status:** Accepted · 2026-10-16

## Context
We need SSH with async I/O, port forwarding, agent support and jump hosts, and it has to cross-compile to Windows, macOS, Linux and mobile.

## Decision
Use `russh` + `russh-sftp`: pure Rust, tokio-native, with no OpenSSL or libssh2 linking.

## Consequences
- Simple cross-compilation and a single async model.
- It's less battle-tested than libssh2 against edge servers (dropbear, network gear), so we keep a `hopr-ssh` abstraction and an integration-test server matrix (Risk R1).
- FIDO2/sk key support needs a spike (Risk R2).
