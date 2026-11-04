//! Importers for other clients' saved sessions (§9.24): PuTTY (registry or `.reg`
//! export), MobaXterm (`.mxtsessions` / `MobaXterm.ini`), SecureCRT (session `.ini`
//! files) and CSV (Termius and spreadsheets). Pure parsing: callers read the files.
//! Passwords are never imported: these tools encrypt or obfuscate them per machine.

mod csv;
mod mobaxterm;
mod putty;
mod securecrt;

pub use csv::*;
pub use mobaxterm::*;
pub use putty::*;
pub use securecrt::*;

use hopr_core::model::{Protocol, SerialConfig};
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

#[cfg(test)]
mod tests {
    use super::*;
    use hopr_core::model::{Parity, SerialFlow};

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
