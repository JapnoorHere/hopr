//! Port forwarding over a quiet connection (P1–P3): local (`-L`), dynamic SOCKS5 (`-D`)
//! and remote (`-R`), with live byte and connection counters for the tunnels dashboard.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tracing::debug;

use crate::exec::Connection;

/// Live counters for one tunnel.
#[derive(Debug, Default)]
pub struct ForwardStats {
    /// Bytes sent towards the destination.
    pub up: AtomicU64,
    /// Bytes received from the destination.
    pub down: AtomicU64,
    /// Connections open now.
    pub active: AtomicU32,
    /// Connections since the tunnel started.
    pub total: AtomicU64,
}

/// Where a remote forward's connections go: bound remote port → local destination.
#[derive(Clone, Default)]
pub struct RemoteForwards(Arc<Mutex<HashMap<u32, Route>>>);

#[derive(Clone)]
struct Route {
    host: String,
    port: u16,
    stats: Arc<ForwardStats>,
}

impl RemoteForwards {
    /// Send connections arriving on remote `bound_port` to `host:port`.
    pub fn insert(&self, bound_port: u32, host: String, port: u16, stats: Arc<ForwardStats>) {
        if let Ok(mut m) = self.0.lock() {
            m.insert(bound_port, Route { host, port, stats });
        }
    }

    /// Stop routing a remote port.
    pub fn remove(&self, bound_port: u32) {
        if let Ok(mut m) = self.0.lock() {
            m.remove(&bound_port);
        }
    }

    pub(crate) fn has(&self, bound_port: u32) -> bool {
        self.get(bound_port).is_some()
    }

    fn get(&self, bound_port: u32) -> Option<Route> {
        self.0.lock().ok()?.get(&bound_port).cloned()
    }

    /// A connection the server forwarded to us: pipe it to its local destination.
    pub(crate) fn dispatch<S>(&self, bound_port: u32, channel: S) -> bool
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let Some(route) = self.get(bound_port) else {
            return false;
        };
        tokio::spawn(async move {
            let stats = route.stats.clone();
            stats.total.fetch_add(1, Ordering::Relaxed);
            stats.active.fetch_add(1, Ordering::Relaxed);
            match TcpStream::connect((route.host.as_str(), route.port)).await {
                Ok(local) => {
                    let _ = local.set_nodelay(true);
                    pump(channel, local, &stats).await;
                }
                Err(e) => debug!(
                    "remote forward to {}:{} failed: {e}",
                    route.host, route.port
                ),
            }
            stats.active.fetch_sub(1, Ordering::Relaxed);
        });
        true
    }
}

/// Copy both ways until both sides finish. Bytes from `a` to `b` count as `up`.
pub async fn pump<A, B>(a: A, b: B, stats: &ForwardStats)
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    let (mut ar, mut aw) = tokio::io::split(a);
    let (mut br, mut bw) = tokio::io::split(b);
    let a_to_b = async {
        let mut buf = vec![0u8; 32 * 1024];
        loop {
            match ar.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if bw.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                    stats.up.fetch_add(n as u64, Ordering::Relaxed);
                }
            }
        }
        let _ = bw.shutdown().await;
    };
    let b_to_a = async {
        let mut buf = vec![0u8; 32 * 1024];
        loop {
            match br.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if aw.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                    stats.down.fetch_add(n as u64, Ordering::Relaxed);
                }
            }
        }
        let _ = aw.shutdown().await;
    };
    tokio::join!(a_to_b, b_to_a);
}

fn origin(sock: &TcpStream) -> (String, u32) {
    sock.peer_addr()
        .map(|a| (a.ip().to_string(), u32::from(a.port())))
        .unwrap_or_else(|_| ("127.0.0.1".into(), 0))
}

/// Accept on `listener` until `stop` turns true; each connection opens a channel to the
/// fixed destination (`-L`) or the one its SOCKS5 request names (`-D`).
async fn serve(
    conn: Arc<Connection>,
    listener: TcpListener,
    stats: Arc<ForwardStats>,
    mut stop: watch::Receiver<bool>,
    fixed: Option<(String, u16)>,
) {
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let Ok((sock, _)) = accepted else { continue };
                let _ = sock.set_nodelay(true);
                let (conn, stats, fixed) = (conn.clone(), stats.clone(), fixed.clone());
                tokio::spawn(async move {
                    stats.total.fetch_add(1, Ordering::Relaxed);
                    stats.active.fetch_add(1, Ordering::Relaxed);
                    handle_one(&conn, sock, &stats, fixed).await;
                    stats.active.fetch_sub(1, Ordering::Relaxed);
                });
            }
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    break;
                }
            }
        }
    }
}

async fn handle_one(
    conn: &Connection,
    mut sock: TcpStream,
    stats: &ForwardStats,
    fixed: Option<(String, u16)>,
) {
    let (oaddr, oport) = origin(&sock);
    let socks = fixed.is_none();
    let target = match fixed {
        Some(t) => t,
        None => match socks_handshake(&mut sock).await {
            Some(t) => t,
            None => return,
        },
    };
    match conn.direct_tcpip(&target.0, target.1, &oaddr, oport).await {
        Ok(channel) => {
            if socks
                && sock
                    .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
                    .await
                    .is_err()
            {
                return;
            }
            pump(sock, channel, stats).await;
        }
        Err(e) => {
            debug!(
                "forward to {}:{} refused: {}",
                target.0, target.1, e.message
            );
            if socks {
                // "Host unreachable".
                let _ = sock.write_all(&[5, 4, 0, 1, 0, 0, 0, 0, 0, 0]).await;
            }
        }
    }
}

