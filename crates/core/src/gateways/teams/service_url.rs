//! Where a company's Teams bot writes back (ADR-138): the service an
//! activity names (`serviceUrl`), which its token, signed by the Bot
//! Framework, names too. The platform still writes only to Microsoft's
//! Teams services of the commercial cloud (GCC included), whose Bot
//! Framework issues the tokens it takes, in HTTPS, on their default port:
//! an activity naming any other address, however signed, is not answered
//! there. Tests name their local Teams instead.

/// The hosts of Microsoft's Teams services, and the domains their hosts
/// end with, in the commercial cloud (GCC included).
const MICROSOFT_HOSTS: [&str; 1] = ["smba.trafficmanager.net"];
const MICROSOFT_DOMAINS: [&str; 2] = [".botframework.com", ".teams.microsoft.com"];
/// The longest service address written to.
const MAX_URL_CHARS: usize = 200;

/// The services a bot may write back to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceUrls {
    /// Microsoft's Teams services.
    Microsoft,
    /// Only those under this address (tests: a local Teams).
    Under(String),
}

impl ServiceUrls {
    /// Whether the bot may write to the service at `url`.
    pub fn trust(&self, url: &str) -> bool {
        if url.chars().count() > MAX_URL_CHARS {
            return false;
        }
        match self {
            Self::Microsoft => {
                let Some(rest) = url.strip_prefix("https://") else {
                    return false;
                };
                let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
                let host = host.to_ascii_lowercase();
                let microsoft = MICROSOFT_HOSTS.contains(&host.as_str())
                    || MICROSOFT_DOMAINS.iter().any(|domain| {
                        host.ends_with(domain)
                            && host.len() > domain.len()
                            && host[..host.len() - domain.len()]
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
                    });
                microsoft && plain_path(path)
            }
            Self::Under(base) => url
                .strip_prefix(base.as_str())
                .is_some_and(|path| base.ends_with('/') && plain_path(path)),
        }
    }
}

/// A path of letters, digits and `/`, `-`, `_`, `.`: nothing that changes
/// where it leads (no `..`, no query, no fragment, no user, no port).
fn plain_path(path: &str) -> bool {
    !path.contains("..")
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_microsoft_s_teams_services_are_written_to() {
        let microsoft = ServiceUrls::Microsoft;
        for url in [
            "https://smba.trafficmanager.net/emea/",
            "https://smba.trafficmanager.net/amer/",
            "https://SMBA.trafficmanager.net/emea/",
            "https://smba.infra.gcc.teams.microsoft.com/gcc/",
        ] {
            assert!(microsoft.trust(url), "{url}");
        }
        for url in [
            "http://smba.trafficmanager.net/emea/",
            "https://smba.trafficmanager.net:8443/emea/",
            "https://user@smba.trafficmanager.net/emea/",
            "https://smba.trafficmanager.net.evil.example/",
            "https://evil.example/smba.trafficmanager.net/",
            "https://teams.microsoft.com.evil.example/",
            "https://evilteams.microsoft.com/",
            "https://.teams.microsoft.com/",
            "https://smba.trafficmanager.net/emea/../../autre",
            "https://smba.trafficmanager.net/emea/?x=1",
            "https://smba.trafficmanager.net/emea/#x",
            "https://127.0.0.1/",
            // Another cloud, whose Bot Framework issues other tokens.
            "https://smba.infra.gov.teams.microsoft.us/",
            "",
        ] {
            assert!(!microsoft.trust(url), "{url}");
        }
        let long = format!("https://smba.trafficmanager.net/{}/", "a".repeat(200));
        assert!(!microsoft.trust(&long));
    }

    #[test]
    fn a_local_teams_is_trusted_under_its_address_only() {
        let local = ServiceUrls::Under("http://127.0.0.1:4100/teams/".into());
        assert!(local.trust("http://127.0.0.1:4100/teams/"));
        assert!(local.trust("http://127.0.0.1:4100/teams/emea/"));
        assert!(!local.trust("http://127.0.0.1:4100/autre/"));
        assert!(!local.trust("http://127.0.0.1:4100/teams/../autre/"));
        assert!(!local.trust("https://smba.trafficmanager.net/emea/"));
        let unrooted = ServiceUrls::Under("http://127.0.0.1:4100/teams".into());
        assert!(!unrooted.trust("http://127.0.0.1:4100/teams.evil/"));
    }
}
