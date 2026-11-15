//! Tool `mail_calendar_event_create` (ADR-205): creates, at the company's
//! CalDAV server and with the account the person the run serves linked,
//! the event they validated, in the agenda their server takes new events
//! in by default (RFC 6638) when it is one where the account may add
//! them, or else the first such agenda: its subject, start and length,
//! place, text and attendees,
//! exactly as shown to them. Its attendees are invited by the server
//! itself (RFC 6638), from an address it knows the account by; a server
//! that would not invite them is told before anything is created. Created
//! once, never repeated, never over another event; always validated,
//! whatever the company's policies; offered only where the company
//! declared an agenda and lets its tools write.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::time_zone::PLATFORM_TIME_ZONE;
use mimorya_core::tools::{Citation, MailServerRole, ToolContext, ToolInfo, ToolResult, TypedTool};
use reqwest_mcp::Url;
use serde_json::{Value, json};
use tokio::time::{Instant, timeout};
use uuid::Uuid;

use super::agenda::NO_CALDAV;
use super::outgoing::sender;
use super::person::{MailReach, PersonAccount, refused, told, told_plain};
use crate::delegated::NewEvent;
use crate::delegated::shape::bounded;
use crate::outbound::{CONNECTOR_USE, Cause, TOO_SLOW};
use crate::standard_mail::CREDENTIALS_REFUSED;
use crate::standard_mail::caldav::{
    CalDav, Discovered, NO_ORGANIZER, NO_SCHEDULING, Stamped, Unmade, calendar_object, chosen,
    default_agenda, discover, organizer, put, schedules, written_as_is,
};

/// What a person with no agenda taking new events is told.
pub const NO_WRITABLE_AGENDA: &str =
    "Aucun agenda de ce compte ne prend de nouveau rendez-vous (droits en lecture seule)";
/// The share of the call's time the server's default agenda may take to
/// be found, and of the time the call has left: the rest is left to
/// invite and create.
const DEFAULT_SHARE: u32 = 4;
const LEFT_SHARE: u32 = 2;
/// What an event whose attendees cannot be invited is told, after why.
const INVITE_OTHERWISE: &str =
    "créez-le sans participant et prévenez-les par e-mail, ou demandez à l’administrateur";

pub struct MailCalendarEventCreate {
    pub reach: MailReach,
}

/// An event created: where, its identifier, when, who invites.
struct Created {
    agenda: Option<String>,
    uid: String,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    organizer: Option<String>,
}

#[async_trait]
impl TypedTool for MailCalendarEventCreate {
    type Input = NewEvent;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "mail_calendar_event_create",
            "Rendez-vous (agenda de l’entreprise)",
            "Crée un rendez-vous dans l’agenda CalDAV de l’entreprise de la personne servie, avec le compte qu’elle a relié : objet, début (heure de Paris), durée, lieu, texte et participants, exactement tels qu’ils sont validés ; le serveur d’agenda envoie lui-même l’invitation aux participants, et un serveur qui ne le fait pas est signalé avant que rien ne soit créé.",
            ToolRisk::External,
        )
        .requires([CONNECTOR_USE])
        .always_requires_approval()
    }

    fn describe_action(&self, input: &NewEvent) -> Option<String> {
        Some(input.described("de messagerie"))
    }

    async fn run(&self, input: NewEvent, ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let put_started = AtomicBool::new(false);
        let creating = self.create(&input, ctx, &put_started);
        let created = match timeout(self.reach.call_timeout, creating).await {
            Ok(Ok(created)) => created,
            Ok(Err(why)) => return Ok(ToolResult::error(why)),
            Err(_) => {
                let said = told_plain(TOO_SLOW, "");
                return Ok(ToolResult::error(
                    match put_started.load(Ordering::SeqCst) {
                        true => perhaps_created(&said),
                        false => nothing_created(&said),
                    },
                ));
            }
        };
        let content = json!({
            "cree": true,
            "id": created.uid,
            "agenda": created.agenda.map(|name| bounded(&name, 120).0),
            "objet": input.subject,
            "debut": in_platform_zone(created.start),
            "fin": in_platform_zone(created.end),
            "participants": input.attendees,
            "organisateur": created.organizer,
        });
        let citation = Citation::new("event", created.uid).labelled(bounded(&input.subject, 120).0);
        Ok(ToolResult::ok(content)
            .with_summary("Rendez-vous créé")
            .with_citations(vec![citation]))
    }
}

