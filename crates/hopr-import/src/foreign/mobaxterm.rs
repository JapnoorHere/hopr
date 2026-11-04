//! MobaXterm: `.mxtsessions` exports and `MobaXterm.ini`.

use hopr_core::model::Protocol;

use super::*;

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
