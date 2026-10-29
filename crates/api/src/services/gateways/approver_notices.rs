//! The requests awaiting a manager announced in Slack (ADR-130): when a run
//! pauses on requests a manager decides, that manager, or whom they named
//! to stand in for them while away and who may decide them (`recipient`,
//! ADR-071, as the e-mail tells), is written a direct message by the
//! company's Slack application, once their Slack identity is linked to it:
//! for whom the agent acts, the actions that may be validated in Slack
//! with their exact parameters and buttons (ADR-129), the others to
//! validate in the application. One message per run and manager, by the
//! pass of a process that executes runs, one at a time (ADR-086), within
//! `NOTIFIED_WITHIN` of the requests. Only whom may decide them now is
//! told: a manager who lost `approval:decide` over the person is written
//! nothing. Slack is asked first whether the linked Slack user is still the
//! person: once it no longer says so, the identity is unlinked and nothing
//! is written. Nobody linked to tell: nothing written, recorded so. While
//! the gateway is paused, nothing is tried: enabled again, it tells of the
//! requests still waiting. What Slack refuses is not tried again; Slack
//! unreachable, tried again a few times. Told in addition to the
//! e-mail (ADR-060), whatever the person chose of their e-mails (ADR-094):
//! linking their Slack identity is their choice, unlinking it ends these
//! messages. Each request told of is audited, never what was written.

use chrono::{DateTime, Duration, Utc};
use mimorya_core::gateways::GatewayKind;
use mimorya_core::gateways::{ShownAction, validated_in_gateway};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::gateway_approval_notices::{
    RunToNotify, SlackApprovalNotice, gateway_approval_notices, gateway_approvals_to_notify,
    record_gateway_approval_notice,
};
use mimorya_db::repositories::gateway_identities::gateway_identity_of;
use mimorya_db::repositories::gateway_replies::ReplyRecord;
use mimorya_db::repositories::gateways::{GatewayRow, list_gateways};
use serde_json::json;
use uuid::Uuid;

use super::approver_text::{ApproverNotice, approver_notice};
use super::bot_token_name;
use super::person::slack_says_theirs;
use super::replies::after_failure;
use super::reply_text::MAX_SHOWN_ACTIONS;
use crate::app::Services;
use crate::services::runs::{Recipient, may_decide_for, recipient};

/// How long after a request it is still told of in Slack: one that Slack
/// could not be reached for is not, days later.
pub const NOTIFIED_WITHIN: Duration = Duration::hours(24);
/// The runs a pass tells of, at most.
const PER_PASS: i32 = 20;
/// The pass's lock (ADR-086).
const NOTICES_SWEEP: &str = "gateway-approval-notices";
/// The audited action of a request told of in Slack.
pub const NOTIFIED_IN_SLACK: &str = "gateway.approval_notified";

/// Tells in Slack the managers whom requests made since `NOTIFIED_WITHIN`
/// await, still to tell of; how many messages were sent. Only in a process
/// that executes runs, and one at a time: none while another tells them.
pub async fn notify_approvers_in_slack(services: &Services) -> anyhow::Result<usize> {
    if !services.runs.executes() {
        return Ok(0);
    }
    // Most passes find nothing: the lock, on a connection of its own, is
    // taken only when there is something to tell.
    let now = Utc::now();
    let pool = services.db.pool();
    if gateway_approvals_to_notify(pool, now - NOTIFIED_WITHIN, now, 1)
        .await?
        .is_empty()
    {
        return Ok(0);
    }
    let Some(sweep) = services.db.try_sweep_lock(NOTICES_SWEEP).await? else {
        return Ok(0);
    };
    let sent = notify_since(services, Utc::now()).await;
    sweep.release().await;
    sent
}

async fn notify_since(services: &Services, now: DateTime<Utc>) -> anyhow::Result<usize> {
    let since = now - NOTIFIED_WITHIN;
    let runs = gateway_approvals_to_notify(services.db.pool(), since, now, PER_PASS).await?;
    let mut sent = 0;
    for run in runs {
        // One run failing leaves the others to tell of.
        match notify_run(services, run, since, now).await {
            Ok(count) => sent += count,
            Err(error) => tracing::warn!(
                run_id = %run.run_id,
                error = format!("{error:#}"),
                "Slack approval notices failed"
            ),
        }
    }
    Ok(sent)
}

/// Tells of the requests of `run` awaiting each manager; how many messages
/// were sent.
async fn notify_run(
    services: &Services,
    run: RunToNotify,
    since: DateTime<Utc>,
    now: DateTime<Utc>,
) -> anyhow::Result<usize> {
    let tenant_id = run.tenant_id;
    let mut tx = services.db.begin(tenant_id).await?;
    let gateway = (list_gateways(&mut tx).await?.into_iter())
        .find(|g| g.kind == GatewayKind::Slack.as_str() && g.enabled);
    let Some(gateway) = gateway else {
        // Paused or removed since it was listed.
        tx.commit().await?;
        return Ok(0);
    };
    let requests = gateway_approval_notices(&mut tx, run.run_id, since, now).await?;
    let token = (services.gateways)
        .secret(&mut tx, tenant_id, &bot_token_name(gateway.id))
        .await?;
    tx.commit().await?;
    let mut sent = 0;
    for theirs in requests.chunk_by(|a, b| a.notice.approver_user_id == b.notice.approver_user_id) {
        let to = recipient(&services.db, &services.access, tenant_id, &theirs[0].notice).await?;
        let told = Told {
            tenant_id,
            gateway: &gateway,
            token: token.as_deref(),
            to: &to,
        };
        let record = tell(services, &told, theirs, now).await?;
        record_each(services, &told, theirs, record).await?;
        if record == ReplyRecord::Done("sent") {
            sent += 1;
        }
    }
    Ok(sent)
}

