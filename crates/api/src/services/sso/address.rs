//! Which addresses the server may call for a sign-in (ADR-025, ADR-063):
//! every one a provider names (its issuer, but also the keys, token and
//! authorization endpoints its declaration lists) is checked before any
//! connection. Names resolve to public addresses only (the client's
//! resolver); an address written as an IP is never resolved, so it is
//! checked here. The end-to-end tests' simulated servers are reached as the
//! connectors reach them, by their exact address and port (ADR-175).

use std::net::IpAddr;

use mimorya_connectors::outbound::{NetworkPolicy, is_public_ip};
use url::{Host, Url};

/// Why the server may not call this address (in French), if it may not.
pub fn address_problem(address: &str, policy: NetworkPolicy) -> Option<String> {
    let Ok(url) = Url::parse(address) else {
        return Some("adresse invalide".into());
    };
    let private = policy.allow_private_network || policy.serves_tests(&url);
    if !(url.scheme() == "https" || (private && url.scheme() == "http")) {
        return Some(format!("{} : https:// attendu", url.scheme()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Some("adresse avec identifiants".into());
    }
    let literal = match url.host() {
        None => return Some("adresse sans nom de serveur".into()),
        Some(Host::Domain(_)) => None,
        Some(Host::Ipv4(ip)) => Some(IpAddr::V4(ip)),
        Some(Host::Ipv6(ip)) => Some(IpAddr::V6(ip)),
    };
    match literal {
        Some(ip) if !private && !is_public_ip(ip) => Some(format!(
            "{ip} : adresse d’un réseau privé, refusée sur ce serveur"
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAAS: NetworkPolicy = NetworkPolicy {
        allow_private_network: false,
        allow_stdio: false,
        proxy: None,
        test_servers: &[],
    };
    const PRIVATE: NetworkPolicy = NetworkPolicy {
        allow_private_network: true,
        allow_stdio: false,
        proxy: None,
        test_servers: &[],
    };
    const TESTED: [std::net::SocketAddr; 1] = [std::net::SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        4110,
    )];

    #[test]
    fn the_end_to_end_tests_servers_alone_by_their_address_and_port() {
        let tests = NetworkPolicy {
            test_servers: &TESTED,
            ..SAAS
        };
        let server = "http://127.0.0.1:4110/.well-known/openid-configuration";
        assert_eq!(address_problem(server, tests), None);
        for refused in [
            "http://127.0.0.1:4111/jwks",
            "http://127.0.0.2:4110/jwks",
            "http://10.0.0.12:4110/",
        ] {
            assert!(address_problem(refused, tests).is_some(), "{refused}");
        }
        assert!(address_problem(server, SAAS).is_some());
    }

    #[test]
    fn only_public_https_addresses_unless_the_operator_allows_more() {
        for allowed in [
            "https://login.microsoftonline.com/common/discovery/v2.0/keys",
            // Some key sets carry a query (Azure AD B2C).
            "https://b2c.example.com/keys?p=b2c_1_signin",
            "https://8.8.8.8/token",
        ] {
            assert_eq!(address_problem(allowed, SAAS), None, "{allowed}");
        }
        for refused in [
            "http://idp.example.com/token",
            "https://127.0.0.1/keys",
            "https://10.0.0.12:5432/",
            "https://169.254.169.254/latest/meta-data",
            "https://[::1]/token",
            "https://user:pass@idp.example.com/token",
            "file:///etc/passwd",
            "pas une adresse",
        ] {
            assert!(address_problem(refused, SAAS).is_some(), "{refused}");
        }
        assert_eq!(
            address_problem("http://127.0.0.1:8080/token", PRIVATE),
            None
        );
        assert!(address_problem("ftp://127.0.0.1/", PRIVATE).is_some());
    }
}
