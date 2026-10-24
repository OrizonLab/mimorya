//! Tells a mission's author by e-mail that it is over (ADR-091): it ran in
//! the background, and they would otherwise learn its end only by opening
//! « Missions ». Done, or stopped unfinished, whichever path ended it (its
//! last stage failed, a stop of the executor, the stale-run sweep); not when
//! they stopped it themselves, nor if they chose not to receive these
//! e-mails (ADR-094): recorded as refused. Every minute, a process that executes runs
//! tells of the missions over since `TOLD_WITHIN`, one process at a time
//! (ADR-086): each is told of once, unless the service stops between the
//! message sent and its record. A message the relay refuses is recorded
//! too, and not sent again; without a relay, nothing is sent. Each notice
//! is audited, without the address nor the relay's answer.

mod mail;

use chrono::{DateTime, Duration, Utc};
use mimorya_agent::runtime::VerificationReport;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::mission_end_notices::{
    EndedMission, MissionEndNotice, mission_end_notice, record_end_notice, unnotified_mission_ends,
};
use serde_json::json;

use crate::app::Services;
use crate::mail::Mailer;
use crate::services::notification_settings::{Notice, OPTED_OUT, wants};
use mail::{End, EndMail, Review, end_mail};

/// The audited action, once per mission told of.
pub const MISSION_NOTIFIED: &str = "mission.notified";
/// The sweep's lock (ADR-086).
const END_NOTICE_SWEEP: &str = "mission-end-notices";
/// How long after its end a mission is still told of: one that ended while
/// no relay was configured is not, days later, once one is.
pub const TOLD_WITHIN: Duration = Duration::hours(24);

/// Tells their authors of the missions over since `TOLD_WITHIN`, never
/// told of; how many. Only in a process that executes runs, and one at a
/// time: none while another tells of them.
pub async fn tell_ended_missions(services: &Services) -> anyhow::Result<usize> {
    let Some(mailer) = services.mailer.clone() else {
        return Ok(0);
    };
    if !services.runs.executes() {
        return Ok(0);
    }
    let Some(sweep) = services.db.try_sweep_lock(END_NOTICE_SWEEP).await? else {
        return Ok(0);
    };
    let told = tell_since(services, mailer.as_ref(), Utc::now() - TOLD_WITHIN).await;
    sweep.release().await;
    told
}

async fn tell_since(
    services: &Services,
    mailer: &dyn Mailer,
    since: DateTime<Utc>,
) -> anyhow::Result<usize> {
    let mut told = 0;
    for ended in unnotified_mission_ends(services.db.pool(), since).await? {
        // One mission failing leaves the others to be told of.
        match tell(services, mailer, ended, since).await {
            Ok(true) => told += 1,
            Ok(false) => {}
            Err(error) => tracing::warn!(
                mission_id = %ended.mission_id,
                error = format!("{error:#}"),
                "Mission's end not told to its author"
            ),
        }
    }
    Ok(told)
}

/// Tells the author of `ended`, if it is still to be told of; whether it was.
async fn tell(
    services: &Services,
    mailer: &dyn Mailer,
    ended: EndedMission,
    since: DateTime<Utc>,
) -> anyhow::Result<bool> {
    let EndedMission {
        tenant_id,
        mission_id,
    } = ended;
    let mut tx = services.db.begin(tenant_id).await?;
    let notice = mission_end_notice(&mut tx, mission_id, since).await?;
    tx.commit().await?;
    let Some(notice) = notice else {
        return Ok(false);
    };
    let config = &services.config;
    let link = format!("{}/missions", config.web_origin);
    let end = end_of(&notice)?;
    let mail = end_mail(&EndMail {
        product: &config.assistant_name,
        company: &notice.company,
        owner: &notice.owner_name,
        email: &notice.owner_email,
        title: &notice.title,
        end,
        link: &link,
    });
    let owner = notice.owner_user_id;
    let opted_out = !wants(&services.db, tenant_id, owner, Notice::MissionEnds).await?;
    let outcome = if opted_out {
        OPTED_OUT
    } else {
        match mailer.send(&mail).await {
            Ok(()) => "success",
            Err(_) => {
                tracing::warn!(mission_id = %mission_id, "Mission end e-mail refused by the relay");
                "failure"
            }
        }
    };
    let mut tx = services.db.begin(tenant_id).await?;
    record_end_notice(&mut tx, mission_id).await?;
    let entry = AuditEntry::new(tenant_id, "system", MISSION_NOTIFIED, outcome)
        .on_behalf_of(notice.owner_user_id)
        .resource("mission", mission_id)
        .metadata(if opted_out {
            json!({ "status": end.status(), "optedOut": true })
        } else {
            json!({ "status": end.status() })
        });
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(!opted_out)
}

/// How the mission ended, as its author is told: done, with the review of
/// its final answer, or unfinished.
fn end_of(notice: &MissionEndNotice) -> anyhow::Result<End> {
    if notice.status != "done" {
        return Ok(End::Unfinished);
    }
    let Some(value) = &notice.verification else {
        return Ok(End::Done(Review::Unjudged));
    };
    let report: VerificationReport = serde_json::from_value(value.clone())?;
    let review = match (report.passed, report.judged) {
        (false, _) => Review::Unverified,
        (true, false) => Review::Unjudged,
        (true, true) => Review::Verified,
    };
    Ok(End::Done(review))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    fn ended(status: &str, verification: Option<serde_json::Value>) -> MissionEndNotice {
        MissionEndNotice {
            mission_id: Uuid::nil(),
            title: "Suivi de la semaine".into(),
            status: status.into(),
            verification,
            owner_user_id: Uuid::nil(),
            owner_email: "marc.lefevre@neo-maintenance.example".into(),
            owner_name: "Marc Lefèvre".into(),
            company: "Néo Maintenance".into(),
        }
    }

    fn review(passed: bool, judged: bool) -> Option<serde_json::Value> {
        Some(json!({ "passed": passed, "issues": [], "judged": judged }))
    }

    #[test]
    fn a_mission_done_is_told_verified_only_when_a_reviewer_judged_it() {
        let told = |verification| end_of(&ended("done", verification)).unwrap();
        assert_eq!(told(review(true, true)), End::Done(Review::Verified));
        assert_eq!(told(review(false, true)), End::Done(Review::Unverified));
        assert_eq!(told(review(false, false)), End::Done(Review::Unverified));
        assert_eq!(told(review(true, false)), End::Done(Review::Unjudged));
        assert_eq!(told(None), End::Done(Review::Unjudged));
    }

    #[test]
    fn a_mission_failed_or_whose_last_stage_failed_is_told_unfinished() {
        for status in ["failed", "in_progress"] {
            let end = end_of(&ended(status, review(true, true))).unwrap();
            assert_eq!(end, End::Unfinished);
            assert_eq!(end.status(), "failed");
        }
        assert_eq!(End::Done(Review::Verified).status(), "done");
    }
}
