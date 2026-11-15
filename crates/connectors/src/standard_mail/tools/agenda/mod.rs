//! Tool `mail_calendar_events` (ADR-205): the events of the person the run
//! serves over a few days, in their agendas at the company's CalDAV server,
//! with the account they linked; each occurrence of a recurring one apart
//! where the server expands them, in the platform's zone, the earliest
//! first; what each says is written by `shown`. Offered only where the
//! company declared an agenda.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{MailServerRole, ToolContext, ToolInfo, ToolResult, TypedTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use validator::Validate;

use self::shown::{Listed, citation, event, instant, overlaps};
use super::person::{MailReach, PersonAccount, refused, told};
use crate::delegated::shape::{day_end, day_start};
use crate::outbound::{CONNECTOR_USE, Cause};
use crate::standard_mail::CREDENTIALS_REFUSED;
use crate::standard_mail::caldav::{CalDav, agenda_events, agendas};

mod shown;

/// The longest period read at once, in days.
pub const MAX_DAYS: i64 = 31;
/// The most events told.
const MAX_SHOWN: usize = 50;
/// What a company with no agenda declared is told.
const NO_CALDAV: &str = "La messagerie de l’entreprise n’a pas d’agenda CalDAV déclaré";

pub struct MailCalendarEvents {
    pub reach: MailReach,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct MailCalendarEventsInput {
    /// Premier jour inclus (AAAA-MM-JJ)
    from: NaiveDate,
    /// Dernier jour inclus (AAAA-MM-JJ) : 31 jours au plus, premier et dernier compris
    to: NaiveDate,
}

/// Why the period is not read, if it is not.
fn period_problem(input: &MailCalendarEventsInput) -> Option<String> {
    let days = (input.to - input.from).num_days();
    if days < 0 {
        return Some("Le dernier jour précède le premier".into());
    }
    (days >= MAX_DAYS).then(|| format!("Période de {MAX_DAYS} jours au plus"))
}

#[async_trait]
impl TypedTool for MailCalendarEvents {
    type Input = MailCalendarEventsInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "mail_calendar_events",
            "Agenda (messagerie de l’entreprise)",
            "Renvoie les rendez-vous des agendas de la personne servie sur l’agenda CalDAV de l’entreprise, avec le compte qu’elle a relié, sur une période (31 jours au plus, bornes comprises) : objet, début et fin (heure de Paris), lieu, organisateur, participants.",
            ToolRisk::Read,
        )
        .requires([CONNECTOR_USE])
    }

    async fn run(
        &self,
        input: MailCalendarEventsInput,
        ctx: &ToolContext,
    ) -> anyhow::Result<ToolResult> {
        if let Some(problem) = period_problem(&input) {
            return Ok(ToolResult::error(problem));
        }
        let period = (day_start(input.from), day_end(input.to));
        let read = self.reach.within(self.read(ctx, period)).await;
        let mut listed = match read {
            Ok(listed) => listed,
            Err(why) => return Ok(ToolResult::error(why)),
        };
        // The server's filter is not trusted: what falls outside is left.
        listed.retain(|listed| overlaps(&listed.event, period));
        listed.sort_by_key(|listed| listed.event.start.map(instant));
        let total = listed.len();
        let shown: Vec<&Listed> = listed.iter().take(MAX_SHOWN).collect();
        let mut content = json!({
            "periode": { "du": input.from, "au": input.to },
            "nombre": shown.len(),
            "rendezVous": shown.iter().map(|listed| event(listed)).collect::<Vec<_>>(),
        });
        if total > MAX_SHOWN {
            content["autres"] = json!(format!(
                "{} autre(s) rendez-vous : réduisez la période",
                total - MAX_SHOWN
            ));
        }
        Ok(ToolResult::ok(content)
            .with_summary(format!("{} rendez-vous", shown.len()))
            .with_citations(shown.iter().filter_map(|l| citation(&l.event)).collect()))
    }
}

impl MailCalendarEvents {
    /// The events of the run's person's agendas over `period`, or why
    /// there are none, in French.
    async fn read(
        &self,
        ctx: &ToolContext,
        period: (DateTime<Utc>, DateTime<Utc>),
    ) -> Result<Vec<Listed>, String> {
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
                false => told(&login, cause),
            }
        };
        let dav = match CalDav::new(address, &login, self.reach.network) {
            Ok(dav) => dav,
            Err(cause) => return Err(failed(cause).await),
        };
        let found = match agendas(&dav).await {
            Ok(found) => found,
            Err(cause) => return Err(failed(cause).await),
        };
        let mut listed = Vec::new();
        for agenda in &found {
            let events = match agenda_events(&dav, agenda, period).await {
                Ok(events) => events,
                Err(cause) => return Err(failed(cause).await),
            };
            listed.extend(events.into_iter().map(|event| Listed {
                agenda: agenda.name.clone(),
                event,
            }));
        }
        Ok(listed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn period(from: &str, to: &str) -> MailCalendarEventsInput {
        serde_json::from_value(json!({ "from": from, "to": to })).unwrap()
    }

    #[test]
    fn a_period_is_read_up_to_its_bound() {
        assert_eq!(period_problem(&period("2026-10-01", "2026-10-31")), None);
        assert!(period_problem(&period("2026-10-01", "2026-11-01")).is_some());
        assert!(period_problem(&period("2026-10-02", "2026-10-01")).is_some());
    }
}
