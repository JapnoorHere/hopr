//! How a saved host is reached: credentials, trust prompts, jump hops and proxies.

use std::sync::Arc;
use std::time::Duration;

use hopr_core::AppError;
use hopr_core::model::{AuthKind, Host, Protocol, ProxyProtocol};
use hopr_ssh::{
    ConnectParams, Credential, Hop, Proxy, ProxyKind, Reconnect, ReconnectFuture, SavePassword,
    SessionHooks, TrustedKey,
};
use hopr_store::TrustKey;
use secrecy::{ExposeSecret, SecretString};

use crate::state::AppState;

/// Methods to try for password-style auth: a known password first, then
/// keyboard-interactive (which fills password/OTP prompts it can and asks for the rest),
/// then an explicit password prompt for servers without keyboard-interactive.
pub(super) fn password_credentials(
    password: Option<SecretString>,
    totp_seed: Option<SecretString>,
) -> Vec<Credential> {
    match password {
        Some(pw) => vec![
            Credential::Password(pw.clone()),
            Credential::KeyboardInteractive {
                password: Some(pw),
                totp_seed,
            },
        ],
        None => vec![
            Credential::KeyboardInteractive {
                password: None,
                totp_seed,
            },
            Credential::PromptPassword,
        ],
    }
}

/// Who a session is for, as the audit log names it: `(host id, label)`.
#[derive(Clone)]
pub(super) struct AuditTarget {
    pub(super) host_id: Option<String>,
    pub(super) label: String,
}

impl AuditTarget {
    pub(super) fn record(&self, state: &AppState, kind: &'static str, detail: impl Into<String>) {
        let host = Some((self.host_id.as_deref().unwrap_or(""), self.label.as_str()));
        state.audit(kind, host, detail);
    }
}

pub(super) fn trust_hook(
    state: &Arc<AppState>,
    target: AuditTarget,
) -> Arc<dyn Fn(TrustedKey) + Send + Sync> {
    let app_state = state.clone();
    let store = state.store.clone();
    Arc::new(move |k: TrustedKey| {
        let store = store.clone();
        let (kind, verb) = if k.replace {
            ("host_key.replace", "Accepted a changed key for")
        } else {
            ("host_key.trust", "Trusted the key of")
        };
        target.record(
            &app_state,
            kind,
            format!(
                "{verb} {}:{} ({} {})",
                k.host, k.port, k.key_type, k.fingerprint
            ),
        );
        tauri::async_runtime::spawn(async move {
            let t = TrustKey {
                host_pattern: &k.host,
                port: k.port,
                key_type: &k.key_type,
                key_b64: &k.key_b64,
                fingerprint: &k.fingerprint,
                source: "tofu",
            };
            let res = if k.replace {
                store.replace_host_key(t).await
            } else {
                store.trust_key(t).await
            };
            if let Err(e) = res {
                eprintln!("hopr: failed to save trusted host key: {e}");
            }
        });
    })
}

/// What an SSH tab connects to, minus secrets: enough to rebuild parameters on reconnect.
#[derive(Clone)]
pub(super) enum SshPlan {
    Host {
        host_id: String,
    },
    Quick {
        host: String,
        port: u16,
        username: String,
    },
}

impl SshPlan {
    pub(super) async fn audit_target(&self, state: &AppState) -> AuditTarget {
        match self {
            Self::Host { host_id } => AuditTarget {
                host_id: Some(host_id.clone()),
                label: state
                    .store
                    .get_host(host_id)
                    .await
                    .map(|h| h.label)
                    .unwrap_or_else(|_| host_id.clone()),
            },
            Self::Quick {
                host,
                port,
                username,
            } => AuditTarget {
                host_id: None,
                label: format!("{username}@{host}:{port}"),
            },
        }
    }
}

