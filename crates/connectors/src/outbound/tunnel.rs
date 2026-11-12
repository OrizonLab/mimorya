//! A connection through the operator's proxy (ADR-162, ADR-205): a
//! `CONNECT host:port` tunnel, over TLS to the proxy itself when its address
//! is `https://` (a certificate chaining to the same roots as any server's),
//! its credentials given if its address holds any. The proxy resolves the
//! name and decides what is reached; its credentials and its answers are
//! never written, only its status code.

use std::net::IpAddr;
use std::time::Duration;

use hyper_util::client::proxy::matcher::Intercept;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use super::failure::{Cause, TOO_SLOW, UNREACHABLE};
use super::network::{CONNECT_TIMEOUT, NetworkPolicy};
use super::tcp::Connection;
use super::tls::secured;

/// What a tunnel the proxy refused is told.
pub const PROXY_REFUSED: &str = "Le mandataire de l’exploitant refuse cette connexion";
/// How long the proxy may take to open the tunnel.
const TUNNEL_TIMEOUT: Duration = Duration::from_secs(10);
/// The longest answer head read from the proxy.
const MAX_HEAD_BYTES: usize = 8 * 1024;

/// A tunnel to `host:port` through the proxy `intercept` names.
pub(super) async fn through(
    intercept: &Intercept,
    host: &str,
    port: u16,
    policy: NetworkPolicy,
) -> Result<Connection, Cause> {
    let proxy = intercept.uri();
    let unreachable = |detail: String| Cause {
        summary: UNREACHABLE,
        detail,
    };
    let proxy_host = proxy
        .host()
        .map(|host| host.trim_start_matches('[').trim_end_matches(']'))
        .ok_or_else(|| unreachable("mandataire sans hôte".into()))?;
    let encrypted = proxy.scheme_str() == Some("https");
    let proxy_port = proxy.port_u16().unwrap_or(if encrypted { 443 } else { 80 });
    let tcp = timeout(
        CONNECT_TIMEOUT,
        TcpStream::connect((proxy_host, proxy_port)),
    )
    .await
    .map_err(|_| Cause {
        summary: TOO_SLOW,
        detail: "connexion au mandataire".into(),
    })?
    .map_err(|error| unreachable(format!("mandataire : {error}")))?;
    let mut stream: Connection = Box::new(tcp);
    if encrypted {
        // Said as the proxy's, not the mail server's.
        stream = secured(stream, proxy_host, policy)
            .await
            .map_err(|cause| Cause {
                detail: format!("mandataire : {}", cause.detail),
                ..cause
            })?;
    }
    let target = authority(host, port);
    let mut request = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n");
    if let Some(credentials) = intercept.basic_auth().and_then(|v| v.to_str().ok()) {
        request.push_str(&format!("Proxy-Authorization: {credentials}\r\n"));
    }
    request.push_str("\r\n");
    let status = timeout(TUNNEL_TIMEOUT, open(&mut stream, request.as_bytes()))
        .await
        .map_err(|_| Cause {
            summary: TOO_SLOW,
            detail: "ouverture du tunnel par le mandataire".into(),
        })?
        .map_err(|error| unreachable(format!("mandataire : {error}")))?;
    match status {
        200 => Ok(stream),
        407 => Err(Cause {
            summary: PROXY_REFUSED,
            detail: "il demande ses identifiants (407)".into(),
        }),
        code => Err(Cause {
            summary: PROXY_REFUSED,
            detail: format!("réponse {code}"),
        }),
    }
}

/// `host:port` as a request line writes it, an IPv6 address in brackets.
fn authority(host: &str, port: u16) -> String {
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V6(_)) => format!("[{host}]:{port}"),
        _ => format!("{host}:{port}"),
    }
}

/// Sends the request and reads the answer's head, byte after byte so that
/// nothing the server says next is taken; its status code.
async fn open(stream: &mut Connection, request: &[u8]) -> std::io::Result<u16> {
    stream.write_all(request).await?;
    stream.flush().await?;
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() >= MAX_HEAD_BYTES || stream.read(&mut byte).await? == 0 {
            return Err(std::io::Error::other("réponse incomplète"));
        }
        head.push(byte[0]);
    }
    let line = String::from_utf8_lossy(&head);
    let mut words = line.split_whitespace();
    match (
        words.next(),
        words.next().and_then(|code| code.parse().ok()),
    ) {
        (Some(version), Some(code)) if version.starts_with("HTTP/1.") => Ok(code),
        _ => Err(std::io::Error::other("réponse illisible")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ipv6_address_is_bracketed() {
        assert_eq!(authority("imap.example.com", 993), "imap.example.com:993");
        assert_eq!(authority("93.184.216.34", 465), "93.184.216.34:465");
        assert_eq!(authority("2001:db8::1", 993), "[2001:db8::1]:993");
    }
}
