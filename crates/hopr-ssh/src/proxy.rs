//! Reaching the first hop through a SOCKS5 (RFC 1928/1929) or HTTP CONNECT proxy (C7).
//! The target name is resolved by the proxy, so hosts only it can see still work.

use std::fmt;
use std::io;
use std::net::IpAddr;

use data_encoding::BASE64;
use secrecy::{ExposeSecret, SecretString};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use zeroize::Zeroizing;

/// Proxy protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyKind {
    /// SOCKS5, optionally with username/password.
    Socks5,
    /// HTTP `CONNECT`, optionally with Basic auth.
    Http,
}

/// Where and how to reach the proxy. Not `Debug`: it may carry a password.
#[derive(Clone)]
pub struct Proxy {
    /// Protocol.
    pub kind: ProxyKind,
    /// Proxy host.
    pub host: String,
    /// Proxy port.
    pub port: u16,
    /// Username, if the proxy requires one.
    pub username: Option<String>,
    /// Password for `username`.
    pub password: Option<SecretString>,
}

impl fmt::Display for Proxy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let scheme = match self.kind {
            ProxyKind::Socks5 => "socks5",
            ProxyKind::Http => "http",
        };
        write!(f, "{scheme}://{}:{}", self.host, self.port)
    }
}

fn refused(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::ConnectionRefused, msg.into())
}

fn denied(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, msg.into())
}

/// Open a TCP stream to `host:port` through `proxy`.
pub(crate) async fn dial(proxy: &Proxy, host: &str, port: u16) -> io::Result<TcpStream> {
    let mut stream = TcpStream::connect((proxy.host.as_str(), proxy.port)).await?;
    let _ = stream.set_nodelay(true);
    handshake(&mut stream, proxy, host, port).await?;
    Ok(stream)
}

/// Ask the proxy for a tunnel to `host:port` over an open connection to it.
pub(crate) async fn handshake<S: AsyncRead + AsyncWrite + Unpin>(
    s: &mut S,
    proxy: &Proxy,
    host: &str,
    port: u16,
) -> io::Result<()> {
    match proxy.kind {
        ProxyKind::Socks5 => socks5(s, proxy, host, port).await,
        ProxyKind::Http => http_connect(s, proxy, host, port).await,
    }
}

fn socks5_reply(code: u8) -> &'static str {
    match code {
        1 => "general failure",
        2 => "connection not allowed by its rules",
        3 => "network unreachable",
        4 => "host unreachable",
        5 => "connection refused",
        6 => "TTL expired",
        7 => "command not supported",
        8 => "address type not supported",
        _ => "unknown error",
    }
}