impl MailCalendarEventCreate {
    /// Creates `input` as the run's person; `put_started` set once the
    /// event is on its way to the server.
    async fn create(
        &self,
        input: &NewEvent,
        ctx: &ToolContext,
        put_started: &AtomicBool,
    ) -> Result<Created, String> {
        // Where the call's time is counted from (`run` bounds it).
        let started = Instant::now();
        if let Some(address) = input.attendees.iter().find(|a| !written_as_is(a)) {
            return Err(nothing_created(&format!(
                "L’adresse de participant {address} ne s’écrit pas telle quelle dans une \
                 invitation d’agenda (caractères ? # % & = / ^ ` {{ | }})"
            )));
        }
        let Some((start, end)) = input.instants() else {
            return Err(nothing_created(
                "Début du rendez-vous hors des dates permises",
            ));
        };
        let PersonAccount {
            account,
            config,
            login,
        } = self.reach.account(ctx).await?;
        let Some(address) = config.caldav.as_deref() else {
            return Err(NO_CALDAV.into());
        };
        let failed = |cause: Cause| async {
            match cause.summary == CREDENTIALS_REFUSED {
                true => refused(ctx, MailServerRole::CalDav, account.linked_at).await,
                false => nothing_created(&told(&login, cause)),
            }
        };
        let dav = match CalDav::new(address, &login, self.reach.network) {
            Ok(dav) => dav,
            Err(cause) => return Err(failed(cause).await),
        };
        let Discovered { agendas, principal } = match discover(&dav).await {
            Ok(found) => found,
            Err(cause) => return Err(failed(cause).await),
        };
        // The server's default is asked only where there is a choice.
        let choice = agendas.iter().filter(|agenda| agenda.takes_events).count() > 1;
        let default = match (choice, principal.as_ref()) {
            (true, Some(principal)) => {
                let patience = default_patience(self.reach.call_timeout, started.elapsed());
                default_agenda(&dav, principal, patience).await
            }
            _ => None,
        };
        let Some(agenda) = chosen(agendas, default.as_ref()) else {
            return Err(nothing_created(NO_WRITABLE_AGENDA));
        };
        let organizer = match input.attendees.is_empty() {
            true => None,
            false => {
                let invited = invites(&dav, &agenda.url, principal.as_ref(), sender(&account));
                match invited.await {
                    Ok(Ok(organizer)) => Some(organizer),
                    Ok(Err(why)) => {
                        return Err(nothing_created(&format!("{why} : {INVITE_OTHERWISE}")));
                    }
                    Err(cause) => return Err(failed(cause).await),
                }
            }
        };
        let uid = Uuid::new_v4().simple().to_string();
        let stamped = Stamped {
            uid: &uid,
            stamp: Utc::now(),
            start,
            end,
        };
        let text = calendar_object(input, &stamped, organizer.as_deref());
        let url =
            object_url(&agenda.url, &uid).ok_or_else(|| nothing_created(NO_WRITABLE_AGENDA))?;
        put_started.store(true, Ordering::SeqCst);
        match put(&dav, &url, text).await {
            Ok(()) => Ok(Created {
                agenda: agenda.name,
                uid,
                start,
                end,
                organizer,
            }),
            Err(Unmade::Nothing(cause)) => Err(failed(cause).await),
            Err(Unmade::Unsure(cause)) => Err(perhaps_created(&told(&login, cause))),
        }
    }
}

/// How long the server's default agenda may take to be found, `spent` of
/// a call of `call` gone: a quarter of the call, never more than half of
/// what it has left.
fn default_patience(call: Duration, spent: Duration) -> Duration {
    (call / DEFAULT_SHARE).min(call.saturating_sub(spent) / LEFT_SHARE)
}

/// The address that invites the attendees, once the server is known to
/// send invitations at `agenda`; or why it would not, in French.
async fn invites(
    dav: &CalDav<'_>,
    agenda: &Url,
    principal: Option<&Url>,
    sender: Option<&str>,
) -> Result<Result<String, &'static str>, Cause> {
    if !schedules(dav, agenda).await? {
        return Ok(Err(NO_SCHEDULING));
    }
    let Some(principal) = principal else {
        return Ok(Err(NO_ORGANIZER));
    };
    Ok(organizer(dav, principal, sender).await?.ok_or(NO_ORGANIZER))
}

/// The address of a new object named `uid` in `agenda`, a collection
/// whether or not its address ends with `/`.
fn object_url(agenda: &Url, uid: &str) -> Option<Url> {
    let mut collection = agenda.clone();
    if !collection.path().ends_with('/') {
        let path = format!("{}/", collection.path());
        collection.set_path(&path);
    }
    collection.join(&format!("{uid}.ics")).ok()
}

fn in_platform_zone(at: DateTime<Utc>) -> Value {
    json!(at.with_timezone(&PLATFORM_TIME_ZONE).to_rfc3339())
}

/// A failure that left nothing created, as the model reads it.
fn nothing_created(said: &str) -> String {
    format!("{said} — rien n’est créé")
}

/// A failure once the event was on its way, as the model reads it.
fn perhaps_created(said: &str) -> String {
    format!(
        "{said} — le rendez-vous a été transmis sans que le serveur confirme l’avoir pris : il \
         est peut-être créé, vérifiez l’agenda avant de le créer de nouveau"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_agenda_waits_a_quarter_of_the_call_at_most_half_what_is_left() {
        let call = Duration::from_secs(20);
        let patience = |spent| default_patience(call, Duration::from_secs(spent));
        assert_eq!(patience(0), Duration::from_secs(5));
        assert_eq!(patience(10), Duration::from_secs(5));
        assert_eq!(patience(16), Duration::from_secs(2));
        assert_eq!(patience(25), Duration::ZERO);
    }

    #[test]
    fn an_event_is_put_inside_its_agenda() {
        let at = |agenda: &str| {
            object_url(&Url::parse(agenda).unwrap(), "abc")
                .unwrap()
                .to_string()
        };
        assert_eq!(
            at("https://dav.exemple.fr/cal/paul/"),
            "https://dav.exemple.fr/cal/paul/abc.ics"
        );
        assert_eq!(
            at("https://dav.exemple.fr/cal/paul"),
            "https://dav.exemple.fr/cal/paul/abc.ics"
        );
    }
}