/// Whom requests are told of, through which gateway.
struct Told<'a> {
    tenant_id: Uuid,
    gateway: &'a GatewayRow,
    /// The bot's token, if it opens.
    token: Option<&'a str>,
    to: &'a Recipient,
}

/// Writes `told.to` the requests `theirs`, all awaiting the same manager;
/// what became of the message.
async fn tell(
    services: &Services,
    told: &Told<'_>,
    theirs: &[SlackApprovalNotice],
    now: DateTime<Utc>,
) -> anyhow::Result<ReplyRecord> {
    let (gateway, to) = (told.gateway, told.to);
    // No master key, or another one: told the operator.
    let Some(token) = told.token else {
        tracing::warn!(gateway = %gateway.id, "Slack gateway's bot token unreadable");
        return Ok(ReplyRecord::Done("failed"));
    };
    // Their exact parameters only to whom may decide them now, as the
    // « Validations » screen shows them to none other.
    let requester = theirs[0].notice.requested_for_user_id;
    if !may_decide(services, told.tenant_id, to.user_id, requester).await? {
        return Ok(ReplyRecord::Done("skipped"));
    }
    let mut tx = services.db.begin(told.tenant_id).await?;
    let identity = gateway_identity_of(&mut tx, gateway.id, to.user_id).await?;
    tx.commit().await?;
    let Some(identity) = identity else {
        return Ok(ReplyRecord::Done("skipped"));
    };
    let attempt = theirs.iter().map(|r| r.attempts).max().unwrap_or(0) + 1;
    let linked = (identity.external_id.as_str(), to.user_id);
    let verdict = slack_says_theirs(
        services,
        told.tenant_id,
        (gateway, token),
        linked,
        &to.email,
    )
    .await?;
    match verdict {
        Ok(true) => {}
        Ok(false) => return Ok(ReplyRecord::Done("skipped")),
        Err(failure) => return Ok(after_failure(&failure, attempt, now)),
    }
    let shown: Vec<ShownAction> = (theirs.iter())
        .filter(|r| validated_in_gateway(&r.notice.risk, &r.payload))
        .take(MAX_SHOWN_ACTIONS)
        .map(|r| ShownAction {
            approval_id: r.notice.approval_id,
            summary: &r.action_summary,
            risk: &r.notice.risk,
            parameters: &r.payload,
        })
        .collect();
    let link = services.gateways.approvals_url();
    let mut notice = ApproverNotice {
        requested_for: &theirs[0].notice.requested_for_name,
        absent: to.absent.as_deref(),
        shown: &shown,
        in_application: theirs.len() - shown.len(),
        link: &link,
    };
    let (text, blocks) = approver_notice(&notice);
    // Their direct conversation with the application.
    let channel = &identity.external_id;
    let slack = services.gateways.slack();
    let posted = match slack
        .post_blocks(token, channel, None, &text, blocks.as_deref())
        .await
    {
        // Buttons Slack would not show: said without them.
        Err(failure) if blocks.is_some() && failure.blocks_refused() => {
            let reason = failure.message();
            tracing::warn!(gateway = %gateway.id, %reason, "Slack refused the notice's buttons");
            notice.shown = &[];
            notice.in_application = theirs.len();
            let (text, _) = approver_notice(&notice);
            slack.post(token, channel, None, &text).await
        }
        posted => posted,
    };
    Ok(match posted {
        Ok(_) => ReplyRecord::Done("sent"),
        Err(failure) => after_failure(&failure, attempt, now),
    })
}

/// Whether `user_id` may decide, now, requests made for `requester`
/// (`approval:decide` over them, ADR-059): a manager may have lost the
/// right since the requests were made, or while their notice waited.
async fn may_decide(
    services: &Services,
    tenant_id: Uuid,
    user_id: Uuid,
    requester: Uuid,
) -> anyhow::Result<bool> {
    let access = &services.access;
    let (Some(decider), Some(requester_access)) = (
        access.active_context(tenant_id, user_id).await?,
        access.active_context(tenant_id, requester).await?,
    ) else {
        return Ok(false);
    };
    let unit = requester_access.profile.primary_unit_id;
    Ok(may_decide_for(&decider, requester, unit))
}

/// Records what became of the notice of each request of `theirs`, and
/// audits each told of, or that could not be.
async fn record_each(
    services: &Services,
    told: &Told<'_>,
    theirs: &[SlackApprovalNotice],
    record: ReplyRecord,
) -> anyhow::Result<()> {
    let (tenant_id, to) = (told.tenant_id, told.to);
    let mut tx = services.db.begin(tenant_id).await?;
    for request in theirs {
        let notice = &request.notice;
        let whom = (told.gateway.id, to.user_id);
        record_gateway_approval_notice(&mut tx, tenant_id, notice.approval_id, whom, record)
            .await?;
        let outcome = match record {
            ReplyRecord::Done("sent") => "success",
            ReplyRecord::Done("refused" | "failed") => "failure",
            _ => continue,
        };
        let mut metadata = json!({
            "gatewayId": told.gateway.id,
            "approverId": notice.approver_user_id,
            "grouped": theirs.len(),
        });
        if to.user_id != notice.approver_user_id {
            metadata["delegateId"] = json!(to.user_id);
        }
        let entry = AuditEntry::new(tenant_id, "system", NOTIFIED_IN_SLACK, outcome)
            .resource("approval", notice.approval_id)
            .run(notice.run_id)
            .on_behalf_of(notice.requested_for_user_id)
            .metadata(metadata);
        append_audit(&mut tx, entry).await?;
    }
    tx.commit().await?;
    Ok(())
}
