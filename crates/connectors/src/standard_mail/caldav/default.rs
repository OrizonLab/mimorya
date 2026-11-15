//! The agenda a person's CalDAV server takes new events in by default
//! (ADR-205, RFC 6638 9.2): the one their scheduling Inbox names
//! (`schedule-default-calendar-URL`), the Inbox named by their principal
//! (`schedule-inbox-URL`). Neither is asked on another server; a server
//! naming neither, failing to, or not answering within its patience,
//! leaves the choice to the order its agendas are listed in. Only an
//! agenda listed as taking events is chosen: the default is a preference,
//! never a way around the rights.

use std::time::Duration;

use reqwest_mcp::Url;
use tokio::time::timeout;

use super::agendas::Agenda;
use super::client::CalDav;

/// What the server may send of an Inbox or of its default agenda.
const DEFAULT_LIMIT: usize = 64 * 1024;

const INBOX: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:prop>
<c:schedule-inbox-URL/></d:prop></d:propfind>"#;
const DEFAULT: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:prop>
<c:schedule-default-calendar-URL/></d:prop></d:propfind>"#;

/// The agenda `principal`'s Inbox names for new events, on the declared
/// server; none if the server names none there, fails to say, or does not
/// within `patience`, the rest of the call's time left to create the event.
pub async fn default_agenda(dav: &CalDav<'_>, principal: &Url, patience: Duration) -> Option<Url> {
    timeout(patience, named_default(dav, principal))
        .await
        .ok()
        .flatten()
}

async fn named_default(dav: &CalDav<'_>, principal: &Url) -> Option<Url> {
    let named = dav
        .ask("PROPFIND", principal, "0", INBOX.into(), DEFAULT_LIMIT)
        .await
        .ok()?;
    let inbox = named.into_iter().find_map(|found| found.inbox)?;
    let inbox = dav.within(&inbox)?;
    let named = dav
        .ask("PROPFIND", &inbox, "0", DEFAULT.into(), DEFAULT_LIMIT)
        .await
        .ok()?;
    let agenda = named.into_iter().find_map(|found| found.default_agenda)?;
    dav.within(&agenda)
}

/// Where a new event goes: `default` if it is one of `agendas` taking
/// events, or else the first of them that takes events.
pub fn chosen(agendas: Vec<Agenda>, default: Option<&Url>) -> Option<Agenda> {
    let mut taking = agendas.into_iter().filter(|agenda| agenda.takes_events);
    let first = taking.next()?;
    let Some(default) = default else {
        return Some(first);
    };
    if same_collection(&first.url, default) {
        return Some(first);
    }
    Some(
        taking
            .find(|agenda| same_collection(&agenda.url, default))
            .unwrap_or(first),
    )
}

/// Whether two addresses name the same collection, its closing `/` or
/// not, an unreserved character written as itself or percent-encoded.
fn same_collection(one: &Url, other: &Url) -> bool {
    let path = |url: &Url| normalized(url.path().trim_end_matches('/'));
    one.scheme() == other.scheme()
        && one.host_str() == other.host_str()
        && one.port_or_known_default() == other.port_or_known_default()
        && path(one) == path(other)
}

/// `path` with each percent-encoded unreserved character written as
/// itself, every other encoding in capitals (RFC 3986, 6.2.2).
fn normalized(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = String::with_capacity(path.len());
    let mut at = 0;
    while at < bytes.len() {
        // Two hexadecimal digits, nothing else (`from_str_radix` would take
        // a sign).
        let encoded = (bytes[at] == b'%')
            .then(|| path.get(at + 1..at + 3))
            .flatten()
            .filter(|hex| hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match encoded {
            Some(byte) if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) => {
                out.push(char::from(byte));
                at += 3;
            }
            Some(byte) => {
                out.push_str(&format!("%{byte:02X}"));
                at += 3;
            }
            None => {
                let rest = &path[at..];
                let next = rest.chars().next().map_or(1, char::len_utf8);
                out.push_str(&rest[..next]);
                at += next;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agenda(url: &str, takes_events: bool) -> Agenda {
        Agenda {
            url: Url::parse(url).unwrap(),
            name: None,
            takes_events,
        }
    }

    fn at(url: &str) -> Url {
        Url::parse(url).unwrap()
    }

    #[test]
    fn the_default_agenda_is_chosen_only_where_it_takes_events() {
        let listed = || {
            vec![
                agenda("https://dav.exemple.fr/cal/anniversaires/", false),
                agenda("https://dav.exemple.fr/cal/personnel/", true),
                agenda("https://dav.exemple.fr/cal/travail/", true),
            ]
        };
        let url = |chosen: Option<Agenda>| chosen.map(|agenda| agenda.url.to_string());
        let first = Some("https://dav.exemple.fr/cal/personnel/".to_string());
        assert_eq!(url(chosen(listed(), None)), first);
        // Named without its closing slash: the same collection.
        let work = at("https://dav.exemple.fr/cal/travail");
        assert_eq!(
            url(chosen(listed(), Some(&work))),
            Some("https://dav.exemple.fr/cal/travail/".to_string())
        );
        // Read-only, unlisted or on another port: the first that takes it.
        for default in [
            "https://dav.exemple.fr/cal/anniversaires/",
            "https://dav.exemple.fr/cal/ailleurs/",
            "https://dav.exemple.fr:8443/cal/travail/",
        ] {
            assert_eq!(
                url(chosen(listed(), Some(&at(default)))),
                first,
                "{default}"
            );
        }
        let read_only = vec![agenda("https://dav.exemple.fr/cal/anniversaires/", false)];
        assert!(chosen(read_only, None).is_none());
    }

    #[test]
    fn an_unreserved_character_percent_encoded_names_the_same_collection() {
        let listed = vec![
            agenda("https://dav.exemple.fr/cal/personnel/", true),
            agenda("https://dav.exemple.fr/cal/travail/", true),
            agenda("https://dav.exemple.fr/cal/a%2Fb/", true),
        ];
        let url = |default: &str| {
            chosen(listed.clone(), Some(&at(default))).map(|agenda| agenda.url.to_string())
        };
        assert_eq!(
            url("https://dav.exemple.fr/cal/trav%61il/"),
            Some("https://dav.exemple.fr/cal/travail/".into())
        );
        // A reserved character stays encoded, its hexadecimal in any case.
        assert_eq!(
            url("https://dav.exemple.fr/cal/a%2fb/"),
            Some("https://dav.exemple.fr/cal/a%2Fb/".into())
        );
        assert_eq!(
            url("https://dav.exemple.fr/cal/a/b/"),
            Some("https://dav.exemple.fr/cal/personnel/".into())
        );
        assert_eq!(normalized("/cal/%7e%E9t%C3%A9"), "/cal/~%E9t%C3%A9");
        // What is not two hexadecimal digits is not an encoding.
        assert_eq!(normalized("/cal/%+5/%-a/%4/%"), "/cal/%+5/%-a/%4/%");
        let signed = vec![
            agenda("https://dav.exemple.fr/cal/personnel/", true),
            agenda("https://dav.exemple.fr/cal/%05/", true),
        ];
        let default = at("https://dav.exemple.fr/cal/%+5/");
        assert_eq!(
            chosen(signed, Some(&default)).map(|agenda| agenda.url.to_string()),
            Some("https://dav.exemple.fr/cal/personnel/".into())
        );
    }
}
