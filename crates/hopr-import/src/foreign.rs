//! Importers for other clients' saved sessions (§9.24): PuTTY (registry or `.reg`
//! export), MobaXterm (`.mxtsessions` / `MobaXterm.ini`), SecureCRT (session `.ini`
//! files) and CSV (Termius and spreadsheets). Pure parsing: callers read the files.
//! Passwords are never imported: these tools encrypt or obfuscate them per machine.

use std::collections::HashMap;

use hopr_core::model::{Parity, Protocol, SerialConfig, SerialFlow};
use serde::{Deserialize, Serialize};
use specta::Type;

/// Where sessions come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ForeignSource {
    /// PuTTY (KiTTY uses the same format).
    Putty,
    /// MobaXterm.
    Mobaxterm,
    /// SecureCRT.
    Securecrt,
    /// A CSV file (Termius export, spreadsheets).
    Csv,
}

impl ForeignSource {
    /// Tag added to imported hosts.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Putty => "putty",
            Self::Mobaxterm => "mobaxterm",
            Self::Securecrt => "securecrt",
            Self::Csv => "csv",
        }
    }
}

/// One session found in another client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ForeignHost {
    /// Session name (becomes the label).
    pub label: String,
    /// SSH, telnet or serial.
    pub protocol: Protocol,
    /// Hostname (serial: the port name).
    pub hostname: String,
    /// Port (22/23 by default).
    pub port: u16,
    /// User, if saved.
    pub username: String,
    /// Folder the session was in (becomes a group).
    pub group: Option<String>,
    /// Serial line settings.
    pub serial: Option<SerialConfig>,
    /// Private key file the session used, if any.
    pub key_file: Option<String>,
    /// Local proxy command, translated to `%h`/`%p`/`%r`.
    pub proxy_command: Option<String>,
}

/// What a file held.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ForeignImport {
    /// Sessions that can be imported.
    pub hosts: Vec<ForeignHost>,
    /// What was skipped and why.
    pub warnings: Vec<String>,
}

