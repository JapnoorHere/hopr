//! Persistent domain model shared by the store, the IPC layer and the UI.

mod activity;
mod files;
mod host;
mod keys;
mod snippets;
mod tunnels;

pub use activity::*;
pub use files::*;
pub use host::*;
pub use keys::*;
pub use snippets::*;
pub use tunnels::*;

use serde::{Deserialize, Serialize};
use specta::Type;

/// A saved layout of tabs and splits (R6). `layout_json` is owned by the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    /// Id.
    pub id: String,
    /// Name ("Last session" for the autosave).
    pub name: String,
    /// Tabs, splits and pane targets as JSON. Never secrets.
    pub layout_json: String,
    /// The autosaved last session.
    pub autosave: bool,
    /// Opened at startup.
    pub startup: bool,
    /// Last saved (RFC 3339).
    pub updated_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> HostInput {
        HostInput {
            protocol: Default::default(),
            serial: None,
            proxy_command: None,
            label: " ".into(),
            hostname: " prod-db ".into(),
            port: 22,
            username: "deploy".into(),
            group_id: None,
            tags: vec!["DB".into(), "db".into(), " ".into(), "eu".into()],
            auth: AuthKind::Password,
            key_id: None,
            jump_host_id: None,
            agent_forwarding: false,
            proxy: None,
            proxy_password: SecretChange::Keep,
            favorite: false,
            environment: Environment::Prod,
            notes: String::new(),
            password: SecretChange::Keep,
            totp_seed: SecretChange::Keep,
        }
    }

    #[test]
    fn telnet_and_serial_rules() {
        // Telnet: no user needed; SSH-only settings are dropped.
        let mut t = input();
        t.protocol = Protocol::Telnet;
        t.username = String::new();
        t.port = 23;
        t.proxy_command = Some("nc x 1".into());
        let n = t.normalized().unwrap();
        assert!(n.username.is_empty() && n.proxy_command.is_none() && n.serial.is_none());

        // Serial: defaults to 9600 8N1, bad settings refused.
        let mut s = input();
        s.protocol = Protocol::Serial;
        s.hostname = "COM3".into();
        s.username = String::new();
        let n = s.clone().normalized().unwrap();
        assert_eq!(n.serial, Some(SerialConfig::default()));
        s.serial = Some(SerialConfig {
            stop_bits: 3,
            ..SerialConfig::default()
        });
        assert!(s.normalized().is_err());

        // SSH still needs a user; ProxyCommand is one line and excludes a jump host.
        let mut ssh = input();
        ssh.username = String::new();
        assert!(ssh.normalized().is_err());
        let mut pc = input();
        pc.proxy_command = Some("  ssh -W %h:%p jump  ".into());
        assert_eq!(
            pc.clone().normalized().unwrap().proxy_command.as_deref(),
            Some("ssh -W %h:%p jump")
        );
        pc.jump_host_id = Some("j".into());
        assert!(pc.clone().normalized().is_err());
        pc.jump_host_id = None;
        pc.proxy_command = Some("a\nb".into());
        assert!(pc.normalized().is_err());
    }

    #[test]
    fn validates_and_trims_the_proxy() {
        let mut i = input();
        i.proxy = Some(HostProxy {
            protocol: ProxyProtocol::Socks5,
            host: " proxy.corp ".into(),
            port: 1080,
            username: Some("  ".into()),
        });
        let n = i.normalized().unwrap();
        let p = n.proxy.unwrap();
        assert_eq!(p.host, "proxy.corp");
        assert_eq!(p.username, None);

        let mut i = input();
        i.proxy = Some(HostProxy {
            protocol: ProxyProtocol::Http,
            host: "".into(),
            port: 8080,
            username: None,
        });
        assert!(i.normalized().is_err());
    }

    #[test]
    fn normalizes_label_and_tags() {
        let n = input().normalized().unwrap();
        assert_eq!(n.hostname, "prod-db");
        assert_eq!(n.label, "prod-db");
        assert_eq!(n.tags, vec!["db", "eu"]);
    }

    #[test]
    fn rejects_bad_input() {
        let mut i = input();
        i.hostname = "two words".into();
        assert!(i.normalized().is_err());
        let mut i = input();
        i.auth = AuthKind::Key;
        assert!(i.normalized().is_err());
        let mut i = input();
        i.port = 0;
        assert!(i.normalized().is_err());
    }

    #[test]
    fn secret_change_serializes_tagged() {
        let v = serde_json::to_value(SecretChange::Set { value: "x".into() }).unwrap();
        assert_eq!(v["action"], "set");
    }
}
