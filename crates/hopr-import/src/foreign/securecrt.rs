//! SecureCRT: one `.ini` file per session, in nested folders.

use std::collections::HashMap;

use hopr_core::model::{Parity, Protocol, SerialConfig, SerialFlow};

use super::*;

/// `C:\keys\id:RAW` → `C:\keys\id` (SecureCRT appends a key-format suffix).
pub(super) fn strip_key_format(f: &str) -> &str {
    match f.rsplit_once(':') {
        Some((path, fmt))
            if !fmt.is_empty() && fmt.len() <= 8 && fmt.chars().all(|c| c.is_ascii_uppercase()) =>
        {
            path
        }
        _ => f,
    }
}

/// One SecureCRT session file (`Sessions/<folder>/<name>.ini`).
pub fn parse_securecrt_session(
    name: &str,
    group: Option<&str>,
    text: &str,
) -> Result<ForeignHost, String> {
    let mut s: HashMap<String, String> = HashMap::new();
    let mut d: HashMap<String, u32> = HashMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if let Some(k) = key.strip_prefix("S:\"").and_then(|k| k.strip_suffix('"')) {
            s.insert(k.to_owned(), value.trim().to_owned());
        } else if let Some(k) = key.strip_prefix("D:\"").and_then(|k| k.strip_suffix('"'))
            && let Ok(n) = u32::from_str_radix(value.trim(), 16)
        {
            d.insert(k.to_owned(), n);
        }
    }
    let get = |k: &str| s.get(k).filter(|v| !v.is_empty()).cloned();
    let proto = get("Protocol Name").unwrap_or_else(|| "SSH2".into());
    let base = |protocol, hostname: String, port| ForeignHost {
        label: name.to_owned(),
        protocol,
        hostname,
        port,
        username: get("Username").unwrap_or_default(),
        group: group.map(str::to_owned),
        serial: None,
        key_file: get("Identity Filename V2")
            .map(|f| strip_key_format(&f).to_owned())
            .filter(|f| !f.is_empty()),
        proxy_command: None,
    };
    let port16 = |k: &str, dflt: u16| {
        d.get(k)
            .and_then(|p| u16::try_from(*p).ok())
            .filter(|p| *p > 0)
            .unwrap_or(dflt)
    };
    match proto.as_str() {
        "SSH2" | "SSH1" => {
            let host = get("Hostname").ok_or("no host name")?;
            Ok(base(Protocol::Ssh, host, port16("[SSH2] Port", 22)))
        }
        "Telnet" => {
            let host = get("Hostname").ok_or("no host name")?;
            Ok(base(Protocol::Telnet, host, port16("Port", 23)))
        }
        "Serial" => {
            let port = get("Com Port").ok_or("no serial port")?;
            let mut h = base(Protocol::Serial, port, 22);
            h.username = String::new();
            h.serial = Some(SerialConfig {
                baud: d.get("Baud Rate").copied().unwrap_or(9600),
                data_bits: d
                    .get("Data Bits")
                    .and_then(|b| u8::try_from(*b).ok())
                    .unwrap_or(8),
                parity: match d.get("Parity") {
                    Some(1) => Parity::Odd,
                    Some(2) => Parity::Even,
                    _ => Parity::None,
                },
                stop_bits: if d.get("Stop Bits") == Some(&2) { 2 } else { 1 },
                flow: if d.get("CTS Flow") == Some(&1) {
                    SerialFlow::Hardware
                } else if d.get("XON Flow") == Some(&1) {
                    SerialFlow::Software
                } else {
                    SerialFlow::None
                },
            });
            Ok(h)
        }
        other => Err(format!("{other} sessions aren't supported")),
    }
}

/// Every session under a SecureCRT `Sessions` folder; sub-folders become groups.
pub fn read_securecrt_dir(root: &std::path::Path) -> ForeignImport {
    let mut out = ForeignImport::default();
    let mut stack = vec![(root.to_path_buf(), None::<String>)];
    while let Some((dir, group)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            out.warnings
                .push(format!("Couldn't read {}", dir.display()));
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for e in entries {
            let path = e.path();
            let file = e.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                let g = match &group {
                    Some(g) => format!("{g} / {file}"),
                    None => file,
                };
                stack.push((path, Some(g)));
                continue;
            }
            let Some(name) = file.strip_suffix(".ini") else {
                continue;
            };
            if name.starts_with("__") || name.eq_ignore_ascii_case("Default") {
                continue;
            }
            let text = std::fs::read(&path)
                .map(|b| decode_text(&b))
                .unwrap_or_default();
            match parse_securecrt_session(name, group.as_deref(), &text) {
                Ok(h) => out.hosts.push(h),
                Err(why) => out.warnings.push(format!("{name}: {why}")),
            }
        }
    }
    out
}
