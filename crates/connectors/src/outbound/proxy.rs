//! The HTTP proxy the operator may route the platform's outbound requests
//! through (ADR-162): those reaching what companies connect (MCP servers,
//! REST APIs, linked accounts, conversation gateways, identity providers).
//! Through a proxy, the proxy resolves the names and decides what is
//! reached: the platform cannot hold its public-only rule, and a proxy is
//! accepted only where the operator lifted it. Its credentials, if its
//! address holds any, are never written: only its scheme, host and port are.

use std::fmt;

use reqwest_mcp::{NoProxy, Proxy};
use url::Url;

/// What a proxy's address may weigh.
const MAX_PROXY_CHARS: usize = 2000;

/// The operator's proxy, and the hosts reached without it.
pub struct OutboundProxy {
    url: Url,
    bypass: Option<String>,
}

impl OutboundProxy {
    /// The proxy at `address` (`http://` or `https://`, optionally with
    /// credentials), the hosts of `bypass` (as `NO_PROXY` writes them:
    /// names, domains, addresses, ranges, comma-separated) reached without
    /// it; why it cannot be used otherwise, in French. An `https://` proxy
    /// carries a public certificate: the client trusts no other root, and
    /// tunnels without letting the proxy read what it carries.
    pub fn new(address: &str, bypass: Option<&str>) -> Result<Self, String> {
        if address.chars().count() > MAX_PROXY_CHARS {
            return Err(format!("{MAX_PROXY_CHARS} caractères au plus"));
        }
        let url = Url::parse(address.trim())
            .map_err(|_| "adresse invalide : http(s)://hôte:port attendu".to_string())?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err("adresse http(s)://hôte:port attendue".into());
        }
        if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
            return Err("ni chemin, ni paramètres, ni fragment : http(s)://hôte:port".into());
        }
        let bypass = bypass.map(str::trim).filter(|hosts| !hosts.is_empty());
        if let Some(problem) = bypass.and_then(Self::bypass_problem) {
            return Err(problem.into());
        }
        Ok(Self {
            url,
            bypass: bypass.map(str::to_string),
        })
    }

    /// Why the hosts reached without the proxy cannot be read, if they cannot.
    pub fn bypass_problem(hosts: &str) -> Option<&'static str> {
        hosts
            .chars()
            .any(char::is_control)
            .then_some("liste d’hôtes sans caractère de contrôle attendue")
    }

    /// Kept for the life of the process, as the configuration it is read
    /// from: a reference keeps the network policy a plain value.
    pub fn leak(self) -> &'static Self {
        Box::leak(Box::new(self))
    }

    /// The proxy as the HTTP client takes it.
    pub(super) fn reqwest(&self) -> anyhow::Result<Proxy> {
        let bypass = self.bypass.as_deref().and_then(NoProxy::from_string);
        Ok(Proxy::all(self.url.as_str())?.no_proxy(bypass))
    }

    /// Its scheme, host and port: what may be written of it.
    pub fn shown(&self) -> String {
        let host = self.url.host_str().unwrap_or_default();
        match self.url.port_or_known_default() {
            Some(port) => format!("{}://{host}:{port}", self.url.scheme()),
            None => format!("{}://{host}", self.url.scheme()),
        }
    }

    /// The hosts reached without the proxy, if any.
    pub fn bypass(&self) -> Option<&str> {
        self.bypass.as_deref()
    }
}

impl fmt::Debug for OutboundProxy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OutboundProxy")
            .field("url", &self.shown())
            .field("bypass", &self.bypass)
            .finish()
    }
}

impl PartialEq for OutboundProxy {
    fn eq(&self, other: &Self) -> bool {
        self.url == other.url && self.bypass == other.bypass
    }
}

impl Eq for OutboundProxy {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proxy_is_an_http_s_host_and_port() {
        let proxy = OutboundProxy::new("http://proxy.example:3128", None).unwrap();
        assert_eq!(proxy.shown(), "http://proxy.example:3128");
        let proxy = OutboundProxy::new(" https://proxy.example ", Some(" ")).unwrap();
        assert_eq!(proxy.shown(), "https://proxy.example:443");
        assert_eq!(proxy.bypass(), None);
        for refused in [
            "proxy.example:3128",
            "socks5://proxy.example:1080",
            "http://",
            "http://proxy.example:3128/chemin",
            "http://proxy.example:3128/?a=b",
            "http://proxy.example:3128/#f",
        ] {
            assert!(OutboundProxy::new(refused, None).is_err(), "{refused}");
        }
        let long = format!("http://{}.example", "a".repeat(MAX_PROXY_CHARS));
        assert!(OutboundProxy::new(&long, None).is_err());
        let control = OutboundProxy::new("http://proxy.example", Some("intra\n.example"));
        assert!(control.is_err());
    }

    #[test]
    fn its_credentials_are_never_shown() {
        let address = "http://compte:secret-du-mandataire@proxy.example:3128";
        let proxy = OutboundProxy::new(address, Some("intra.example, 10.0.0.0/8")).unwrap();
        assert_eq!(proxy.shown(), "http://proxy.example:3128");
        let debug = format!("{proxy:?}");
        assert!(
            !debug.contains("secret") && !debug.contains("compte"),
            "{debug}"
        );
        assert_eq!(proxy.bypass(), Some("intra.example, 10.0.0.0/8"));
        assert!(proxy.reqwest().is_ok());
    }
}
