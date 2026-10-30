//! The requests awaiting a manager announced in a messaging service: in
//! Slack (ADR-130), in Telegram (ADR-137), in Teams (ADR-142). When a run
//! pauses on requests a manager decides, that manager, or whom they named
//! to stand in for them while away and who may decide them (`recipient`,
//! ADR-071, as the e-mail tells), is written through each of the company's
//! gateways their account is linked to: a direct message of its Slack
//! application (`slack`), a message in their private chat with its Telegram
//! bot (`telegram`), in their personal conversation with its Teams bot
//! (`teams`). For whom the agent acts, the actions that may be validated
//! there with their exact parameters and buttons (ADR-129, ADR-136,
//! ADR-141), the others to validate in the application. One message per run, manager and gateway, by the
//! pass of a process that executes runs, one at a time (ADR-086), within
//! `NOTIFIED_WITHIN` of the requests. Only whom may decide them now is
//! told: a manager who lost `approval:decide` over the person is written
//! nothing. Nobody linked to tell: nothing written, recorded so. While a
//! gateway is paused, nothing is tried through it: enabled again, it tells
//! of the requests still waiting. What the service refuses is not tried
//! again; unreachable, tried again a few times. Told in addition to the
//! e-mail (ADR-060), whatever the person chose of their e-mails (ADR-094):
//! linking their account there is their choice, unlinking it ends these
//! messages. Each request told of is audited with its service, never what
//! was written.

mod slack;
mod teams;
mod telegram;

use chrono::{DateTime, Duration, Utc};
use mimorya_core::gateways::{GatewayKind, ShownAction};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::gateway_approval_notices::{
    GatewayApprovalNotice, RunToNotify, gateway_approval_notices, gateway_approvals_to_notify,
    record_gateway_approval_notice,
};
use mimorya_db::repositories::gateway_identities::gateway_identity_of;
use mimorya_db::repositories::gateway_replies::ReplyRecord;
use mimorya_db::repositories::gateways::{GatewayRow, get_gateway};
use serde_json::json;
use uuid::Uuid;

use super::approver_text::ApproverNotice;
use super::{bot_token_name, client_secret_name};
use crate::app::Services;
use crate::services::runs::{Recipient, may_decide_for, recipient};

/// How long after a request it is still told of: one that the service
/// could not be reached for is not, days later.
pub const NOTIFIED_WITHIN: Duration = Duration::hours(24);
/// The runs, each through one gateway, a pass tells of, at most.
const PER_PASS: i32 = 20;
/// The pass's lock (ADR-086).
const NOTICES_SWEEP: &str = "gateway-approval-notices";
/// The audited action of a request told of through a gateway.
pub const APPROVAL_NOTIFIED: &str = "gateway.approval_notified";

/// Tells, through each gateway, the managers whom requests made since
/// `NOTIFIED_WITHIN` await, still to tell of; how many messages were sent.
/// Only in a process that executes runs, and one at a time: none while
/// another tells them.
pub async fn notify_approvers(services: &Services) -> anyhow::Result<usize> {
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
                gateway = %run.gateway_id,
                error = format!("{error:#}"),
                "Gateway approval notices failed"
            ),
        }
    }
    Ok(sent)
}

