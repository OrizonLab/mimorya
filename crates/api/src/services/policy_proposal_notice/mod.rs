//! Tells the other administrators of the access policies by e-mail of the
//! allow policies proposed (ADR-170): a proposal opens nothing before
//! another administrator approves it, within seven days, and they would
//! otherwise learn of it only by opening « Politiques d'accès ». Every
//! minute, a process that executes runs gathers each company's proposals
//! still waiting and never announced, one process at a time (ADR-086), and
//! sends each account that may decide them (`policy:manage` on the whole
//! company, as the company's rules say now, `company_wide_administrators`)
//! one e-mail counting those it did not propose itself, unless it chose not
//! to receive the requests awaiting its decision (ADR-094): recorded as
//! refused. A proposal that nobody but its author may decide waits,
//! unannounced, for another administrator; so does one whose author no
//! longer administers the policies, which nobody may approve meanwhile
//! (`approve.rs`), until they do again. Each proposal is announced once,
//! unless the service stops between the messages sent and their record; an
//! administrator failing leaves the others told, and one the relay refuses
//! is recorded too, and not sent again. Without a relay, nothing is sent
//! nor recorded. Each notice is audited, without the address nor the
//! relay's answer.

mod mail;

use mimorya_db::repositories::administrators::Recipient;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::org::load_authorization_data;
use mimorya_db::repositories::policy_proposal_notices::{
    UnannouncedPolicyProposal, companies_with_unannounced_policy_proposals,
    record_policy_proposals_announced, unannounced_policy_proposals,
};
use serde_json::json;
use uuid::Uuid;

use crate::app::Services;
use crate::mail::Mailer;
use crate::services::admin::{company_wide_administrators, engine_of};
use crate::services::notification_settings::{Notice as Choice, deliver};
use mail::{PolicyProposalsMail, policy_proposals_mail};

/// The audited action, once per administrator told.
pub const POLICY_PROPOSALS_ANNOUNCED: &str = "policy.proposals_announced";
/// The sweep's lock (ADR-086).
pub const POLICY_PROPOSAL_NOTICE_SWEEP: &str = "policy-proposal-notices";
const PERMISSION: &str = "policy:manage";

/// Tells the administrators of each company of its proposals never
/// announced; how many proposals were announced. Only in a process that
/// executes runs, and one at a time: none while another tells of them.
pub async fn announce_policy_proposals(services: &Services) -> anyhow::Result<usize> {
    let Some(mailer) = services.mailer.clone() else {
        return Ok(0);
    };
    if !services.runs.executes() {
        return Ok(0);
    }
    let Some(sweep) = services
        .db
        .try_sweep_lock(POLICY_PROPOSAL_NOTICE_SWEEP)
        .await?
    else {
        return Ok(0);
    };
    let announced = announce_all(services, mailer.as_ref()).await;
    sweep.release().await;
    announced
}

async fn announce_all(services: &Services, mailer: &dyn Mailer) -> anyhow::Result<usize> {
    let mut announced = 0;
    for tenant_id in companies_with_unannounced_policy_proposals(services.db.pool()).await? {
        // One company failing leaves the others to be told.
        match announce(services, mailer, tenant_id).await {
            Ok(count) => announced += count,
            Err(error) => tracing::warn!(
                tenant_id = %tenant_id,
                error = format!("{error:#}"),
                "Policy proposals not announced to their administrators"
            ),
        }
    }
    Ok(announced)
}

/// Tells each administrator of the company of the proposals never
/// announced that another made, while their author administers the
/// policies; how many proposals were told to someone.
/// One administrator failing leaves the others told, and the proposals
/// recorded as announced.
async fn announce(
    services: &Services,
    mailer: &dyn Mailer,
    tenant_id: Uuid,
) -> anyhow::Result<usize> {
    let mut tx = services.db.begin(tenant_id).await?;
    let mut proposals = unannounced_policy_proposals(&mut tx).await?;
    let rules = load_authorization_data(&mut tx).await?;
    let company = rules.tenant.name.clone();
    let engine = engine_of(rules)?;
    let administrators = company_wide_administrators(&mut tx, &engine, PERMISSION).await?;
    tx.commit().await?;
    // Its author no longer administering the policies, nobody may approve
    // it: it waits for them to again, or for its expiry.
    proposals.retain(|p| {
        administrators
            .iter()
            .any(|a| a.user_id == p.proposed_by_user_id)
    });

    let mut told = Vec::new();
    for administrator in &administrators {
        let theirs: Vec<&UnannouncedPolicyProposal> = proposals
            .iter()
            .filter(|p| p.proposed_by_user_id != administrator.user_id)
            .collect();
        if theirs.is_empty() {
            continue;
        }
        told.extend(theirs.iter().map(|p| p.id));
        if let Err(error) = tell(
            services,
            mailer,
            tenant_id,
            &company,
            administrator,
            &theirs,
        )
        .await
        {
            tracing::warn!(
                tenant_id = %tenant_id,
                error = format!("{error:#}"),
                "Policy proposals not told to an administrator"
            );
        }
    }
    told.sort();
    told.dedup();
    if told.is_empty() {
        return Ok(0);
    }
    let mut tx = services.db.begin(tenant_id).await?;
    record_policy_proposals_announced(&mut tx, &told).await?;
    tx.commit().await?;
    Ok(told.len())
}

/// Sends `administrator` the notice of `proposals`, unless they chose not
/// to receive the requests awaiting their decision (ADR-094), and audits it.
async fn tell(
    services: &Services,
    mailer: &dyn Mailer,
    tenant_id: Uuid,
    company: &str,
    administrator: &Recipient,
    proposals: &[&UnannouncedPolicyProposal],
) -> anyhow::Result<()> {
    let config = &services.config;
    let link = format!("{}/admin", config.web_origin);
    let mut authors: Vec<String> = Vec::new();
    for proposal in proposals {
        if !authors.contains(&proposal.proposed_by_name) {
            authors.push(proposal.proposed_by_name.clone());
        }
    }
    let mail = policy_proposals_mail(&PolicyProposalsMail {
        product: &config.assistant_name,
        company,
        administrator: &administrator.display_name,
        email: &administrator.email,
        authors: &authors,
        count: proposals.len(),
        link: &link,
    });
    let send = async {
        match mailer.send(&mail).await {
            Ok(()) => "success",
            Err(_) => {
                tracing::warn!(tenant_id = %tenant_id, "Policy proposals e-mail refused by the relay");
                "failure"
            }
        }
    };
    let user_id = administrator.user_id;
    let delivery = deliver(&services.db, tenant_id, user_id, Choice::Approvals, send).await?;
    let ids: Vec<Uuid> = proposals.iter().map(|p| p.id).collect();
    let mut tx = services.db.begin(tenant_id).await?;
    let metadata = json!({ "proposals": ids });
    let entry = AuditEntry::new(
        tenant_id,
        "system",
        POLICY_PROPOSALS_ANNOUNCED,
        delivery.outcome,
    )
    .on_behalf_of(user_id)
    .metadata(delivery.mark(metadata));
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(())
}
