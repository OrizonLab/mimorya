//! Tells the skills' administrators by e-mail of the agent's proposals
//! (ADR-102): a proposal reaches no run before one of them makes it
//! active, and they would otherwise learn of it only by opening
//! « Compétences ». Every minute, a process that executes runs gathers each
//! company's proposals awaiting a decision and never announced, one
//! process at a time (ADR-086), and sends each account that may decide them
//! (`skill:manage` on the whole company, as the company's rules say now)
//! one e-mail naming their skills, unless it chose not to receive the
//! requests awaiting its decision (ADR-094): recorded as refused. Each
//! proposal is announced once, unless the service stops between the
//! messages sent and their record; one the relay refuses is recorded too,
//! and not sent again. Without a relay, nothing is sent nor recorded;
//! with nobody to tell, the proposals wait for an administrator. Each
//! notice is audited, without the address nor the relay's answer.

mod mail;

use mimorya_core::authz::{Actor, AuthorizationEngine, DataScope, OrgTree};
use mimorya_db::TenantTx;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::org::{AuthorizationData, load_authorization_data, load_user_access};
use mimorya_db::repositories::skill_proposal_notices::{
    Recipient, companies_with_unannounced_proposals, possible_skill_administrators,
    record_announced, unannounced_proposals,
};
use serde_json::json;
use uuid::Uuid;

use crate::app::Services;
use crate::mail::Mailer;
use crate::services::notification_settings::{Notice, deliver};
use mail::{ProposalsMail, proposals_mail};

/// The audited action, once per administrator told.
pub const PROPOSALS_ANNOUNCED: &str = "skill.proposals_announced";
/// The sweep's lock (ADR-086).
const PROPOSAL_NOTICE_SWEEP: &str = "skill-proposal-notices";
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

/// Tells the company's administrators of its proposals never announced;
/// how many, none when nobody may decide them.
async fn announce(
    services: &Services,
    mailer: &dyn Mailer,
    tenant_id: Uuid,
) -> anyhow::Result<usize> {
    let mut tx = services.db.begin(tenant_id).await?;
    let proposals = unannounced_proposals(&mut tx).await?;
    let rules = load_authorization_data(&mut tx).await?;
    let company = rules.tenant.name.clone();
    let administrators = administrators(&mut tx, rules).await?;
    tx.commit().await?;
    if proposals.is_empty() || administrators.is_empty() {
        return Ok(0);
    }
    let mut skills: Vec<String> = proposals.iter().map(|p| p.name.clone()).collect();
    skills.dedup();
    let config = &services.config;
    let link = format!("{}/admin", config.web_origin);
    for administrator in &administrators {
        let mail = proposals_mail(&ProposalsMail {
            product: &config.assistant_name,
            company: &company,
            administrator: &administrator.display_name,
            email: &administrator.email,
            skills: &skills,
            count: proposals.len(),
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
        let delivery = deliver(
            &services.db,
            tenant_id,
            administrator.user_id,
            Notice::Approvals,
            send,
        )
        .await?;
        let mut tx = services.db.begin(tenant_id).await?;
        let metadata = json!({ "proposals": proposals.len(), "skills": skills });
        let entry = AuditEntry::new(tenant_id, "system", PROPOSALS_ANNOUNCED, delivery.outcome)
            .on_behalf_of(administrator.user_id)
            .metadata(delivery.mark(metadata));
        append_audit(&mut tx, entry).await?;
        tx.commit().await?;
    }
    let ids: Vec<Uuid> = proposals.iter().map(|p| p.version_id).collect();
    let mut tx = services.db.begin(tenant_id).await?;
    record_announced(&mut tx, &ids).await?;
    tx.commit().await?;
    Ok(ids.len())
}

/// The accounts that may decide the company's proposals now: active,
/// holding `skill:manage` on the whole company as its rules say, deny
/// policies included, as the administration checks it (`begin_change`).
async fn administrators(
    tx: &mut TenantTx,
    data: AuthorizationData,
) -> anyhow::Result<Vec<Recipient>> {
    let definitions = data.roles.into_iter().map(|r| r.definition).collect();
    let engine = AuthorizationEngine::new(OrgTree::new(data.units)?, definitions, data.policies);
    let mut administrators = Vec::new();
    for candidate in possible_skill_administrators(tx).await? {
        let access = load_user_access(tx, candidate.user_id).await?;
        let may_decide = access.is_some_and(|a| {
            a.profile.status == "active"
                && engine.data_scope(&a.subject, PERMISSION, &Actor::user()) == DataScope::All
        });
        if may_decide {
            administrators.push(candidate);
        }
    }
    Ok(administrators)
}