/// Tells of the requests of `run` awaiting each manager through its
/// gateway; how many messages were sent.
async fn notify_run(
    services: &Services,
    run: RunToNotify,
    since: DateTime<Utc>,
    now: DateTime<Utc>,
) -> anyhow::Result<usize> {
    let tenant_id = run.tenant_id;
    let mut tx = services.db.begin(tenant_id).await?;
    let gateway = get_gateway(&mut tx, run.gateway_id).await?;
    // Paused or removed since it was listed.
    let Some((gateway, kind)) = gateway
        .filter(|g| g.enabled)
        .and_then(|g| GatewayKind::of(&g.kind).map(|kind| (g, kind)))
    else {
        tx.commit().await?;
        return Ok(0);
    };
    let ids = (run.run_id, gateway.id);
    let requests = gateway_approval_notices(&mut tx, ids, since, now).await?;
    let secret = match kind {
        GatewayKind::Teams => client_secret_name(gateway.id),
        GatewayKind::Slack | GatewayKind::Telegram => bot_token_name(gateway.id),
    };
    let token = (services.gateways)
        .secret(&mut tx, tenant_id, &secret)
        .await?;
    tx.commit().await?;
    let mut sent = 0;
    for theirs in requests.chunk_by(|a, b| a.notice.approver_user_id == b.notice.approver_user_id) {
        let to = recipient(&services.db, &services.access, tenant_id, &theirs[0].notice).await?;
        let told = Told {
            tenant_id,
            gateway: &gateway,
            kind,
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
    kind: GatewayKind,
    /// The bot's token (a Teams bot's: its application's client secret),
    /// if it opens.
    token: Option<&'a str>,
    to: &'a Recipient,
}

impl Told<'_> {
    /// What `theirs` are told as, those `shown` with their buttons, the
    /// others to validate in the application at `link`.
    fn notice<'b>(
        &'b self,
        theirs: &'b [GatewayApprovalNotice],
        shown: &'b [ShownAction<'b>],
        link: &'b str,
    ) -> ApproverNotice<'b> {
        ApproverNotice {
            requested_for: &theirs[0].notice.requested_for_name,
            absent: self.to.absent.as_deref(),
            shown,
            in_application: theirs.len() - shown.len(),
            link,
        }
    }
}

/// A request as a message shows it, with its exact parameters.
fn shown_action(request: &GatewayApprovalNotice) -> ShownAction<'_> {
    ShownAction {
        approval_id: request.notice.approval_id,
        summary: &request.action_summary,
        risk: &request.notice.risk,
        parameters: &request.payload,
    }
}

/// Writes `told.to` the requests `theirs`, all awaiting the same manager,
/// through the gateway; what became of the message.
async fn tell(
    services: &Services,
    told: &Told<'_>,
    theirs: &[GatewayApprovalNotice],
    now: DateTime<Utc>,
) -> anyhow::Result<ReplyRecord> {
    let (gateway, to) = (told.gateway, told.to);
    // No master key, or another one: told the operator.
    let Some(token) = told.token else {
        tracing::warn!(gateway = %gateway.id, "Gateway's bot token unreadable");
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
    let reach = Reach {
        token,
        person: &identity.external_id,
        conversation: identity.teams_conversation(),
        attempt,
        now,
    };
    match told.kind {
        GatewayKind::Slack => slack::tell(services, told, reach, theirs).await,
        GatewayKind::Telegram => Ok(telegram::tell(services, told, reach, theirs).await),
        GatewayKind::Teams => Ok(teams::tell(services, told, reach, theirs).await),
    }
}

/// How the person is reached through the gateway, at which attempt.
#[derive(Clone, Copy)]
struct Reach<'a> {
    token: &'a str,
    /// Their identifier in the service, as linked to their account.
    person: &'a str,
    /// A Teams user's personal conversation with the bot, at its service.
    conversation: Option<(&'a str, &'a str)>,
    attempt: i32,
    now: DateTime<Utc>,
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

/// Records what became of the notice of each request of `theirs` through
/// the gateway, and audits each told of, or that could not be.
async fn record_each(
    services: &Services,
    told: &Told<'_>,
    theirs: &[GatewayApprovalNotice],
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
            "gateway": told.kind.as_str(),
            "approverId": notice.approver_user_id,
            "grouped": theirs.len(),
        });
        if to.user_id != notice.approver_user_id {
            metadata["delegateId"] = json!(to.user_id);
        }
        let entry = AuditEntry::new(tenant_id, "system", APPROVAL_NOTIFIED, outcome)
            .resource("approval", notice.approval_id)
            .run(notice.run_id)
            .on_behalf_of(notice.requested_for_user_id)
            .metadata(metadata);
        append_audit(&mut tx, entry).await?;
    }
    tx.commit().await?;
    Ok(())
}