/// One saved host as a hop: its credentials from the vault, trusted keys, and where a
/// typed password gets remembered. `password` (typed for this one attempt) is tried first.
pub(super) async fn host_hop(
    state: &Arc<AppState>,
    vault: &Arc<hopr_vault::UnlockedVault>,
    h: &Host,
    password: Option<SecretString>,
) -> Result<Hop, AppError> {
    let mut creds = state.store.host_credentials(vault, &h.id).await?;
    if password.is_some() {
        creds.password = password;
    }
    let credentials = match h.auth {
        AuthKind::Password => password_credentials(creds.password, creds.totp_seed),
        AuthKind::Key | AuthKind::Agent => {
            let mut c = if h.auth == AuthKind::Key {
                let key_id = h
                    .key_id
                    .clone()
                    .ok_or_else(|| AppError::invalid(format!("{} has no key selected", h.label)))?;
                let private = state.store.key_private(vault, &key_id).await?;
                let key = hopr_ssh::keys::load(&private)?;
                let mut c = Vec::with_capacity(2);
                // A certificate for the key goes first; the bare key stays as a fallback.
                if let Some(text) = state.store.key_certificate(&key_id).await? {
                    let public = key.public_key().to_openssh().unwrap_or_default();
                    match hopr_ssh::keys::parse_certificate(&text, &public) {
                        Ok((cert, _)) => c.push(Credential::Certificate {
                            key: key.clone(),
                            cert: Box::new(cert),
                        }),
                        Err(e) => eprintln!(
                            "hopr: ignoring the certificate of {}: {}",
                            h.label, e.message
                        ),
                    }
                }
                c.push(Credential::Key(key));
                c
            } else {
                vec![Credential::Agent]
            };
            if creds.totp_seed.is_some() {
                // Key/agent + OTP setups answer the second factor via keyboard-interactive.
                c.push(Credential::KeyboardInteractive {
                    password: creds.password,
                    totp_seed: creds.totp_seed,
                });
            }
            c
        }
    };
    let store = state.store.clone();
    let vault = vault.clone();
    let host_id = h.id.clone();
    let on_save_password: SavePassword = Arc::new(move |pw: SecretString| {
        let (store, vault, id) = (store.clone(), vault.clone(), host_id.clone());
        tauri::async_runtime::spawn(async move {
            if let Err(e) = store
                .set_host_password(&vault, &id, pw.expose_secret())
                .await
            {
                eprintln!("hopr: failed to save password: {e}");
            }
        });
    });
    Ok(Hop {
        host: h.hostname.clone(),
        port: h.port,
        username: h.username.clone(),
        credentials,
        known_hosts: state
            .store
            .known_host_candidates(&h.hostname, h.port)
            .await?,
        on_save_password: Some(on_save_password),
    })
}

/// How to reach a saved host: its jump chain as hops, agent forwarding, and the proxy of
/// the outermost host.
pub(crate) struct HostRoute {
    pub hops: Vec<Hop>,
    pub agent_forwarding: bool,
    pub proxy: Option<Proxy>,
    /// Labels, outermost first (for audit detail).
    pub trail: Vec<String>,
    /// The outermost host's ProxyCommand.
    pub proxy_command: Option<String>,
}

/// Build the route to `host_id`. `target_password` replaces the target's saved password
/// for this attempt (deploy key asks for it when none is saved).
pub(crate) async fn host_route(
    state: &Arc<AppState>,
    host_id: &str,
    target_password: Option<SecretString>,
) -> Result<HostRoute, AppError> {
    let vault = state.vault()?;
    let chain = state.store.jump_chain(host_id).await?;
    if let Some(h) = chain.last()
        && h.protocol != Protocol::Ssh
    {
        return Err(AppError::invalid(format!(
            "{} is a {} host; files, keys and tunnels need SSH",
            h.label,
            h.protocol.as_str()
        )));
    }
    let last = chain.len().saturating_sub(1);
    let mut hops = Vec::with_capacity(chain.len());
    let mut target_password = target_password;
    for (i, h) in chain.iter().enumerate() {
        let pw = if i == last {
            target_password.take()
        } else {
            None
        };
        hops.push(host_hop(state, &vault, h, pw).await?);
    }
    let proxy = match chain.first() {
        Some(first) => match &first.proxy {
            Some(p) => {
                let creds = state.store.host_credentials(&vault, &first.id).await?;
                Some(Proxy {
                    kind: match p.protocol {
                        ProxyProtocol::Socks5 => ProxyKind::Socks5,
                        ProxyProtocol::Http => ProxyKind::Http,
                    },
                    host: p.host.clone(),
                    port: p.port,
                    username: p.username.clone(),
                    password: creds.proxy_password,
                })
            }
            None => None,
        },
        None => None,
    };
    Ok(HostRoute {
        hops,
        agent_forwarding: chain.last().is_some_and(|h| h.agent_forwarding),
        proxy,
        trail: chain.iter().map(|h| h.label.clone()).collect(),
        proxy_command: chain.first().and_then(|h| h.proxy_command.clone()),
    })
}

