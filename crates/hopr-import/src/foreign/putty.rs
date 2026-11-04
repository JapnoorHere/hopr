//! PuTTY: sessions from the registry or a `.reg` export.

use std::collections::HashMap;

use hopr_core::model::{Parity, Protocol, SerialConfig, SerialFlow};

use super::*;

/// A value in a PuTTY session (registry string or DWORD).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PuttyValue {
    /// REG_SZ.
    Str(String),
    /// REG_DWORD.
    Dword(u32),
}

/// Sessions read from the registry or a `.reg` file: `(escaped name, values)`.
pub type PuttySessions = Vec<(String, HashMap<String, PuttyValue>)>;

/// Parse a regedit export of `HKCU\Software\SimonTatham\PuTTY\Sessions`.
pub fn parse_putty_reg(text: &str) -> PuttySessions {
    const MARK: &str = "\\putty\\sessions\\";
    let mut sessions: PuttySessions = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(key) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let name = key
                .to_ascii_lowercase()
                .find(MARK)
                .map(|i| &key[i + MARK.len()..])
                .filter(|n| !n.is_empty() && !n.contains('\\'))
                .unwrap_or_default();
            // Keys we don't read get an empty name and are dropped below.
            sessions.push((name.to_owned(), HashMap::new()));
            continue;
        }
        let Some((_, values)) = sessions.last_mut() else {
            continue;
        };
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim().trim_matches('"').to_owned();
        let value = value.trim();
        if let Some(hex) = value.strip_prefix("dword:") {
            if let Ok(n) = u32::from_str_radix(hex.trim(), 16) {
                values.insert(name, PuttyValue::Dword(n));
            }
        } else if let Some(s) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
            values.insert(
                name,
                PuttyValue::Str(s.replace("\\\"", "\"").replace("\\\\", "\\")),
            );
        }
    }
    sessions.retain(|(n, _)| !n.is_empty());
    sessions
}

/// PuTTY sessions → hosts.
pub fn from_putty(sessions: PuttySessions) -> ForeignImport {
    let mut out = ForeignImport::default();
    for (raw_name, v) in sessions {
        let name = unescape_percent(&raw_name);
        let s = |k: &str| match v.get(k) {
            Some(PuttyValue::Str(s)) => Some(s.trim().to_owned()).filter(|s| !s.is_empty()),
            _ => None,
        };
        let d = |k: &str| match v.get(k) {
            Some(PuttyValue::Dword(n)) => Some(*n),
            _ => None,
        };
        let protocol = match s("Protocol").as_deref().unwrap_or("ssh") {
            "ssh" => Protocol::Ssh,
            "telnet" => Protocol::Telnet,
            "serial" => Protocol::Serial,
            other => {
                out.warnings
                    .push(format!("{name}: {other} sessions aren't supported"));
                continue;
            }
        };
        if protocol == Protocol::Serial {
            let Some(line) = s("SerialLine") else {
                out.warnings.push(format!("{name}: no serial line set"));
                continue;
            };
            let serial = SerialConfig {
                baud: d("SerialSpeed").unwrap_or(9600),
                data_bits: d("SerialDataBits")
                    .and_then(|b| u8::try_from(b).ok())
                    .unwrap_or(8),
                parity: match d("SerialParity") {
                    Some(1) => Parity::Odd,
                    Some(2) => Parity::Even,
                    _ => Parity::None,
                },
                // PuTTY counts stop bits in halves: 2 = one, 4 = two.
                stop_bits: if d("SerialStopHalfbits") == Some(4) {
                    2
                } else {
                    1
                },
                flow: match d("SerialFlowControl") {
                    Some(1) => SerialFlow::Software,
                    Some(2) => SerialFlow::Hardware,
                    _ => SerialFlow::None,
                },
            };
            out.hosts.push(ForeignHost {
                label: name,
                protocol,
                hostname: line,
                port: 22,
                username: String::new(),
                group: None,
                serial: Some(serial),
                key_file: None,
                proxy_command: None,
            });
            continue;
        }
        let Some(host_field) = s("HostName") else {
            if raw_name != "Default%20Settings" {
                out.warnings.push(format!("{name}: no host name"));
            }
            continue;
        };
        let (user_in_host, hostname) = split_user(&host_field);
        let default_port = if protocol == Protocol::Telnet { 23 } else { 22 };
        // ProxyMethod 5 = "Local" command, with PuTTY's own %host/%port/%user tokens.
        let proxy_command = (d("ProxyMethod") == Some(5))
            .then(|| s("ProxyTelnetCommand"))
            .flatten()
            .map(|c| {
                c.replace("%host", "%h")
                    .replace("%port", "%p")
                    .replace("%user", "%r")
            });
        out.hosts.push(ForeignHost {
            label: name,
            protocol,
            hostname,
            port: d("PortNumber")
                .and_then(|p| u16::try_from(p).ok())
                .filter(|p| *p > 0)
                .unwrap_or(default_port),
            username: s("UserName").or(user_in_host).unwrap_or_default(),
            group: None,
            serial: None,
            key_file: s("PublicKeyFile"),
            proxy_command,
        });
    }
    out
}
