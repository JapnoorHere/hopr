//! The system SSH agent: authenticating with its keys, and forwarding it to a server
//! (`auth-agent@openssh.com` channels are bridged byte-for-byte to the local agent).

use russh::client::{self, Handle};
use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::{AgentClient, AgentStream};
use tracing::debug;

use crate::connect::Client;

#[cfg(windows)]
const OPENSSH_AGENT_PIPE: &str = r"\\.\pipe\openssh-ssh-agent";

async fn try_identities<S: AgentStream + Unpin + Send + 'static>(
    handle: &mut Handle<Client>,
    user: &str,
    mut agent: AgentClient<S>,
) -> bool {
    let Ok(ids) = agent.request_identities().await else {
        return false;
    };
    for id in ids {
        let AgentIdentity::PublicKey { key, .. } = id else {
            continue;
        };
        let hash = if key.algorithm().is_rsa() {
            handle
                .best_supported_rsa_hash()
                .await
                .ok()
                .flatten()
                .flatten()
        } else {
            None
        };
        if let Ok(res) = handle
            .authenticate_publickey_with(user, key, hash, &mut agent)
            .await
            && res.success()
        {
            return true;
        }
    }
    false
}

/// Try every key the system agent offers. Missing agent = method skipped.
pub(crate) async fn authenticate(handle: &mut Handle<Client>, user: &str) -> bool {
    #[cfg(windows)]
    {
        if let Ok(a) = AgentClient::connect_named_pipe(OPENSSH_AGENT_PIPE).await
            && try_identities(handle, user, a).await
        {
            return true;
        }
        if let Ok(a) = AgentClient::connect_pageant().await {
            return try_identities(handle, user, a).await;
        }
        false
    }
    #[cfg(unix)]
    {
        match AgentClient::connect_env().await {
            Ok(a) => try_identities(handle, user, a).await,
            Err(_) => false,
        }
    }
}

/// Pipe a forwarded-agent channel to the local agent until either side closes.
pub(crate) async fn bridge(channel: russh::Channel<client::Msg>) {
    let mut remote = channel.into_stream();
    #[cfg(windows)]
    let local = tokio::net::windows::named_pipe::ClientOptions::new().open(OPENSSH_AGENT_PIPE);
    #[cfg(unix)]
    let local = match std::env::var_os("SSH_AUTH_SOCK") {
        Some(path) => tokio::net::UnixStream::connect(path).await,
        None => Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "SSH_AUTH_SOCK not set",
        )),
    };
    match local {
        Ok(mut local) => {
            if let Err(e) = tokio::io::copy_bidirectional(&mut remote, &mut local).await {
                debug!(error = %e, "agent forwarding channel ended");
            }
        }
        Err(e) => debug!(error = %e, "no local agent to forward"),
    }
}
