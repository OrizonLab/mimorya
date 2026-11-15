//! What `xml` reads of CalDAV answers, and what it refuses.

use super::*;

const HOME: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:response>
<d:href>/dav/calendars/paul/</d:href>
<d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop>
  <d:status>HTTP/1.1 200 OK</d:status></d:propstat>
  </d:response>
  <d:response>
<d:href>/dav/calendars/paul/personnel/</d:href>
<d:propstat><d:prop>
    <d:resourcetype><d:collection/><c:calendar/></d:resourcetype>
    <d:displayname>Agenda &amp; rendez-vous</d:displayname>
    <c:supported-calendar-component-set><c:comp name="VEVENT"/></c:supported-calendar-component-set>
  </d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>
<d:propstat><d:prop><c:calendar-home-set/></d:prop>
  <d:status>HTTP/1.1 404 Not Found</d:status></d:propstat>
  </d:response>
  <d:response>
<d:href>/dav/calendars/paul/taches/</d:href>
<d:propstat><d:prop>
    <d:resourcetype><d:collection/><c:calendar/></d:resourcetype>
    <c:supported-calendar-component-set><c:comp name="VTODO"/></c:supported-calendar-component-set>
  </d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>
  </d:response>
</d:multistatus>"#;

#[test]
fn reads_each_response_and_its_properties_found() {
    let read = multistatus(HOME).unwrap();
    assert_eq!(read.len(), 3);
    assert!(!read[0].calendar);
    assert_eq!(read[1].href, "/dav/calendars/paul/personnel/");
    assert!(read[1].holds_events());
    assert_eq!(read[1].name.as_deref(), Some("Agenda & rendez-vous"));
    assert_eq!(read[1].home, None);
    assert!(read[2].calendar && !read[2].holds_events());
}

#[test]
fn reads_a_principal_a_home_and_calendar_data() {
    let principal = r#"<multistatus xmlns="DAV:"><response><href>/dav/</href><propstat>
        <prop><current-user-principal><href>/dav/principals/paul/</href></current-user-principal></prop>
        <status>HTTP/1.1 200 OK</status></propstat></response></multistatus>"#;
    let read = multistatus(principal).unwrap();
    assert_eq!(read[0].principal.as_deref(), Some("/dav/principals/paul/"));
    let data = r#"<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
        <D:response><D:href>/e.ics</D:href><D:propstat><D:prop>
        <C:calendar-data><![CDATA[BEGIN:VCALENDAR
SUMMARY:a &amp; b
END:VCALENDAR]]></C:calendar-data></D:prop><D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat></D:response></D:multistatus>"#;
    let read = multistatus(data).unwrap();
    assert!(
        read[0]
            .data
            .as_deref()
            .unwrap()
            .contains("SUMMARY:a &amp; b")
    );
}

#[test]
fn reads_what_the_account_may_do_and_a_principal_s_addresses() {
    let listing = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
        <d:response><d:href>/a/</d:href><d:propstat><d:prop>
        <d:current-user-privilege-set><d:privilege><d:read/></d:privilege>
        <d:privilege><c:read-free-busy/></d:privilege></d:current-user-privilege-set>
        </d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
        <d:response><d:href>/b/</d:href><d:propstat><d:prop>
        <d:current-user-privilege-set><d:privilege><d:read/></d:privilege>
        <d:privilege><d:bind></d:bind></d:privilege></d:current-user-privilege-set>
        </d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
        <d:response><d:href>/c/</d:href><d:propstat><d:prop>
        <c:calendar-user-address-set><d:href>mailto:paul@exemple.fr</d:href>
        <d:href> /principals/paul/ </d:href></c:calendar-user-address-set>
        </d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>
        <d:propstat><d:prop><d:current-user-privilege-set/></d:prop>
        <d:status>HTTP/1.1 404 Not Found</d:status></d:propstat></d:response>
        </d:multistatus>"#;
    let read = multistatus(listing).unwrap();
    assert_eq!(
        read[0].privileges.as_deref(),
        Some(&["read".to_string(), "read-free-busy".to_string()][..])
    );
    assert!(!read[0].takes_events());
    assert!(read[1].takes_events());
    // Not said (its property not found): the server's answer decides.
    assert_eq!(read[2].privileges, None);
    assert!(read[2].takes_events());
    assert_eq!(
        read[2].addresses,
        ["mailto:paul@exemple.fr", "/principals/paul/"]
    );
}

#[test]
fn reads_a_principal_s_inbox_and_the_agenda_it_names_for_new_events() {
    let xml = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
        <d:response><d:href>/principals/paul/</d:href><d:propstat><d:prop>
        <c:schedule-inbox-URL><d:href> /inbox/paul/ </d:href></c:schedule-inbox-URL>
        </d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
        <d:response><d:href>/inbox/paul/</d:href><d:propstat><d:prop>
        <c:schedule-default-calendar-URL><d:href>/cal/travail/</d:href>
        </c:schedule-default-calendar-URL></d:prop>
        <d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
        <d:response><d:href>/inbox/autre/</d:href><d:propstat><d:prop>
        <c:schedule-default-calendar-URL><d:href>/cal/x/</d:href>
        </c:schedule-default-calendar-URL></d:prop>
        <d:status>HTTP/1.1 404 Not Found</d:status></d:propstat></d:response>
        </d:multistatus>"#;
    let read = multistatus(xml).unwrap();
    assert_eq!(read[0].inbox.as_deref(), Some("/inbox/paul/"));
    assert_eq!(read[1].default_agenda.as_deref(), Some("/cal/travail/"));
    // A property the server says it does not have is not read.
    assert_eq!(read[2].default_agenda, None);
}

#[test]
fn what_is_not_a_multistatus_is_refused() {
    assert_eq!(multistatus("<html><body>Bienvenue</body></html>"), None);
    assert_eq!(multistatus("<multistatus><response>"), None);
    assert_eq!(multistatus("pas du XML"), None);
    // A declared entity is never expanded.
    let entity = r#"<!DOCTYPE m [<!ENTITY x "piège">]><multistatus xmlns="DAV:">
        <response><href>&x;/a</href></response></multistatus>"#;
    assert_eq!(multistatus(entity).unwrap()[0].href, "/a");
}
