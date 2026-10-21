//! Host export (H8): OpenSSH `ssh_config` and CSV. Secrets are never exported.

use std::collections::HashSet;
use std::fmt::Write as _;

/// One host to export.
pub struct ExportHost<'a> {
    /// Display label (becomes the `Host` alias).
    pub label: &'a str,
    /// Hostname or IP.
    pub hostname: &'a str,
    /// Port.
    pub port: u16,
    /// Remote user.
    pub username: &'a str,
    /// Label of the host to jump through, if any.
    pub jump_label: Option<&'a str>,
    /// Group name.
    pub group: Option<&'a str>,
    /// Tags.
    pub tags: &'a [String],
    /// Environment badge (`none`, `dev`, `staging`, `prod`).
    pub environment: &'a str,
    /// Proxy as `(protocol, host, port)`, protocol `socks5` or `http`.
    pub proxy: Option<(&'a str, &'a str, u16)>,
    /// Notes.
    pub notes: &'a str,
}

/// A label as an ssh_config alias: spaces become dashes, anything unusual is dropped.
pub fn alias(label: &str) -> String {
    let a: String = label
        .trim()
        .chars()
        .map(|c| if c.is_whitespace() { '-' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || "._-@".contains(*c))
        .collect();
    if a.is_empty() { "host".to_owned() } else { a }
}

fn quoted(v: &str) -> String {
    if v.chars().any(char::is_whitespace) {
        format!("\"{}\"", v.replace('"', ""))
    } else {
        v.to_owned()
    }
}

/// Render hosts as an `ssh_config` file. Duplicate labels get `-2`, `-3`… suffixes.
pub fn to_ssh_config(hosts: &[ExportHost<'_>]) -> String {
    let mut out =
        String::from("# Exported from Hopr. Passwords, keys and TOTP seeds are not included.\n");
    let mut used = HashSet::new();
    for h in hosts {
        let base = alias(h.label);
        let mut name = base.clone();
        let mut n = 2;
        while !used.insert(name.clone()) {
            name = format!("{base}-{n}");
            n += 1;
        }
        let _ = writeln!(out, "\nHost {name}");
        let _ = writeln!(out, "    HostName {}", h.hostname);
        let _ = writeln!(out, "    User {}", quoted(h.username));
        if h.port != 22 {
            let _ = writeln!(out, "    Port {}", h.port);
        }
        if let Some(j) = h.jump_label {
            let _ = writeln!(out, "    ProxyJump {}", alias(j));
        } else if let Some((proto, host, port)) = h.proxy {
            // OpenSSH has no proxy directive; netcat's -X does the same job.
            let x = if proto == "http" { "connect" } else { "5" };
            let _ = writeln!(out, "    ProxyCommand nc -X {x} -x {host}:{port} %h %p");
        }
    }
    out
}

/// One CSV field: quoted when needed, and defused against spreadsheet formulas.
fn field(v: &str) -> String {
    let v = if v.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{v}")
    } else {
        v.to_owned()
    };
    if v.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v
    }
}

/// Render hosts as CSV (RFC 4180, CRLF line ends).
pub fn to_csv(hosts: &[ExportHost<'_>]) -> String {
    let mut out = String::from(
        "label,hostname,port,username,group,tags,environment,jump_host,proxy,notes\r\n",
    );
    for h in hosts {
        let proxy = h
            .proxy
            .map(|(p, host, port)| format!("{p}://{host}:{port}"))
            .unwrap_or_default();
        let row = [
            field(h.label),
            field(h.hostname),
            h.port.to_string(),
            field(h.username),
            field(h.group.unwrap_or_default()),
            field(&h.tags.join(" ")),
            field(h.environment),
            field(h.jump_label.unwrap_or_default()),
            field(&proxy),
            field(h.notes),
        ];
        out.push_str(&row.join(","));
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    struct NoIncludes;

    impl crate::Includes for NoIncludes {
        fn read(&self, _: &Path) -> Option<String> {
            None
        }
        fn list(&self, _: &Path) -> Vec<PathBuf> {
            Vec::new()
        }
    }

    fn host<'a>(label: &'a str, tags: &'a [String]) -> ExportHost<'a> {
        ExportHost {
            label,
            hostname: "10.0.0.5",
            port: 22,
            username: "deploy",
            jump_label: None,
            group: None,
            tags,
            environment: "none",
            proxy: None,
            notes: "",
        }
    }

    #[test]
    fn ssh_config_has_aliases_jumps_and_proxies() {
        let tags = vec![];
        let mut db = host("prod db", &tags);
        db.port = 2222;
        db.jump_label = Some("edge bastion");
        let mut edge = host("edge bastion", &tags);
        edge.proxy = Some(("socks5", "proxy.corp", 1080));
        let dup = host("prod db", &tags);
        let text = to_ssh_config(&[db, edge, dup]);
        assert!(
            text.contains("\nHost prod-db\n    HostName 10.0.0.5\n    User deploy\n    Port 2222\n    ProxyJump edge-bastion\n"),
            "{text}"
        );
        assert!(text.contains("Host edge-bastion\n"), "{text}");
        assert!(
            text.contains("ProxyCommand nc -X 5 -x proxy.corp:1080 %h %p"),
            "{text}"
        );
        assert!(text.contains("\nHost prod-db-2\n"), "{text}");
    }

    #[test]
    fn ssh_config_output_parses_back() {
        let tags = vec![];
        let mut a = host("app", &tags);
        a.jump_label = Some("bastion");
        let text = to_ssh_config(&[host("bastion", &tags), a]);
        let parsed = crate::parse_ssh_config(
            &text,
            Path::new("/home/u/.ssh"),
            Path::new("/home/u"),
            &NoIncludes,
        );
        let app = parsed.hosts.iter().find(|h| h.alias == "app").unwrap();
        assert_eq!(app.hostname, "10.0.0.5");
        assert_eq!(app.proxy_jump.as_deref(), Some("bastion"));
    }

    #[test]
    fn csv_quotes_and_defuses_formulas() {
        let tags = vec!["db".to_owned(), "eu".to_owned()];
        let mut h = host("=cmd|' /C calc'!A0", &tags);
        h.notes = "line one, \"two\"\nline three";
        h.group = Some("EU");
        let text = to_csv(&[h]);
        let row = text.lines().nth(1).unwrap();
        assert!(
            row.starts_with("'=cmd|' /C calc'!A0,10.0.0.5,22,deploy,EU,db eu,none,,,"),
            "{row}"
        );
        assert!(
            text.contains("\"line one, \"\"two\"\"\nline three\""),
            "{text}"
        );
    }
}