async fn socks5<S: AsyncRead + AsyncWrite + Unpin>(
    s: &mut S,
    proxy: &Proxy,
    host: &str,
    port: u16,
) -> io::Result<()> {
    let with_auth = proxy.username.is_some();
    if with_auth {
        s.write_all(&[5, 2, 0x00, 0x02]).await?;
    } else {
        s.write_all(&[5, 1, 0x00]).await?;
    }
    let mut choice = [0u8; 2];
    s.read_exact(&mut choice).await?;
    if choice[0] != 5 {
        return Err(refused("not a SOCKS5 proxy"));
    }
    match choice[1] {
        0x00 => {}
        0x02 if with_auth => {
            let user = proxy.username.as_deref().unwrap_or_default().as_bytes();
            let pass = proxy
                .password
                .as_ref()
                .map(|p| p.expose_secret().as_bytes())
                .unwrap_or_default();
            let (Ok(ulen), Ok(plen)) = (u8::try_from(user.len()), u8::try_from(pass.len())) else {
                return Err(denied("the proxy username or password is too long"));
            };
            let mut msg = Zeroizing::new(Vec::with_capacity(3 + user.len() + pass.len()));
            msg.push(1);
            msg.push(ulen);
            msg.extend_from_slice(user);
            msg.push(plen);
            msg.extend_from_slice(pass);
            s.write_all(&msg).await?;
            let mut status = [0u8; 2];
            s.read_exact(&mut status).await?;
            if status[1] != 0 {
                return Err(denied("the proxy rejected the username or password"));
            }
        }
        0x02 | 0xFF => return Err(denied("the proxy requires a username and password")),
        _ => return Err(refused("the proxy offered no usable login method")),
    }

    let mut req = vec![5, 1, 0];
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            req.push(1);
            req.extend_from_slice(&ip.octets());
        }
        Ok(IpAddr::V6(ip)) => {
            req.push(4);
            req.extend_from_slice(&ip.octets());
        }
        Err(_) => {
            let len = u8::try_from(host.len()).map_err(|_| refused("the hostname is too long"))?;
            req.push(3);
            req.push(len);
            req.extend_from_slice(host.as_bytes());
        }
    }
    req.extend_from_slice(&port.to_be_bytes());
    s.write_all(&req).await?;

    let mut head = [0u8; 4];
    s.read_exact(&mut head).await?;
    if head[1] != 0 {
        return Err(refused(format!(
            "the proxy said: {}",
            socks5_reply(head[1])
        )));
    }
    // Skip the bound address the proxy reports.
    let addr_len = match head[3] {
        1 => 4,
        4 => 16,
        3 => usize::from(s.read_u8().await?),
        _ => return Err(refused("the proxy sent an invalid reply")),
    };
    let mut rest = vec![0u8; addr_len + 2];
    s.read_exact(&mut rest).await?;
    Ok(())
}

