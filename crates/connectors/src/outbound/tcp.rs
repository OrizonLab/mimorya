//! A connection to a server a company names by its host and port, outside
//! HTTP: its mail servers (IMAP, SMTP; ADR-205). The HTTP client's rules
//! hold (ADR-025, ADR-162): a literal private address is refused, a name is
//! resolved to public addresses only and connected to the very addresses
//! checked, the operator's proxy carries it (a `CONNECT` tunnel) unless its
//! bypass names the host, and the tests' simulated servers are reached by
//! their exact address. What it carries is the caller's to secure
//! (`secured`): the platform never sends anything over it in clear.

use std::fmt;
use std::net::{IpAddr, SocketAddr};

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::time::timeout;

use super::failure::{Cause, NOT_PUBLIC, TOO_SLOW, UNREACHABLE};
use super::network::{CONNECT_TIMEOUT, NetworkPolicy, Unresolved, is_public_ip, public_addrs};
use super::proxy::Route;
use super::tunnel;

/// What a connection is, plain or secured, direct or tunnelled.
pub trait Stream: AsyncRead + AsyncWrite + Send + Sync + Unpin + fmt::Debug {}

impl<T: AsyncRead + AsyncWrite + Send + Sync + Unpin + fmt::Debug> Stream for T {}

/// A connection to a server, whatever carries it.
pub type Connection = Box<dyn Stream>;

/// A host's longest name (RFC 1035).
pub const MAX_HOST_CHARS: usize = 253;
/// What a server named in a form no server has is told.
pub const INVALID_HOST: &str = "Serveur mal désigné : nom ou port invalide";

/// Why a company may not name this server (in French), if it may not: a
/// host name or a literal address, public unless the operator allows
/// private ones or it is one of the tests' servers.
pub fn host_problem(host: &str, port: u16, policy: NetworkPolicy) -> Option<String> {
    form_problem(host, port).or_else(|| private_problem(host, port, policy))
}

/// What makes `host:port` no server's name, if anything does.
fn form_problem(host: &str, port: u16) -> Option<String> {
    if host.is_empty() {
        return Some("Nom de serveur attendu".into());
    }
    if host.chars().count() > MAX_HOST_CHARS {
        return Some(format!(
            "Nom de serveur trop long ({MAX_HOST_CHARS} caractères au plus)"
        ));
    }
    if port == 0 {
        return Some("Port entre 1 et 65535 attendu".into());
    }
    match host.parse::<IpAddr>() {
        Ok(_) => None,
        Err(_) if !is_host_name(host) => Some(
            "Nom de serveur invalide : lettres, chiffres et tirets, séparés par des points".into(),
        ),
        Err(_) => None,
    }
}

/// A literal private address the policy does not allow.
fn private_problem(host: &str, port: u16, policy: NetworkPolicy) -> Option<String> {
    match host.parse::<IpAddr>() {
        Ok(ip) if !is_public_ip(ip) && !allowed_privately(ip, port, policy) => {
            Some("Adresse d’un réseau privé : refusée sur ce serveur".into())
        }
        _ => None,
    }
}

/// A private address the operator allows, or one of the tests' servers.
fn allowed_privately(ip: IpAddr, port: u16, policy: NetworkPolicy) -> bool {
    policy.allow_private_network || policy.test_servers.contains(&SocketAddr::new(ip, port))
}

/// A host name: labels of letters, digits and hyphens (RFC 1123), neither
/// starting nor ending with a hyphen, 63 characters at most each.
fn is_host_name(host: &str) -> bool {
    host.split('.').all(|label| {
        (1..=63).contains(&label.len())
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    })
}

/// A connection to `host:port` as the policy allows it: through the
/// operator's proxy unless its bypass names the host, else straight to the
/// addresses checked. Why it cannot be opened, if it cannot.
pub async fn connect(host: &str, port: u16, policy: NetworkPolicy) -> Result<Connection, Cause> {
    if let Some(problem) = form_problem(host, port) {
        return Err(Cause {
            summary: INVALID_HOST,
            detail: problem,
        });
    }
    if let Some(problem) = private_problem(host, port, policy) {
        return Err(Cause {
            summary: NOT_PUBLIC,
            detail: problem,
        });
    }
    match policy.proxy {
        // The proxy resolves the names: only where the operator lifted the
        // public-only rule, as the configuration checks (ADR-162).
        Some(_) if !policy.allow_private_network => Err(Cause {
            summary: UNREACHABLE,
            detail: "mandataire sans réseau privé permis".into(),
        }),
        Some(proxy) => match proxy.route(host, port) {
            Ok(Route::Proxy(intercept)) => tunnel::through(&intercept, host, port, policy).await,
            Ok(Route::Direct) => direct(host, port, policy).await,
            Err(detail) => Err(Cause {
                summary: INVALID_HOST,
                detail,
            }),
        },
        None => direct(host, port, policy).await,
    }
}

