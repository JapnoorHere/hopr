//! Importers (HOPR_PLAN §9.24): OpenSSH `~/.ssh/config` and `known_hosts`.
//! Pure parsing: callers read files and decide what to persist.

#![warn(missing_docs)]

pub mod export;
pub mod foreign;

use std::path::{Path, PathBuf};

use data_encoding::{BASE64, BASE64_NOPAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;

/// One host parsed from an OpenSSH client config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportedHost {
    /// The `Host` alias (becomes the label).
    pub alias: String,
    /// `HostName`, or the alias when absent.
    pub hostname: String,
    /// `Port` (default 22).
    pub port: u16,
    /// `User`, if set.
    pub user: Option<String>,
    /// `IdentityFile` paths, `~` expanded.
    pub identity_files: Vec<String>,
    /// `ProxyJump`, kept for display (jump chains arrive in Phase 3).
    pub proxy_jump: Option<String>,
    /// `ProxyCommand` (the whole command line; `none` means no command).
    pub proxy_command: Option<String>,
}

/// Result of parsing a config file tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfigImport {
    /// Concrete hosts (wildcard patterns are applied as defaults, not imported).
    pub hosts: Vec<ImportedHost>,
    /// Things skipped and why.
    pub warnings: Vec<String>,
}

#[derive(Default, Clone)]
struct Block {
    patterns: Vec<String>,
    hostname: Option<String>,
    port: Option<u16>,
    user: Option<String>,
    identity_files: Vec<String>,
    proxy_jump: Option<String>,
    proxy_command: Option<String>,
}

fn is_pattern(s: &str) -> bool {
    s.contains(['*', '?', '!'])
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    fn go(p: &[u8], t: &[u8]) -> bool {
        match (p.first(), t.first()) {
            (None, None) => true,
            (Some(b'*'), _) => go(&p[1..], t) || (!t.is_empty() && go(p, &t[1..])),
            (Some(b'?'), Some(_)) => go(&p[1..], &t[1..]),
            (Some(a), Some(b)) if a.eq_ignore_ascii_case(b) => go(&p[1..], &t[1..]),
            _ => false,
        }
    }
    go(pattern.as_bytes(), text.as_bytes())
}

fn expand_tilde(path: &str, home: &Path) -> String {
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        home.join(rest).to_string_lossy().into_owned()
    } else if path == "~" {
        home.to_string_lossy().into_owned()
    } else {
        path.to_owned()
    }
}

/// Split a config line into keyword and arguments (supports `Key=Value` and quotes).
fn split_line(line: &str) -> Option<(String, Vec<String>)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (key, rest) = match line.find(|c: char| c.is_whitespace() || c == '=') {
        Some(i) => (
            &line[..i],
            line[i..].trim_start_matches(|c: char| c.is_whitespace() || c == '='),
        ),
        None => (line, ""),
    };
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in rest.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        args.push(cur);
    }
    Some((key.to_ascii_lowercase(), args))
}

/// Reads `Include`d files.
pub trait Includes {
    /// Contents of `path`, if readable.
    fn read(&self, path: &Path) -> Option<String>;
    /// Files in `dir` (for `Include dir/*`).
    fn list(&self, dir: &Path) -> Vec<PathBuf>;
}

/// File-system backed includes.
pub struct FsIncludes;

impl Includes for FsIncludes {
    fn read(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }
    fn list(&self, dir: &Path) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.is_file())
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }
}

struct Ctx<'a> {
    ssh_dir: &'a Path,
    home: &'a Path,
    inc: &'a dyn Includes,
    blocks: Vec<Block>,
    warnings: Vec<String>,
}

