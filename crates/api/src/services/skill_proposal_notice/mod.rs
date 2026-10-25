//! Tells the skills' administrators by e-mail of the agent's proposals
//! (ADR-102): a proposal reaches no run before one of them makes it
//! active, and they would otherwise learn of it only by opening
//! « Compétences ». Every minute, a process that executes runs gathers each
//! company's proposals awaiting a decision and never announced, one
//! process at a time (ADR-086), and sends each account that may decide them
//! (`skill:manage` on the whole company, as the company's rules say now,
//! `holds_company_wide`) one e-mail naming their skills, unless it chose not
//! to receive the requests awaiting its decision (ADR-094): recorded as
//! refused. Each proposal is announced once, unless the service stops
//! between the messages sent and their record; an administrator failing
//! leaves the others told, and one the relay refuses is recorded too, and
//! not sent again. Without a relay, nothing is sent nor recorded; with
//! nobody to tell, the proposals wait for an administrator. Each notice is
//! audited, without the address nor the relay's answer.

mod mail;

use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::org::load_authorization_data;
use mimorya_db::repositories::skill_proposal_notices::{
    Recipient, companies_with_unannounced_proposals, possible_skill_administrators,
    record_announced, unannounced_proposals,
};
use serde_json::json;
use uuid::Uuid;

use crate::app::Services;
use crate::mail::Mailer;
use crate::services::admin::{engine_of, holds_company_wide};
use crate::services::notification_settings::{Notice as Choice, deliver};
use mail::{ProposalsMail, proposals_mail};

/// The audited action, once per administrator told.
pub const PROPOSALS_ANNOUNCED: &str = "skill.proposals_announced";
/// The sweep's lock (ADR-086).
pub const PROPOSAL_NOTICE_SWEEP: &str = "skill-proposal-notices";
const PERMISSION: &str = "skill:manage";

/// Tells the administrators of each company of its proposals never
/// announced; how many proposals were announced. Only in a process that
/// executes runs, and one at a time: none while another tells of them.
pub async fn announce_skill_proposals(services: &Services) -> anyhow::Result<usize> {
    let Some(mailer) = services.mailer.clone() else {
        return Ok(0);
    };
    if !services.runs.executes() {
        return Ok(0);
    }
    let Some(sweep) = services.db.try_sweep_lock(PROPOSAL_NOTICE_SWEEP).await? else {
        return Ok(0);
    };
    let announced = announce_all(services, mailer.as_ref()).await;
    sweep.release().await;
    announced
}

async fn announce_all(services: &Services, mailer: &dyn Mailer) -> anyhow::Result<usize> {
    let mut announced = 0;
    for tenant_id in companies_with_unannounced_proposals(services.db.pool()).await? {
        // One company failing leaves the others to be told.
        match announce(services, mailer, tenant_id).await {
            Ok(count) => announced += count,
            Err(error) => tracing::warn!(
                tenant_id = %tenant_id,
                error = format!("{error:#}"),
                "Skill proposals not announced to their administrators"
            ),
        }
    }
    Ok(announced)
}

/// What every administrator of a company is told, in one pass.
struct Notice<'a> {
    company: &'a str,
    skills: &'a [String],
    count: usize,
}

/// Tells the company's administrators of its proposals never announced;
/// how many, none when nobody may decide them. One administrator failing
/// leaves the others told, and the proposals recorded as announced.
async fn announce(
    services: &Services,
    mailer: &dyn Mailer,
    tenant_id: Uuid,
) -> anyhow::Result<usize> {
    let mut tx = services.db.begin(tenant_id).await?;
    let proposals = unannounced_proposals(&mut tx).await?;
    let rules = load_authorization_data(&mut tx).await?;
    let company = rules.tenant.name.clone();
    let engine = engine_of(rules)?;
    let mut administrators = Vec::new();
    for candidate in possible_skill_administrators(&mut tx).await? {
        if holds_company_wide(&mut tx, &engine, candidate.user_id, PERMISSION).await? {
            administrators.push(candidate);
        }
    }
    tx.commit().await?;
    if proposals.is_empty() || administrators.is_empty() {
        return Ok(0);
    }
    let mut skills: Vec<String> = proposals.iter().map(|p| p.name.clone()).collect();
    skills.dedup();
    let notice = Notice {
        company: &company,
        skills: &skills,
        count: proposals.len(),
    };
    for administrator in &administrators {
        if let Err(error) = tell(services, mailer, tenant_id, administrator, &notice).await {
            tracing::warn!(
                tenant_id = %tenant_id,
                error = format!("{error:#}"),
                "Skill proposals not told to an administrator"
            );
        }
    }
    let ids: Vec<Uuid> = proposals.iter().map(|p| p.version_id).collect();
    let mut tx = services.db.begin(tenant_id).await?;
    record_announced(&mut tx, &ids).await?;
    tx.commit().await?;
    Ok(ids.len())
}

/// Sends `administrator` the notice, unless they chose not to receive the
/// requests awaiting their decision (ADR-094), and audits it.
async fn tell(
    services: &Services,
    mailer: &dyn Mailer,
    tenant_id: Uuid,
    administrator: &Recipient,
    notice: &Notice<'_>,
) -> anyhow::Result<()> {
    let config = &services.config;
    let link = format!("{}/admin", config.web_origin);
    let mail = proposals_mail(&ProposalsMail {
        product: &config.assistant_name,
        company: notice.company,
        administrator: &administrator.display_name,
        email: &administrator.email,
        skills: notice.skills,
        count: notice.count,
        link: &link,
    });
    let send = async {
        match mailer.send(&mail).await {
            Ok(()) => "success",
            Err(_) => {
                tracing::warn!(tenant_id = %tenant_id, "Skill proposals e-mail refused by the relay");
                "failure"
            }
        }
    };
    let user_id = administrator.user_id;
    let delivery = deliver(&services.db, tenant_id, user_id, Choice::Approvals, send).await?;
    let mut tx = services.db.begin(tenant_id).await?;
    let metadata = json!({ "proposals": notice.count, "skills": notice.skills });
    let entry = AuditEntry::new(tenant_id, "system", PROPOSALS_ANNOUNCED, delivery.outcome)
        .on_behalf_of(user_id)
        .metadata(delivery.mark(metadata));
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(())
}
