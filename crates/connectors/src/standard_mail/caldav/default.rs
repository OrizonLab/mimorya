//! The agenda a person's CalDAV server takes new events in by default
//! (ADR-205, RFC 6638 9.2): the one their scheduling Inbox names
//! (`schedule-default-calendar-URL`), the Inbox named by their principal
//! (`schedule-inbox-URL`). Neither is asked on another server; a server
//! naming neither, or failing to, leaves the choice to the order its
//! agendas are listed in. Only an agenda listed as taking events is
//! chosen: the default is a preference, never a way around the rights.

use reqwest_mcp::Url;

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
/// server; none if the server names none there, or fails to say.
pub async fn default_agenda(dav: &CalDav<'_>, principal: &Url) -> Option<Url> {
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

/// Whether two addresses name the same collection, its closing `/` or not.
fn same_collection(one: &Url, other: &Url) -> bool {
    let path = |url: &Url| url.path().trim_end_matches('/').to_string();
    one.scheme() == other.scheme()
        && one.host_str() == other.host_str()
        && one.port_or_known_default() == other.port_or_known_default()
        && path(one) == path(other)
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
}
