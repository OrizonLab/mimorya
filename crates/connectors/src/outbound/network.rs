//! How the server reaches an MCP server (ADR-025) or a REST API (ADR-112)
//! a company configured.
//! An address chosen by a company must never lead into the operator's own
//! network: names are resolved to public addresses only (checked at
//! connection time, so a name cannot be re-pointed after the check), literal
//! private addresses are refused, and redirects are not followed. Private
//! deployments can lift this for their internal servers.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use reqwest_mcp::dns::{Addrs, Name, Resolve, Resolving};
use reqwest_mcp::{Client, Url, redirect};

/// What the operator allows (environment), never the companies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NetworkPolicy {
    /// Private, loopback and link-local addresses, and plain `http://`
    /// (`MIMORYA_MCP_ALLOW_PRIVATE_NETWORK`, private deployments).
    pub allow_private_network: bool,
    /// Child processes (`MIMORYA_MCP_ALLOW_STDIO`): only the operator writes
    /// such a configuration.
    pub allow_stdio: bool,
}

pub const MAX_URL_CHARS: usize = 2000;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Whether an address belongs to the public Internet.
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_public_v4(v4),
            None => is_public_v6(v6),
        },
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || a == 0
        // Shared address space (carrier-grade NAT), benchmarking, reserved.
        || (a == 100 && (64..128).contains(&b))
        || (a == 198 && (b == 18 || b == 19))
        || a >= 240
        || (a == 192 && b == 0 && ip.octets()[2] == 0))
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    let first = ip.segments()[0];
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        // Unique local fc00::/7, link-local fe80::/10, deprecated site-local
        // fec0::/10, documentation 2001:db8::/32.
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
        || (first & 0xffc0) == 0xfec0
        // Deprecated IPv4-compatible ::/96.
        || ip.segments()[..6].iter().all(|s| *s == 0)
        || (first == 0x2001 && ip.segments()[1] == 0x0db8)
        // NAT64 64:ff9b::/96 and 6to4 2002::/16 embed an IPv4 address.
        || (first == 0x0064 && ip.segments()[1] == 0xff9b)
        || first == 0x2002)
}

/// Why a company may not use this address (in French), if it may not.
pub fn url_problem(url: &str, policy: NetworkPolicy) -> Option<String> {
    if url.chars().count() > MAX_URL_CHARS {
        return Some(format!(
            "Adresse trop longue ({MAX_URL_CHARS} caractères au plus)"
        ));
    }
    let Ok(parsed) = Url::parse(url) else {
        return Some("Adresse invalide : https://… attendu".into());
    };
    let secure = parsed.scheme() == "https";
    if !secure && !(parsed.scheme() == "http" && policy.allow_private_network) {
        return Some("Adresse en https:// attendue".into());
    }
    // A credential in the address would be stored, shown and audited in clear.
    if !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Some(
            "L’adresse ne contient ni identifiants, ni paramètres, ni fragment : utilisez le jeton"
                .into(),
        );
    }
    let literal = match parsed.host() {
        None => return Some("Adresse sans nom de serveur".into()),
        Some(url::Host::Domain(_)) => None,
        Some(url::Host::Ipv4(ip)) => Some(IpAddr::V4(ip)),
        Some(url::Host::Ipv6(ip)) => Some(IpAddr::V6(ip)),
    };
    if literal.is_some_and(|ip| !is_public_ip(ip)) && !policy.allow_private_network {
        return Some("Adresse d’un réseau privé : refusée sur ce serveur".into());
    }
    None
}

/// The public addresses of a name; an error when it has none.
async fn public_addrs(host: &str) -> Result<Vec<SocketAddr>, String> {
    let found: Vec<SocketAddr> = tokio::net::lookup_host((host, 0))
        .await
        .map_err(|error| format!("{host} : {error}"))?
        .filter(|addr| is_public_ip(addr.ip()))
        .collect();
    if found.is_empty() {
        return Err(format!("{host} : aucune adresse publique"));
    }
    Ok(found)
}

/// Resolves names to public addresses only.
struct PublicOnly;

impl Resolve for PublicOnly {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addrs: Addrs = Box::new(public_addrs(&host).await?.into_iter());
            Ok(addrs)
        })
    }
}

/// The HTTP client of MCP connections: explicit TLS (ring, webpki roots), no
/// redirect, no proxy from the environment, public addresses unless allowed.
pub fn http_client(policy: NetworkPolicy) -> anyhow::Result<Client> {
    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let mut builder = Client::builder()
        .tls_backend_preconfigured(tls)
        .redirect(redirect::Policy::none())
        .connect_timeout(CONNECT_TIMEOUT)
        .pool_max_idle_per_host(0)
        .no_proxy();
    if !policy.allow_private_network {
        builder = builder.dns_resolver(PublicOnly);
    }
    Ok(builder.build()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAAS: NetworkPolicy = NetworkPolicy {
        allow_private_network: false,
        allow_stdio: false,
    };

    #[test]
    fn tells_public_addresses_apart() {
        for public in [
            "1.1.1.1",
            "8.8.8.8",
            "2606:4700:4700::1111",
            "::ffff:1.1.1.1",
        ] {
            assert!(is_public_ip(public.parse().unwrap()), "{public}");
        }
        for private in [
            "10.0.0.1",
            "172.16.5.4",
            "192.168.1.1",
            "127.0.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "255.255.255.255",
            "198.18.0.1",
            "::1",
            "::",
            "fd00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "64:ff9b::a00:1",
            "2002:a00:1::1",
            "::127.0.0.1",
            "::808:808",
            "fec0::1",
        ] {
            assert!(!is_public_ip(private.parse().unwrap()), "{private}");
        }
    }

    #[test]
    fn refuses_addresses_a_company_may_not_use() {
        assert_eq!(url_problem("https://mcp.example.com/mcp", SAAS), None);
        assert_eq!(url_problem("https://203.0.113.0.example.org/x", SAAS), None);
        for refused in [
            "http://mcp.example.com/mcp",
            "https://user:pass@mcp.example.com/mcp",
            "https://mcp.example.com/mcp?api_key=secret",
            "https://mcp.example.com/mcp#token",
            "https://127.0.0.1/mcp",
            "https://[::1]/mcp",
            "https://169.254.169.254/latest",
            "https://10.1.2.3:8443/mcp",
            "ftp://mcp.example.com",
            "pas une adresse",
        ] {
            assert!(url_problem(refused, SAAS).is_some(), "{refused}");
        }
        let private = NetworkPolicy {
            allow_private_network: true,
            allow_stdio: false,
        };
        assert_eq!(url_problem("http://127.0.0.1:8080/mcp", private), None);
        assert!(url_problem("ftp://127.0.0.1", private).is_some());
    }

    #[tokio::test]
    async fn never_resolves_a_name_to_a_private_address() {
        assert!(public_addrs("localhost").await.is_err());
        assert!(http_client(SAAS).is_ok());
    }
}