/// Server side of a SOCKS5 CONNECT without authentication (RFC 1928). Returns the target.
async fn socks_handshake(sock: &mut TcpStream) -> Option<(String, u16)> {
    let mut head = [0u8; 2];
    sock.read_exact(&mut head).await.ok()?;
    if head[0] != 5 {
        return None;
    }
    let mut methods = vec![0u8; usize::from(head[1])];
    sock.read_exact(&mut methods).await.ok()?;
    if !methods.contains(&0) {
        let _ = sock.write_all(&[5, 0xFF]).await; // no acceptable method
        return None;
    }
    sock.write_all(&[5, 0]).await.ok()?;
    let mut req = [0u8; 4];
    sock.read_exact(&mut req).await.ok()?;
    if req[0] != 5 || req[1] != 1 {
        let _ = sock.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0]).await; // command not supported
        return None;
    }
    let host = match req[3] {
        1 => {
            let mut ip = [0u8; 4];
            sock.read_exact(&mut ip).await.ok()?;
            IpAddr::from(ip).to_string()
        }
        3 => {
            let mut len = [0u8; 1];
            sock.read_exact(&mut len).await.ok()?;
            let mut name = vec![0u8; usize::from(len[0])];
            sock.read_exact(&mut name).await.ok()?;
            String::from_utf8(name).ok()?
        }
        4 => {
            let mut ip = [0u8; 16];
            sock.read_exact(&mut ip).await.ok()?;
            IpAddr::from(ip).to_string()
        }
        _ => return None,
    };
    let mut port = [0u8; 2];
    sock.read_exact(&mut port).await.ok()?;
    Some((host, u16::from_be_bytes(port)))
}

/// `-L`: connections to `listener` go to `dest_host:dest_port` as seen from the server.
pub async fn serve_local(
    conn: Arc<Connection>,
    listener: TcpListener,
    dest_host: String,
    dest_port: u16,
    stats: Arc<ForwardStats>,
    stop: watch::Receiver<bool>,
) {
    serve(conn, listener, stats, stop, Some((dest_host, dest_port))).await;
}

/// `-D`: `listener` is a SOCKS5 proxy whose connections leave from the server.
pub async fn serve_socks(
    conn: Arc<Connection>,
    listener: TcpListener,
    stats: Arc<ForwardStats>,
    stop: watch::Receiver<bool>,
) {
    serve(conn, listener, stats, stop, None).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn socks_handshake_reads_domain_and_ipv4_targets() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let client = tokio::spawn(async move {
            let mut c = TcpStream::connect(addr).await.unwrap();
            c.write_all(&[5, 1, 0]).await.unwrap();
            let mut ok = [0u8; 2];
            c.read_exact(&mut ok).await.unwrap();
            assert_eq!(ok, [5, 0]);
            let mut req = vec![5, 1, 0, 3, 7];
            req.extend_from_slice(b"db.corp");
            req.extend_from_slice(&5432u16.to_be_bytes());
            c.write_all(&req).await.unwrap();
        });
        let (mut s, _) = listener.accept().await.unwrap();
        assert_eq!(
            socks_handshake(&mut s).await,
            Some(("db.corp".into(), 5432))
        );
        client.await.unwrap();

        let client = tokio::spawn(async move {
            let mut c = TcpStream::connect(addr).await.unwrap();
            c.write_all(&[5, 1, 0, 5, 1, 0, 1, 10, 0, 0, 7, 0, 80])
                .await
                .unwrap();
            let mut ok = [0u8; 2];
            c.read_exact(&mut ok).await.unwrap();
        });
        let (mut s, _) = listener.accept().await.unwrap();
        assert_eq!(socks_handshake(&mut s).await, Some(("10.0.0.7".into(), 80)));
        client.await.unwrap();
    }

    #[tokio::test]
    async fn socks_refuses_auth_only_clients() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let client = tokio::spawn(async move {
            let mut c = TcpStream::connect(addr).await.unwrap();
            c.write_all(&[5, 1, 2]).await.unwrap(); // username/password only
            let mut reply = [0u8; 2];
            c.read_exact(&mut reply).await.unwrap();
            reply
        });
        let (mut s, _) = listener.accept().await.unwrap();
        assert_eq!(socks_handshake(&mut s).await, None);
        assert_eq!(client.await.unwrap(), [5, 0xFF]);
    }

    #[tokio::test]
    async fn pump_counts_both_directions() {
        let (a, mut a_peer) = tokio::io::duplex(1024);
        let (b, mut b_peer) = tokio::io::duplex(1024);
        let stats = Arc::new(ForwardStats::default());
        let s2 = stats.clone();
        let task = tokio::spawn(async move { pump(a, b, &s2).await });
        a_peer.write_all(b"hello").await.unwrap();
        let mut got = [0u8; 5];
        b_peer.read_exact(&mut got).await.unwrap();
        b_peer.write_all(b"pong!!").await.unwrap();
        let mut back = [0u8; 6];
        a_peer.read_exact(&mut back).await.unwrap();
        drop(a_peer);
        drop(b_peer);
        task.await.unwrap();
        assert_eq!(stats.up.load(Ordering::Relaxed), 5);
        assert_eq!(stats.down.load(Ordering::Relaxed), 6);
    }
}
