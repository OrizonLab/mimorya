//! What creating an event in a person's agenda asks of their company's
//! CalDAV server (ADR-205): whether it sends invitations itself (RFC 6638,
//! `calendar-auto-schedule` in its `DAV` header), the address it knows the
//! person by that invites (`calendar-user-address-set`), and the event put
//! there as a new object (`If-None-Match: *`), never over another. A
//! failure says whether the event is surely not created, or perhaps.

use reqwest_mcp::header::CONTENT_TYPE;
use reqwest_mcp::{Method, StatusCode, Url};

use super::client::{CalDav, refused};
use super::write::written_as_is;
use crate::outbound::{Cause, request_cause};

/// What a server that does not send invitations itself is told.
pub const NO_SCHEDULING: &str = "Le serveur d’agenda n’envoie pas lui-même les invitations \
     (planification CalDAV, RFC 6638)";
/// What a server naming no address of the account is told.
pub const NO_ORGANIZER: &str =
    "Le serveur d’agenda ne donne aucune adresse électronique du compte d’où inviter";
/// What a server refusing the event is told.
pub const EVENT_REFUSED: &str = "Le serveur d’agenda refuse ce rendez-vous";
/// What a server failing while it took the event is told.
pub const EVENT_UNSURE: &str = "Le serveur d’agenda a échoué en traitant le rendez-vous";
/// What the server may send of the person's addresses.
const ADDRESS_LIMIT: usize = 64 * 1024;

const ADDRESSES: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:prop>
<c:calendar-user-address-set/></d:prop></d:propfind>"#;

/// Why an event is not known to be created: surely not, or perhaps.
#[derive(Debug)]
pub enum Unmade {
    Nothing(Cause),
    Unsure(Cause),
}

/// Whether the server sends invitations itself for `agenda`.
pub async fn schedules(dav: &CalDav<'_>, agenda: &Url) -> Result<bool, Cause> {
    let response = dav
        .request(Method::OPTIONS, agenda)
        .send()
        .await
        .map_err(|error| request_cause(&error))?;
    let status = response.status();
    if !status.is_success() {
        return Err(refused(status));
    }
    let announced = response
        .headers()
        .get_all("DAV")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|token| token.trim().eq_ignore_ascii_case("calendar-auto-schedule"));
    Ok(announced)
}

/// The address the person invites from: among those the server knows
/// their principal by, `sender` if it is one, or else the first; each a
/// `mailto:` address written as it is. None if the server names none.
pub async fn organizer(
    dav: &CalDav<'_>,
    principal: &Url,
    sender: Option<&str>,
) -> Result<Option<String>, Cause> {
    let found = dav
        .ask("PROPFIND", principal, "0", ADDRESSES.into(), ADDRESS_LIMIT)
        .await?;
    let addresses: Vec<String> = found
        .into_iter()
        .next()
        .map(|found| found.addresses)
        .unwrap_or_default()
        .iter()
        .filter_map(|href| mailto(href))
        .filter(|address| written_as_is(address))
        .collect();
    let chosen = sender
        .and_then(|sender| {
            addresses
                .iter()
                .find(|address| address.eq_ignore_ascii_case(sender))
        })
        .or(addresses.first());
    Ok(chosen.cloned())
}

/// The address of a `mailto:` URI.
fn mailto(href: &str) -> Option<String> {
    let scheme = href.get(..7)?;
    scheme
        .eq_ignore_ascii_case("mailto:")
        .then(|| href[7..].to_string())
}

/// Puts `text` at `url` as a new calendar object, never over another.
pub async fn put(dav: &CalDav<'_>, url: &Url, text: String) -> Result<(), Unmade> {
    let sent = dav
        .request(Method::PUT, url)
        .header(CONTENT_TYPE, "text/calendar; charset=utf-8")
        .header("If-None-Match", "*")
        .body(text)
        .send()
        .await;
    match sent {
        Ok(response) => answered(response.status()),
        Err(error) => Err(unsent(&error)),
    }
}

/// What a request that got no answer says of the event: nothing reached
/// the server when it was never connected to; otherwise it may have.
fn unsent(error: &reqwest_mcp::Error) -> Unmade {
    match error.is_connect() || error.is_builder() {
        true => Unmade::Nothing(request_cause(error)),
        false => Unmade::Unsure(request_cause(error)),
    }
}

/// What the server's answer to the event says of it.
fn answered(status: StatusCode) -> Result<(), Unmade> {
    let code = status.as_u16();
    let detail = format!("HTTP {code}");
    match code {
        // Whatever success the server says, the event is taken: said
        // otherwise, the model could create it again.
        200..=299 => Ok(()),
        300..=399 | 401 | 403 | 404 | 410 | 429 | 503 => Err(Unmade::Nothing(refused(status))),
        // A gateway or the server failed: the event may have been kept.
        500..=599 => Err(Unmade::Unsure(Cause {
            summary: EVENT_UNSURE,
            detail,
        })),
        _ => Err(Unmade::Nothing(Cause {
            summary: EVENT_REFUSED,
            detail,
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::standard_mail::{CREDENTIALS_REFUSED, TRY_LATER};

    fn said(code: u16) -> Result<&'static str, (bool, &'static str)> {
        answered(StatusCode::from_u16(code).unwrap())
            .map(|()| "créé")
            .map_err(|unmade| match unmade {
                Unmade::Nothing(cause) => (false, cause.summary),
                Unmade::Unsure(cause) => (true, cause.summary),
            })
    }

    #[test]
    fn an_answer_says_whether_the_event_is_created() {
        for created in [200, 201, 202, 204, 207] {
            assert_eq!(said(created), Ok("créé"));
        }
        assert_eq!(said(401), Err((false, CREDENTIALS_REFUSED)));
        assert_eq!(said(503), Err((false, TRY_LATER)));
        assert_eq!(said(412), Err((false, EVENT_REFUSED)));
        assert_eq!(said(507), Err((true, EVENT_UNSURE)));
        assert_eq!(said(502), Err((true, EVENT_UNSURE)));
    }

    #[tokio::test]
    async fn a_request_never_connected_created_nothing_one_unanswered_perhaps() {
        let local = crate::outbound::NetworkPolicy {
            allow_private_network: true,
            ..crate::outbound::NetworkPolicy::default()
        };
        let http = crate::outbound::http_client(local).unwrap();
        // Nothing listens there: never connected.
        let closed = http
            .put("http://127.0.0.1:1/agenda/e.ics")
            .send()
            .await
            .unwrap_err();
        assert!(matches!(unsent(&closed), Unmade::Nothing(_)));
        // Connected, sent, never answered.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let silent = http
            .put(format!("http://{address}/agenda/e.ics"))
            .timeout(std::time::Duration::from_millis(300))
            .body("BEGIN:VCALENDAR")
            .send()
            .await
            .unwrap_err();
        assert!(matches!(unsent(&silent), Unmade::Unsure(_)));
        drop(listener);
    }

    #[test]
    fn only_a_mailto_address_invites() {
        assert_eq!(
            mailto("MAILTO:paul@exemple.fr").as_deref(),
            Some("paul@exemple.fr")
        );
        assert_eq!(mailto("/principals/paul/"), None);
        assert_eq!(mailto("urn:uuid:0d2c"), None);
        assert_eq!(mailto("mailé"), None);
    }
}
