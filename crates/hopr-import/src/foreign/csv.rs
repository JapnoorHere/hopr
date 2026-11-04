//! CSV: Termius exports and hand-made spreadsheets.

use hopr_core::model::Protocol;

use super::*;

/// Split CSV text into rows (quotes, doubled quotes and embedded newlines handled).
pub(super) fn csv_rows(text: &str) -> Vec<Vec<String>> {
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
