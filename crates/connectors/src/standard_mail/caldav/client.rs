//! Requests to a company's CalDAV server as a person (ADR-205): over HTTPS
//! alone, whatever the network policy allows, by the HTTP client of what
//! companies connect (no redirect followed, public addresses unless the
//! operator allows others, the operator's proxy); the person's account
//! given by Basic authentication; never an address outside the server the
//! company declared, whatever its answers name; what it answers bounded,
//! read as a multi-status.

use reqwest_mcp::header::CONTENT_TYPE;
use reqwest_mcp::{Client, Method, Response, StatusCode, Url};

use super::xml::{Found, multistatus};
use crate::outbound::{Cause, NetworkPolicy, TOO_LONG, UNREACHABLE, http_client, request_cause};
use crate::standard_mail::{CREDENTIALS_REFUSED, MailLogin, TRY_LATER};

/// What a server that does not answer as an agenda is told.
pub const NOT_AN_AGENDA: &str = "Le serveur ne répond pas comme un agenda CalDAV";
/// What an agenda naming another server is told.
pub const ELSEWHERE: &str =
    "L’agenda CalDAV renvoie vers un autre serveur : rien ne lui est envoyé";
/// What an agenda redirecting is told.
pub const REDIRECTED: &str =
    "L’agenda CalDAV redirige ailleurs : l’administrateur déclare l’adresse finale";
/// What an address with no agenda is told.
pub const NO_AGENDA_THERE: &str = "Aucun agenda CalDAV à cette adresse";
/// What an agenda refusing the account access is told.
pub const ACCESS_REFUSED: &str = "Le serveur CalDAV refuse l’accès à cet agenda";
/// What an agenda address not in HTTPS is told: no password goes in clear.
pub const NOT_HTTPS: &str =
    "Agenda CalDAV : adresse en https:// attendue, aucun identifiant n’est envoyé en clair";

/// A company's CalDAV server, reached as a person.
pub struct CalDav<'a> {
    http: Client,
    base: Url,
    login: &'a MailLogin,
}

impl<'a> CalDav<'a> {
    /// The server at `address`, the company's declaration, for `login`.
    pub fn new(address: &str, login: &'a MailLogin, policy: NetworkPolicy) -> Result<Self, Cause> {
        let base = Url::parse(address)
            .ok()
            .filter(|url| url.scheme() == "https")
            .ok_or(Cause {
                summary: NOT_HTTPS,
                detail: String::new(),
            })?;
        let http = http_client(policy).map_err(|error| Cause {
            summary: UNREACHABLE,
            detail: error.to_string(),
        })?;
        Ok(Self { http, base, login })
    }

    /// The address the company declared.
    pub fn base(&self) -> &Url {
        &self.base
    }

    /// `href`, as an answer names it, as an address of the declared server;
    /// none when it leads to another (another scheme, host or port).
    pub fn within(&self, href: &str) -> Option<Url> {
        let url = self.base.join(href.trim()).ok()?;
        let same = url.scheme() == self.base.scheme()
            && url.host_str() == self.base.host_str()
            && url.port_or_known_default() == self.base.port_or_known_default()
            && url.username().is_empty()
            && url.password().is_none();
        same.then_some(url)
    }

    /// The responses to `method` (`PROPFIND` or `REPORT`) of `body` at
    /// `url`, `depth` deep, the server's answer read up to `limit` bytes.
    pub async fn ask(
        &self,
        method: &'static str,
        url: &Url,
        depth: &'static str,
        body: String,
        limit: usize,
    ) -> Result<Vec<Found>, Cause> {
        let method = Method::from_bytes(method.as_bytes()).map_err(|_| unreadable())?;
        let response = self
            .http
            .request(method, url.clone())
            .basic_auth(self.login.username(), Some(self.login.password()))
            .header("Depth", depth)
            .header(CONTENT_TYPE, "application/xml; charset=utf-8")
            .body(body)
            .send()
            .await
            .map_err(|error| request_cause(&error))?;
        let status = response.status();
        if status != StatusCode::MULTI_STATUS {
            return Err(refused(status));
        }
        let bytes = bounded(response, limit).await?;
        let text = String::from_utf8(bytes).map_err(|_| unreadable())?;
        multistatus(&text).ok_or_else(unreadable)
    }
}

/// What an answer other than a multi-status says.
fn refused(status: StatusCode) -> Cause {
    let summary = match status.as_u16() {
        401 => CREDENTIALS_REFUSED,
        403 => ACCESS_REFUSED,
        404 | 410 => NO_AGENDA_THERE,
        300..=399 => REDIRECTED,
        429 | 503 => TRY_LATER,
        _ => NOT_AN_AGENDA,
    };
    Cause {
        summary,
        detail: format!("HTTP {}", status.as_u16()),
    }
}

fn unreadable() -> Cause {
    Cause {
        summary: NOT_AN_AGENDA,
        detail: "réponse illisible".into(),
    }
}

/// The body of `response`, `limit` bytes at most.
async fn bounded(mut response: Response, limit: usize) -> Result<Vec<u8>, Cause> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| request_cause(&error))?
    {
        if body.len() + chunk.len() > limit {
            return Err(Cause {
                summary: TOO_LONG,
                detail: format!("plus de {} Kio", limit / 1024),
            });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dav(login: &MailLogin) -> CalDav<'_> {
        CalDav::new(
            "https://dav.exemple.fr/dav/",
            login,
            NetworkPolicy::default(),
        )
        .unwrap()
    }

    #[test]
    fn nothing_goes_outside_the_declared_server() {
        let login = MailLogin::new("paul", "secret").unwrap();
        let dav = dav(&login);
        let inside = dav.within("/dav/calendars/paul/").unwrap();
        assert_eq!(
            inside.as_str(),
            "https://dav.exemple.fr/dav/calendars/paul/"
        );
        assert!(dav.within("calendars/").is_some());
        for elsewhere in [
            "https://autre.exemple.fr/dav/",
            "http://dav.exemple.fr/dav/",
            "https://dav.exemple.fr:8443/dav/",
            "//autre.exemple.fr/dav/",
            "https://paul:secret@dav.exemple.fr/dav/",
        ] {
            assert!(dav.within(elsewhere).is_none(), "{elsewhere}");
        }
    }

    #[test]
    fn an_agenda_in_clear_is_refused() {
        let login = MailLogin::new("paul", "secret").unwrap();
        let policy = NetworkPolicy {
            allow_private_network: true,
            ..NetworkPolicy::default()
        };
        let refused = CalDav::new("http://10.0.0.5/dav/", &login, policy).err();
        assert_eq!(refused.map(|cause| cause.summary), Some(NOT_HTTPS));
        assert_eq!(refused_status(401), CREDENTIALS_REFUSED);
        assert_eq!(refused_status(301), REDIRECTED);
        assert_eq!(refused_status(503), TRY_LATER);
    }

    fn refused_status(code: u16) -> &'static str {
        refused(StatusCode::from_u16(code).unwrap()).summary
    }
}