async fn http_connect<S: AsyncRead + AsyncWrite + Unpin>(
    s: &mut S,
    proxy: &Proxy,
    host: &str,
    port: u16,
) -> io::Result<()> {
    if host.contains(['\r', '\n', ' ']) {
        return Err(refused("invalid hostname"));
    }
    let authority = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let mut req = Zeroizing::new(format!(
        "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n"
    ));
    if let Some(user) = &proxy.username {
        let pass = proxy
            .password
            .as_ref()
            .map(|p| p.expose_secret().to_owned())
            .unwrap_or_default();
        let creds = Zeroizing::new(format!("{user}:{pass}"));
        req.push_str(&format!(
            "Proxy-Authorization: Basic {}\r\n",
            BASE64.encode(creds.as_bytes())
        ));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes()).await?;

    // Read byte by byte up to the blank line so nothing of the SSH banner is consumed.
    let mut head = Vec::with_capacity(256);
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() > 8192 {
            return Err(refused("the proxy sent an oversized reply"));
        }
        head.push(s.read_u8().await?);
    }
    let text = String::from_utf8_lossy(&head);
    let status_line = text.lines().next().unwrap_or_default();
    let code = status_line.split_whitespace().nth(1).unwrap_or_default();
    match code {
        "200" => Ok(()),
        "407" => Err(denied("the proxy requires a valid username and password")),
        _ => Err(refused(format!("the proxy said: {}", status_line.trim()))),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn proxy(kind: ProxyKind, auth: bool) -> Proxy {
        Proxy {
            kind,
            host: "proxy".into(),
            port: 1080,
            username: auth.then(|| "u".into()),
            password: auth.then(|| SecretString::from("p".to_owned())),
        }
    }

    #[tokio::test]
    async fn socks5_with_auth_requests_the_domain_name() {
        let (mut client, mut server) = tokio::io::duplex(1024);
        let srv = tokio::spawn(async move {
            let mut greet = [0u8; 4];
            server.read_exact(&mut greet).await.unwrap();
            assert_eq!(greet, [5, 2, 0, 2]);
            server.write_all(&[5, 2]).await.unwrap();
            let mut auth = [0u8; 5];
            server.read_exact(&mut auth).await.unwrap();
            assert_eq!(auth, [1, 1, b'u', 1, b'p']);
            server.write_all(&[1, 0]).await.unwrap();
            let mut req = vec![0u8; 5 + 9 + 2];
            server.read_exact(&mut req).await.unwrap();
            assert_eq!(&req[..5], &[5, 1, 0, 3, 9]);
            assert_eq!(&req[5..14], b"db.inside");
            assert_eq!(&req[14..], &22u16.to_be_bytes());
            server
                .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
                .await
                .unwrap();
            server.write_all(b"SSH-2.0-x").await.unwrap();
            server
        });
        handshake(
            &mut client,
            &proxy(ProxyKind::Socks5, true),
            "db.inside",
            22,
        )
        .await
        .unwrap();
        let mut banner = [0u8; 9];
        client.read_exact(&mut banner).await.unwrap();
        assert_eq!(
            &banner, b"SSH-2.0-x",
            "the tunnel starts right after the reply"
        );
        srv.await.unwrap();
    }

    #[tokio::test]
    async fn socks5_reports_refusals() {
        let (mut client, mut server) = tokio::io::duplex(1024);
        tokio::spawn(async move {
            let mut greet = [0u8; 3];
            server.read_exact(&mut greet).await.unwrap();
            server.write_all(&[5, 0]).await.unwrap();
            let mut req = [0u8; 10];
            server.read_exact(&mut req).await.unwrap();
            assert_eq!(req[3], 1, "IPv4 targets use the address form");
            server
                .write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0])
                .await
                .unwrap();
        });
        let err = handshake(
            &mut client,
            &proxy(ProxyKind::Socks5, false),
            "10.0.0.9",
            22,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("connection refused"), "{err}");
    }

    #[tokio::test]
    async fn socks5_wrong_password_is_denied() {
        let (mut client, mut server) = tokio::io::duplex(1024);
        tokio::spawn(async move {
            let mut greet = [0u8; 4];
            server.read_exact(&mut greet).await.unwrap();
            server.write_all(&[5, 2]).await.unwrap();
            let mut auth = [0u8; 5];
            server.read_exact(&mut auth).await.unwrap();
            server.write_all(&[1, 1]).await.unwrap();
        });
        let err = handshake(&mut client, &proxy(ProxyKind::Socks5, true), "h", 22)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    }

    #[tokio::test]
    async fn http_connect_sends_basic_auth_and_stops_at_the_blank_line() {
        let (mut client, mut server) = tokio::io::duplex(4096);
        let srv = tokio::spawn(async move {
            let mut got = Vec::new();
            while !got.ends_with(b"\r\n\r\n") {
                got.push(server.read_u8().await.unwrap());
            }
            let text = String::from_utf8(got).unwrap();
            assert!(
                text.starts_with("CONNECT db.inside:2222 HTTP/1.1\r\n"),
                "{text}"
            );
            assert!(
                text.contains("Proxy-Authorization: Basic dTpw\r\n"),
                "{text}"
            );
            server
                .write_all(b"HTTP/1.1 200 Connection established\r\nX: y\r\n\r\nSSH-2.0-x")
                .await
                .unwrap();
            server
        });
        handshake(
            &mut client,
            &proxy(ProxyKind::Http, true),
            "db.inside",
            2222,
        )
        .await
        .unwrap();
        let mut banner = [0u8; 9];
        client.read_exact(&mut banner).await.unwrap();
        assert_eq!(&banner, b"SSH-2.0-x");
        srv.await.unwrap();
    }

    #[tokio::test]
    async fn http_connect_maps_407() {
        let (mut client, mut server) = tokio::io::duplex(4096);
        tokio::spawn(async move {
            let mut got = Vec::new();
            while !got.ends_with(b"\r\n\r\n") {
                got.push(server.read_u8().await.unwrap());
            }
            server
                .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n")
                .await
                .unwrap();
        });
        let err = handshake(&mut client, &proxy(ProxyKind::Http, false), "h", 22)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    }
}
