//! A company's CalDAV agenda, reached as a person with their account
//! (ADR-205): requests over HTTPS to the declared server alone (`client`),
//! the person's agendas found there (`agendas`), their events over a period
//! (`report`), what the server answers (`xml`) and the events it holds
//! (`ical`), when they start, end and last (`time`); and an account checked
//! against it when a person links it.

mod agendas;
mod client;
mod ical;
mod report;
mod time;
mod xml;

pub use agendas::{NO_AGENDA, agendas};
pub use client::{
    ACCESS_REFUSED, CalDav, ELSEWHERE, NO_AGENDA_THERE, NOT_AN_AGENDA, NOT_HTTPS, REDIRECTED,
};
pub use ical::{Event, Person, When};
pub use report::agenda_events;
pub use time::in_zone;
pub use xml::Found;

use tokio::time::timeout;

use super::{MailLogin, SIGN_IN_TIMEOUT};
use crate::outbound::{Cause, NetworkPolicy, TOO_SLOW};

/// Whether `login` finds an agenda of events at `address`, within the
/// sign-in delay.
pub async fn check_caldav(
    address: &str,
    login: &MailLogin,
    policy: NetworkPolicy,
) -> Result<(), Cause> {
    let found = async {
        let dav = CalDav::new(address, login, policy)?;
        agendas(&dav).await.map(|_| ())
    };
    let checked = timeout(SIGN_IN_TIMEOUT, found).await.unwrap_or(Err(Cause {
        summary: TOO_SLOW,
        detail: "connexion à l’agenda CalDAV".into(),
    }));
    // What the server said back never carries the password out.
    checked.map_err(|cause| Cause {
        detail: login.scrub(&cause.detail),
        ..cause
    })
}