/// Text from a file that may be UTF-16 (regedit exports) or UTF-8 with a BOM.
pub fn decode_text(bytes: &[u8]) -> String {
    let utf16 = |be: bool| {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| {
                if be {
                    u16::from_be_bytes([c[0], c[1]])
                } else {
                    u16::from_le_bytes([c[0], c[1]])
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xFF, 0xFE, ..] => utf16(false),
        [0xFE, 0xFF, ..] => utf16(true),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// `%XX` escapes (PuTTY session names).
fn unescape_percent(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(b) = bytes
                .get(i + 1..i + 3)
                .and_then(|h| std::str::from_utf8(h).ok())
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `user@host` in a host field: split out the user.
fn split_user(host: &str) -> (Option<String>, String) {
    match host.rsplit_once('@') {
        Some((u, h)) if !u.is_empty() && !h.is_empty() => (Some(u.to_owned()), h.to_owned()),
        _ => (None, host.to_owned()),
    }
}

// ---------- PuTTY ----------

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

// ---------- MobaXterm ----------

/// Parse MobaXterm bookmarks (`.mxtsessions` export or the `[Bookmarks*]` sections of
/// `MobaXterm.ini`). SSH and telnet sessions are imported; other kinds are counted.
pub fn parse_mobaxterm(text: &str) -> ForeignImport {
    let mut out = ForeignImport::default();
    let mut in_bookmarks = false;
    let mut folder: Option<String> = None;
    let mut skipped = 0usize;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_bookmarks = line.to_ascii_lowercase().starts_with("[bookmarks");
            folder = None;
            continue;
        }
        if !in_bookmarks {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim();
        match name {
            "SubRep" => {
                folder = Some(value.trim().replace('\\', " / ")).filter(|f| !f.is_empty());
                continue;
            }
            "ImgNum" | "" => continue,
            _ => {}
        }
        // `#<icon>#<kind>%host%port%user%…#<terminal settings>#…`
        let mut parts = value.trim().splitn(3, '#');
        let (Some(""), Some(_icon), Some(rest)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let settings = rest.split('#').next().unwrap_or_default();
        let fields: Vec<&str> = settings.split('%').collect();
        let (protocol, default_port) = match fields.first().copied().unwrap_or_default() {
            "0" => (Protocol::Ssh, 22),
            "1" => (Protocol::Telnet, 23),
            _ => {
                skipped += 1;
                continue;
            }
        };
        let host = fields.get(1).copied().unwrap_or_default().trim();
        if host.is_empty() {
            out.warnings.push(format!("{name}: no host"));
            continue;
        }
        out.hosts.push(ForeignHost {
            label: name.to_owned(),
            protocol,
            hostname: host.to_owned(),
            port: fields
                .get(2)
                .and_then(|p| p.trim().parse().ok())
                .filter(|p| *p > 0)
                .unwrap_or(default_port),
            username: fields
                .get(3)
                .map(|u| u.trim().to_owned())
                .unwrap_or_default(),
            group: folder.clone(),
            serial: None,
            key_file: None,
            proxy_command: None,
        });
    }
    if skipped > 0 {
        out.warnings.push(format!(
            "{skipped} session{} of other kinds (RDP, VNC, FTP, …) skipped",
            if skipped == 1 { "" } else { "s" }
        ));
    }
    out
}

// ---------- SecureCRT ----------

/// `C:\keys\id:RAW` → `C:\keys\id` (SecureCRT appends a key-format suffix).
fn strip_key_format(f: &str) -> &str {
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

// ---------- CSV ----------

/// Split CSV text into rows (quotes, doubled quotes and embedded newlines handled).
fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') if field.is_empty() => quoted = true,
            (false, ',' | ';') => row.push(std::mem::take(&mut field)),
            (false, '\r') => {}
            (false, '\n') => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (false, c) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| r.iter().any(|f| !f.trim().is_empty()));
    rows
}

/// A CSV with a header row. Recognized columns (any case): label/name/alias,
/// hostname/host/address/ip, port, username/user/login, group/folder, protocol.
pub fn parse_csv(text: &str) -> ForeignImport {
    let mut out = ForeignImport::default();
    let rows = csv_rows(text);
    let Some((header, body)) = rows.split_first() else {
        out.warnings.push("The file is empty".into());
        return out;
    };
    let col = |names: &[&str]| {
        header
            .iter()
            .position(|h| names.contains(&h.trim().to_ascii_lowercase().as_str()))
    };
    let Some(host_col) = col(&["hostname", "host", "address", "ip", "host name"]) else {
        out.warnings
            .push("No hostname column (expected a header like \"hostname\" or \"host\")".into());
        return out;
    };
    let (label_c, port_c, user_c, group_c, proto_c) = (
        col(&["label", "name", "alias", "title"]),
        col(&["port"]),
        col(&["username", "user", "login"]),
        col(&["group", "folder", "groups"]),
        col(&["protocol", "type"]),
    );
    for (i, r) in body.iter().enumerate() {
        let cell = |c: Option<usize>| {
            c.and_then(|c| r.get(c))
                .map(|v| v.trim().to_owned())
                .unwrap_or_default()
        };
        let (user_in_host, hostname) = split_user(&cell(Some(host_col)));
        if hostname.is_empty() {
            out.warnings.push(format!("Row {}: no hostname", i + 2));
            continue;
        }
        let protocol = match cell(proto_c).to_ascii_lowercase().as_str() {
            "telnet" => Protocol::Telnet,
            "" | "ssh" | "ssh2" => Protocol::Ssh,
            other => {
                out.warnings
                    .push(format!("Row {}: {other} isn't supported", i + 2));
                continue;
            }
        };
        let default_port = if protocol == Protocol::Telnet { 23 } else { 22 };
        let label = Some(cell(label_c))
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| hostname.clone());
        out.hosts.push(ForeignHost {
            label,
            protocol,
            port: cell(port_c)
                .parse()
                .ok()
                .filter(|p| *p > 0)
                .unwrap_or(default_port),
            username: Some(cell(user_c))
                .filter(|u| !u.is_empty())
                .or(user_in_host)
                .unwrap_or_default(),
            hostname,
            group: Some(cell(group_c)).filter(|g| !g.is_empty()),
            serial: None,
            key_file: None,
            proxy_command: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PUTTY_REG: &str = r#"Windows Registry Editor Version 5.00

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions]

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\Default%20Settings]
"HostName"=""

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\prod%20db]
"HostName"="deploy@10.0.0.4"
"PortNumber"=dword:00000822
"Protocol"="ssh"
"PublicKeyFile"="C:\\keys\\prod.ppk"
"ProxyMethod"=dword:00000005
"ProxyTelnetCommand"="plink jump -nc %host:%port"

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\core-switch]
"HostName"="192.168.1.1"
"PortNumber"=dword:00000017
"Protocol"="telnet"
"UserName"="admin"

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\console]
"Protocol"="serial"
"SerialLine"="COM3"
"SerialSpeed"=dword:0001c200
"SerialDataBits"=dword:00000007
"SerialStopHalfbits"=dword:00000004
"SerialParity"=dword:00000002
"SerialFlowControl"=dword:00000002

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\old-rlogin]
"HostName"="legacy"
"Protocol"="rlogin"
"#;

    #[test]
    fn putty_reg_export() {
        let got = from_putty(parse_putty_reg(PUTTY_REG));
        assert_eq!(got.hosts.len(), 3, "{got:?}");
        let prod = &got.hosts[0];
        assert_eq!(prod.label, "prod db");
        assert_eq!(
            (prod.hostname.as_str(), prod.port, prod.username.as_str()),
            ("10.0.0.4", 2082, "deploy")
        );
        assert_eq!(prod.key_file.as_deref(), Some("C:\\keys\\prod.ppk"));
        assert_eq!(prod.proxy_command.as_deref(), Some("plink jump -nc %h:%p"));
        let sw = &got.hosts[1];
        assert_eq!(
            (sw.protocol, sw.port, sw.username.as_str()),
            (Protocol::Telnet, 23, "admin")
        );
        let con = &got.hosts[2];
        assert_eq!(con.hostname, "COM3");
        assert_eq!(
            con.serial,
            Some(SerialConfig {
                baud: 115_200,
                data_bits: 7,
                parity: Parity::Even,
                stop_bits: 2,
                flow: SerialFlow::Hardware
            })
        );
        assert_eq!(
            got.warnings,
            ["old-rlogin: rlogin sessions aren't supported"]
        );
    }

    #[test]
    fn utf16_regedit_files_decode() {
        let mut bytes = vec![0xFF, 0xFE];
        for u in PUTTY_REG.encode_utf16() {
            bytes.extend(u.to_le_bytes());
        }
        let got = from_putty(parse_putty_reg(&decode_text(&bytes)));
        assert_eq!(got.hosts.len(), 3);
        assert_eq!(decode_text(b"\xEF\xBB\xBFhi"), "hi");
    }

    #[test]
    fn mobaxterm_bookmarks() {
        let text = "[Misc]\nfoo=bar\n[Bookmarks]\nSubRep=\nImgNum=42\n\
                    web-1= #109#0%10.1.0.5%2222%ubuntu%%-1%-1%%%%%0%0%0#MobaFont%10%0#0# #-1\n\
                    win= #91#4%10.1.0.9%3389%admin%0#MobaFont#0\n\
                    [Bookmarks_1]\nSubRep=Lab\\Routers\nImgNum=41\n\
                    r1= #98#1%192.168.0.1%23%cisco%%2#MobaFont%10#0# #-1\n";
        let got = parse_mobaxterm(text);
        assert_eq!(got.hosts.len(), 2, "{got:?}");
        assert_eq!(
            (
                got.hosts[0].label.as_str(),
                got.hosts[0].port,
                got.hosts[0].username.as_str()
            ),
            ("web-1", 2222, "ubuntu")
        );
        assert_eq!(got.hosts[0].group, None);
        assert_eq!(got.hosts[1].protocol, Protocol::Telnet);
        assert_eq!(got.hosts[1].group.as_deref(), Some("Lab / Routers"));
        assert_eq!(got.warnings.len(), 1);
        assert!(got.warnings[0].starts_with("1 session of other kinds"));
    }

    #[test]
    fn securecrt_sessions() {
        let ssh = "S:\"Protocol Name\"=SSH2\nS:\"Hostname\"=10.2.0.4\nS:\"Username\"=ops\n\
                   D:\"[SSH2] Port\"=00000016\nS:\"Identity Filename V2\"=C:\\k\\id_ed25519:RAW\n";
        let h = parse_securecrt_session("db", Some("Prod"), ssh).unwrap();
        assert_eq!(
            (h.hostname.as_str(), h.port, h.username.as_str()),
            ("10.2.0.4", 22, "ops")
        );
        assert_eq!(h.group.as_deref(), Some("Prod"));
        assert_eq!(h.key_file.as_deref(), Some("C:\\k\\id_ed25519"));
        let serial = "S:\"Protocol Name\"=Serial\nS:\"Com Port\"=COM4\nD:\"Baud Rate\"=00002580\n\
                      D:\"Data Bits\"=00000008\nD:\"Parity\"=00000000\nD:\"Stop Bits\"=00000000\n\
                      D:\"CTS Flow\"=00000001\n";
        let s = parse_securecrt_session("console", None, serial).unwrap();
        assert_eq!(s.hostname, "COM4");
        assert_eq!(
            s.serial.map(|c| (c.baud, c.flow)),
            Some((9600, SerialFlow::Hardware))
        );
        assert!(parse_securecrt_session("x", None, "S:\"Protocol Name\"=RDP\n").is_err());
    }

    #[test]
    fn securecrt_folder_tree() {
        let root = std::env::temp_dir().join(format!("hopr-scrt-{}", std::process::id()));
        let sub = root.join("Prod").join("DB");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(root.join("__FolderData__.ini"), "x").unwrap();
        std::fs::write(root.join("Default.ini"), "S:\"Hostname\"=nope\n").unwrap();
        std::fs::write(
            sub.join("pg-1.ini"),
            "S:\"Protocol Name\"=SSH2\nS:\"Hostname\"=pg-1.corp\n",
        )
        .unwrap();
        let got = read_securecrt_dir(&root);
        assert_eq!(got.hosts.len(), 1, "{got:?}");
        assert_eq!(got.hosts[0].group.as_deref(), Some("Prod / DB"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn csv_with_quotes_and_user_at_host() {
        let text = "Label,Address,Port,User,Group,Protocol\n\
                    \"web, eu\",10.0.0.1,,deploy,Web,ssh\n\
                    switch,admin@192.168.1.2,,,Net,telnet\n\
                    ,,,,,\n\
                    bad,10.0.0.9,,,,rdp\n";
        let got = parse_csv(text);
        assert_eq!(got.hosts.len(), 2, "{got:?}");
        assert_eq!(got.hosts[0].label, "web, eu");
        assert_eq!(got.hosts[0].port, 22);
        assert_eq!(got.hosts[1].username, "admin");
        assert_eq!(
            (got.hosts[1].protocol, got.hosts[1].port),
            (Protocol::Telnet, 23)
        );
        assert_eq!(got.warnings.len(), 1);
        assert!(parse_csv("name,port\nx,22\n").hosts.is_empty());
    }
}