/// Straight to the server: a literal address as given (checked above), a
/// name to its public addresses only unless private ones are allowed.
async fn direct(host: &str, port: u16, policy: NetworkPolicy) -> Result<Connection, Cause> {
    let addrs: Vec<SocketAddr> = match host.parse::<IpAddr>() {
        Ok(ip) => vec![SocketAddr::new(ip, port)],
        Err(_) if policy.allow_private_network => tokio::net::lookup_host((host, port))
            .await
            .map_err(|error| Cause {
                summary: UNREACHABLE,
                detail: format!("{host} : {error}"),
            })?
            .collect(),
        Err(_) => public_addrs(host)
            .await
            .map_err(|unresolved| Cause {
                summary: match unresolved {
                    Unresolved::PrivateOnly(_) => NOT_PUBLIC,
                    Unresolved::Failed(_) => UNREACHABLE,
                },
                detail: unresolved.to_string(),
            })?
            .into_iter()
            .map(|addr| SocketAddr::new(addr.ip(), port))
            .collect(),
    };
    let stream = timeout(CONNECT_TIMEOUT, TcpStream::connect(&addrs[..]))
        .await
        .map_err(|_| Cause {
            summary: TOO_SLOW,
            detail: format!("connexion à {host}:{port}"),
        })?
        .map_err(|error| Cause {
            summary: UNREACHABLE,
            detail: error.to_string(),
        })?;
    // Commands and their answers are short lines.
    let _ = stream.set_nodelay(true);
    Ok(Box::new(stream))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAAS: NetworkPolicy = NetworkPolicy {
        allow_private_network: false,
        allow_stdio: false,
        proxy: None,
        test_servers: &[],
        test_roots: &[],
    };

    #[test]
    fn names_and_public_addresses_are_accepted() {
        for host in [
            "imap.example.com",
            "mail-1.fournisseur.fr",
            "SMTP.EXAMPLE.COM",
            "93.184.216.34",
            "2606:2800:220:1:248:1893:25c8:1946",
        ] {
            assert_eq!(host_problem(host, 993, SAAS), None, "{host}");
        }
    }

    #[test]
    fn what_is_no_server_name_is_refused() {
        let long = format!("{}.example", "a".repeat(MAX_HOST_CHARS));
        for host in [
            "",
            "imap.example.com:993",
            "imap .example.com",
            "-imap.example.com",
            "imap-.example.com",
            "imap..example.com",
            "imap.example.com.",
            "imap_example.com",
            "imaps://imap.example.com",
            "[2606:2800:220:1:248:1893:25c8:1946]",
            "compte@imap.example.com",
            "imap.example.com/inbox",
            &long,
        ] {
            assert!(host_problem(host, 993, SAAS).is_some(), "{host:?}");
        }
        assert!(host_problem("imap.example.com", 0, SAAS).is_some());
        let label = format!("{}.example", "a".repeat(64));
        assert!(host_problem(&label, 993, SAAS).is_some());
    }

    #[test]
    fn a_private_address_is_refused_unless_allowed_or_tested() {
        for host in [
            "127.0.0.1",
            "10.0.0.8",
            "192.168.1.20",
            "::1",
            "fd00::1",
            "::ffff:127.0.0.1",
        ] {
            let problem = host_problem(host, 993, SAAS).unwrap_or_default();
            assert!(problem.contains("réseau privé"), "{host}: {problem}");
        }
        let private = NetworkPolicy {
            allow_private_network: true,
            ..SAAS
        };
        assert_eq!(host_problem("10.0.0.8", 993, private), None);
        static SERVERS: [SocketAddr; 1] = [SocketAddr::new(
            IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            1993,
        )];
        let tests = NetworkPolicy {
            test_servers: &SERVERS,
            ..SAAS
        };
        assert_eq!(host_problem("127.0.0.1", 1993, tests), None);
        // Another port of the same address, or the same address written
        // as an IPv4-mapped IPv6 one, is no test server.
        assert!(host_problem("127.0.0.1", 993, tests).is_some());
        assert!(host_problem("::ffff:127.0.0.1", 1993, tests).is_some());
    }
}
