//! A person's agendas at their company's CalDAV server (ADR-205): the
//! declared address itself when it is a calendar; otherwise their calendar
//! home, named by the address or by their principal (RFC 5397, RFC 4791,
//! 6.2.1), and the calendars of events it holds, ten at most. A principal
//! or a home on another server stops the search: nothing is sent there. A
//! calendar on another server is left out.

use reqwest_mcp::Url;

use super::client::{CalDav, ELSEWHERE};
use crate::outbound::Cause;

/// The most agendas read.
pub const MAX_AGENDAS: usize = 10;
/// What the server may send while agendas are looked for, per request.
const DISCOVERY_LIMIT: usize = 256 * 1024;
/// What a person with no agenda of events is told.
pub const NO_AGENDA: &str = "Aucun agenda de rendez-vous n’est trouvé pour ce compte";

const ITSELF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:prop>
<d:current-user-principal/><d:resourcetype/><d:displayname/>
<c:calendar-home-set/><c:supported-calendar-component-set/>
</d:prop></d:propfind>"#;
const HOME: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:prop>
<c:calendar-home-set/></d:prop></d:propfind>"#;
const LISTING: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:prop>
<d:resourcetype/><d:displayname/><c:supported-calendar-component-set/>
</d:prop></d:propfind>"#;

/// An agenda: where it is, its name.
#[derive(Debug, Clone)]
pub struct Agenda {
    pub url: Url,
    pub name: Option<String>,
}

/// The person's agendas of events.
pub async fn agendas(dav: &CalDav<'_>) -> Result<Vec<Agenda>, Cause> {
    let base = dav.base().clone();
    let itself = first(
        dav.ask("PROPFIND", &base, "0", ITSELF.into(), DISCOVERY_LIMIT)
            .await?,
    );
    if itself.holds_events() {
        return Ok(vec![Agenda {
            url: base,
            name: itself.name,
        }]);
    }
    let home = match (itself.home, itself.principal) {
        (Some(home), _) => inside(dav, &home)?,
        (None, Some(principal)) => {
            let principal = inside(dav, &principal)?;
            let found = dav
                .ask("PROPFIND", &principal, "0", HOME.into(), DISCOVERY_LIMIT)
                .await?;
            match found.into_iter().find_map(|found| found.home) {
                Some(home) => inside(dav, &home)?,
                None => base,
            }
        }
        (None, None) => base,
    };
    let listed = dav
        .ask("PROPFIND", &home, "1", LISTING.into(), DISCOVERY_LIMIT)
        .await?;
    let agendas: Vec<Agenda> = listed
        .into_iter()
        .filter(|found| found.holds_events())
        .filter_map(|found| {
            Some(Agenda {
                url: dav.within(&found.href)?,
                name: found.name,
            })
        })
        .take(MAX_AGENDAS)
        .collect();
    if agendas.is_empty() {
        return Err(Cause {
            summary: NO_AGENDA,
            detail: String::new(),
        });
    }
    Ok(agendas)
}

/// What the server said of the address asked: its first response.
fn first(found: Vec<super::Found>) -> super::Found {
    found.into_iter().next().unwrap_or_default()
}

/// `href` on the declared server, or the search stops.
fn inside(dav: &CalDav<'_>, href: &str) -> Result<Url, Cause> {
    dav.within(href).ok_or(Cause {
        summary: ELSEWHERE,
        detail: String::new(),
    })
}