/// Parameters for a headless connection to a saved host (exec, tunnels, run on group):
/// its route, no PTY prompts, no reconnect.
pub(crate) async fn quiet_params(
    state: &Arc<AppState>,
    host_id: &str,
    hooks: SessionHooks,
) -> Result<ConnectParams, AppError> {
    let route = host_route(state, host_id, None).await?;
    Ok(ConnectParams {
        hops: route.hops,
        agent_forwarding: false,
        proxy: route.proxy,
        proxy_command: route.proxy_command,
        cols: 80,
        rows: 24,
        connect_timeout: Duration::from_secs(20),
        hooks,
        reconnect: None,
    })
}

/// Build connection parameters for a plan. `quick_password` is only used for the first
/// connect; reconnects re-read the vault (saved hosts) or let the server prompt (quick).
pub(super) fn build_params(
    state: Arc<AppState>,
    plan: SshPlan,
    quick_password: Option<SecretString>,
    cols: u32,
    rows: u32,
) -> ReconnectFuture {
    // Boxed with an explicit `Send` type: the reconnect closure below refers back to this
    // function, and the compiler can't infer auto traits through that cycle.
    Box::pin(async move {
        let state = &state;
        let plan = &plan;
        let target = plan.audit_target(state).await;
        let mut hooks = SessionHooks {
            on_trust: Some(trust_hook(state, target.clone())),
            ..SessionHooks::default()
        };
        let (hops, agent_forwarding, proxy, proxy_command) = match plan {
            SshPlan::Host { host_id } => {
                let route = host_route(state, host_id, None).await?;
                let store = state.store.clone();
                let id = host_id.clone();
                let app_state = state.clone();
                let detail = match (&route.proxy, &route.proxy_command) {
                    (_, Some(_)) => format!("{} via ProxyCommand", route.trail.join(" › ")),
                    (Some(p), None) => format!("{} via proxy {p}", route.trail.join(" › ")),
                    (None, None) => route.trail.join(" › "),
                };
                hooks.on_ready = Some(Arc::new(move || {
                    target.record(&app_state, "session.connect", detail.clone());
                    let (store, id) = (store.clone(), id.clone());
                    tauri::async_runtime::spawn(async move {
                        let _ = store.mark_connected(&id).await;
                    });
                }));
                (
                    route.hops,
                    route.agent_forwarding,
                    route.proxy,
                    route.proxy_command,
                )
            }
            SshPlan::Quick {
                host,
                port,
                username,
            } => {
                state.require_unlocked()?;
                let hop = Hop {
                    host: host.clone(),
                    port: *port,
                    username: username.clone(),
                    credentials: password_credentials(quick_password, None),
                    known_hosts: state.store.known_host_candidates(host, *port).await?,
                    on_save_password: None,
                };
                let app_state = state.clone();
                hooks.on_ready = Some(Arc::new(move || {
                    target.record(&app_state, "session.connect", "Quick connect");
                }));
                (vec![hop], false, None, None)
            }
        };

        let reconnect: Reconnect = {
            let state = state.clone();
            let plan = plan.clone();
            Arc::new(move || {
                let (state, plan) = (state.clone(), plan.clone());
                build_params(state, plan, None, cols, rows)
            })
        };
        Ok(ConnectParams {
            hops,
            agent_forwarding,
            proxy,
            proxy_command,
            cols,
            rows,
            connect_timeout: Duration::from_secs(20),
            hooks,
            reconnect: Some(reconnect),
        })
    })
}