impl Ctx<'_> {
    fn include(&mut self, arg: &str, depth: u8) {
        let expanded = expand_tilde(arg, self.home);
        let path = if Path::new(&expanded).is_absolute() {
            PathBuf::from(expanded)
        } else {
            self.ssh_dir.join(expanded)
        };
        let files: Vec<PathBuf> = match path.file_name().map(|f| f.to_string_lossy().into_owned()) {
            Some(name) if is_pattern(&name) => path
                .parent()
                .map(|dir| {
                    self.inc
                        .list(dir)
                        .into_iter()
                        .filter(|p| {
                            p.file_name()
                                .is_some_and(|f| wildcard_match(&name, &f.to_string_lossy()))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            _ => vec![path],
        };
        for f in files {
            match self.inc.read(&f) {
                Some(t) => self.parse(&t, depth + 1),
                None => self
                    .warnings
                    .push(format!("Couldn't read included file {}", f.display())),
            }
        }
    }

    fn parse(&mut self, text: &str, depth: u8) {
        let mut skipping_match = false;
        for raw in text.lines() {
            let Some((key, args)) = split_line(raw) else {
                continue;
            };
            match key.as_str() {
                "host" => {
                    skipping_match = false;
                    self.blocks.push(Block {
                        patterns: args,
                        ..Block::default()
                    });
                }
                "match" => {
                    skipping_match = true;
                    self.warnings.push(format!(
                        "Skipped a `Match {}` block (not supported)",
                        args.join(" ")
                    ));
                }
                "include" => {
                    if depth < 8 {
                        for arg in args {
                            self.include(&arg, depth);
                        }
                    }
                }
                _ if skipping_match => {}
                _ => {
                    if self.blocks.is_empty() {
                        // Options before any Host apply to everything.
                        self.blocks.push(Block {
                            patterns: vec!["*".into()],
                            ..Block::default()
                        });
                    }
                    let home = self.home;
                    let Some(b) = self.blocks.last_mut() else {
                        continue;
                    };
                    let first = args.first().cloned();
                    match key.as_str() {
                        "hostname" => b.hostname = b.hostname.take().or(first),
                        "user" => b.user = b.user.take().or(first),
                        "port" => b.port = b.port.or_else(|| first.and_then(|p| p.parse().ok())),
                        "identityfile" => b
                            .identity_files
                            .extend(first.map(|f| expand_tilde(&f, home))),
                        "proxyjump" => b.proxy_jump = b.proxy_jump.take().or(first),
                        // The rest of the line is the command; `none` cancels one set earlier.
                        "proxycommand" if b.proxy_command.is_none() => {
                            b.proxy_command = Some(args.join(" "));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

/// Parse an OpenSSH client config. `ssh_dir` resolves relative `Include`s; `home` expands `~`.
pub fn parse_ssh_config(
    text: &str,
    ssh_dir: &Path,
    home: &Path,
    inc: &dyn Includes,
) -> ConfigImport {
    let mut ctx = Ctx {
        ssh_dir,
        home,
        inc,
        blocks: Vec::new(),
        warnings: Vec::new(),
    };
    ctx.parse(text, 0);
    let Ctx {
        blocks, warnings, ..
    } = ctx;

    let mut hosts: Vec<ImportedHost> = Vec::new();
    for b in &blocks {
        for alias in b.patterns.iter().filter(|p| !is_pattern(p)) {
            if hosts.iter().any(|h| h.alias == *alias) {
                continue;
            }
            // OpenSSH semantics: first value wins, in file order, across all matching blocks.
            let mut h = Block::default();
            for other in &blocks {
                let applies = other.patterns.iter().any(|p| p == alias)
                    || other
                        .patterns
                        .iter()
                        .any(|p| is_pattern(p) && !p.starts_with('!') && wildcard_match(p, alias));
                if !applies {
                    continue;
                }
                h.hostname = h.hostname.or_else(|| other.hostname.clone());
                h.user = h.user.or_else(|| other.user.clone());
                h.port = h.port.or(other.port);
                h.proxy_jump = h.proxy_jump.or_else(|| other.proxy_jump.clone());
                h.proxy_command = h.proxy_command.or_else(|| other.proxy_command.clone());
                for f in &other.identity_files {
                    if !h.identity_files.contains(f) {
                        h.identity_files.push(f.clone());
                    }
                }
            }
            hosts.push(ImportedHost {
                alias: alias.clone(),
                hostname: h
                    .hostname
                    .unwrap_or_else(|| alias.clone())
                    .replace("%h", alias),
                port: h.port.unwrap_or(22),
                user: h.user,
                identity_files: h.identity_files,
                proxy_jump: h.proxy_jump,
                proxy_command: h.proxy_command.filter(|c| !c.eq_ignore_ascii_case("none")),
            });
        }
    }
    ConfigImport { hosts, warnings }
}

/// One trusted key parsed from a known_hosts file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportedKnownHost {
    /// Host name or hashed `|1|…` pattern.
    pub host_pattern: String,
    /// Port (0 = encoded in a hashed pattern).
    pub port: u16,
    /// Key algorithm.
    pub key_type: String,
    /// Base64 key blob.
    pub key_b64: String,
    /// "SHA256:…".
    pub fingerprint: String,
}

/// Result of parsing known_hosts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KnownHostsImport {
    /// Entries.
    pub entries: Vec<ImportedKnownHost>,
    /// Lines skipped and why.
    pub warnings: Vec<String>,
}

/// OpenSSH-style "SHA256:…" fingerprint of a base64 key blob.
pub fn fingerprint_b64(key_b64: &str) -> Option<String> {
    let blob = BASE64.decode(key_b64.as_bytes()).ok()?;
    Some(format!(
        "SHA256:{}",
        BASE64_NOPAD.encode(&Sha256::digest(&blob))
    ))
}

/// Parse an OpenSSH known_hosts file.
pub fn parse_known_hosts(text: &str) -> KnownHostsImport {
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('@') {
            warnings.push(format!(
                "Line {}: skipped {} entry",
                n + 1,
                line.split_whitespace().next().unwrap_or("@")
            ));
            continue;
        }
        let mut parts = line.split_whitespace();
        let (Some(hosts), Some(key_type), Some(key_b64)) =
            (parts.next(), parts.next(), parts.next())
        else {
            warnings.push(format!("Line {}: not a known_hosts entry", n + 1));
            continue;
        };
        let Some(fingerprint) = fingerprint_b64(key_b64) else {
            warnings.push(format!("Line {}: invalid key data", n + 1));
            continue;
        };
        for host in hosts.split(',') {
            let (pattern, port) = if host.starts_with("|1|") {
                (host.to_owned(), 0)
            } else if is_pattern(host) {
                warnings.push(format!("Line {}: skipped wildcard pattern {host}", n + 1));
                continue;
            } else if let Some(rest) = host.strip_prefix('[') {
                match rest.split_once("]:").map(|(h, p)| (h, p.parse::<u16>())) {
                    Some((h, Ok(p))) => (h.to_ascii_lowercase(), p),
                    _ => {
                        warnings.push(format!("Line {}: bad host {host}", n + 1));
                        continue;
                    }
                }
            } else {
                (host.to_ascii_lowercase(), 22)
            };
            entries.push(ImportedKnownHost {
                host_pattern: pattern,
                port,
                key_type: key_type.to_owned(),
                key_b64: key_b64.to_owned(),
                fingerprint: fingerprint.clone(),
            });
        }
    }
    KnownHostsImport { entries, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MemIncludes(HashMap<PathBuf, String>);

    impl Includes for MemIncludes {
        fn read(&self, path: &Path) -> Option<String> {
            self.0.get(path).cloned()
        }
        fn list(&self, dir: &Path) -> Vec<PathBuf> {
            let mut v: Vec<_> = self
                .0
                .keys()
                .filter(|p| p.parent() == Some(dir))
                .cloned()
                .collect();
            v.sort();
            v
        }
    }

    #[test]
    fn parses_hosts_defaults_includes_and_skips_patterns() {
        let home = PathBuf::from("/home/me");
        let dir = home.join(".ssh");
        let inc = MemIncludes(HashMap::from([(
            dir.join("conf.d").join("work.conf"),
            "Host build\n  HostName 10.0.0.9\n".to_owned(),
        )]));
        let cfg = "
# comment
Include conf.d/*.conf

Host prod-db prod-db-replica
    HostName db.internal
    User deploy
    Port 2222
    IdentityFile ~/.ssh/id_work
    ProxyJump bastion

Host bastion
    HostName=bastion.example.com

Match host *.corp
    User corp

Host *
    User fallback
    IdentityFile ~/.ssh/id_ed25519
";
        let r = parse_ssh_config(cfg, &dir, &home, &inc);
        let names: Vec<_> = r.hosts.iter().map(|h| h.alias.as_str()).collect();
        assert_eq!(
            names,
            vec!["build", "prod-db", "prod-db-replica", "bastion"]
        );

        let prod = &r.hosts[1];
        assert_eq!(prod.hostname, "db.internal");
        assert_eq!(prod.port, 2222);
        assert_eq!(prod.user.as_deref(), Some("deploy"));
        assert_eq!(prod.proxy_jump.as_deref(), Some("bastion"));
        assert_eq!(prod.identity_files.len(), 2, "own key plus Host * default");
        assert!(prod.identity_files[0].ends_with("id_work"));

        let bastion = &r.hosts[3];
        assert_eq!(bastion.hostname, "bastion.example.com");
        assert_eq!(
            bastion.user.as_deref(),
            Some("fallback"),
            "Host * default applies"
        );
        assert_eq!(r.hosts[0].hostname, "10.0.0.9");
        assert!(r.warnings.iter().any(|w| w.contains("Match")));
    }

    #[test]
    fn parses_known_hosts_variants() {
        let key = "AAAAC3NzaC1lZDI1NTE5AAAAIGxvbmdlbm91Z2hrZXlkYXRhZm9ydGVzdGluZzEyMw==";
        let text = format!(
            "prod-db,10.0.0.4 ssh-ed25519 {key}\n[bastion]:2222 ssh-ed25519 {key} comment\n|1|c2FsdA==|aGFzaA== ssh-ed25519 {key}\n@cert-authority *.corp ssh-ed25519 {key}\n*.wild ssh-ed25519 {key}\ngarbage\n"
        );
        let r = parse_known_hosts(&text);
        let pats: Vec<_> = r
            .entries
            .iter()
            .map(|e| (e.host_pattern.as_str(), e.port))
            .collect();
        assert_eq!(
            pats,
            vec![
                ("prod-db", 22),
                ("10.0.0.4", 22),
                ("bastion", 2222),
                ("|1|c2FsdA==|aGFzaA==", 0)
            ]
        );
        assert!(r.entries[0].fingerprint.starts_with("SHA256:"));
        assert_eq!(r.warnings.len(), 3);
    }

    #[test]
    fn wildcard_matching() {
        assert!(wildcard_match("*.corp", "a.corp"));
        assert!(wildcard_match("web-?", "web-1"));
        assert!(!wildcard_match("web-?", "web-12"));
    }
}
