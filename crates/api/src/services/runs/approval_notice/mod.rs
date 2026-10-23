//! Tells managers by e-mail that requests await their validation
//! (ADR-060), once a run pauses on them: one message per manager and per
//! pause, however many requests the step made. Sent off the run's path: the
//! run is paused whether the message leaves or not; one a stop of the
//! service lost is sent at startup (ADR-070). The day before the requests
//! expire, their manager is reminded once (ADR-062). Without a mail
//! relay, nothing is sent and managers find the requests in « Validations ».
//! While the manager is away, whom they named to stand in for them is told
//! instead, if that colleague may decide the requests (ADR-071). The
//! author of a scheduled task is told of the requests they decide in its
//! run (`author`, ADR-082). Each request notified or reminded is audited,
//! without the address nor the relay's answer, which may name it.

mod author;
mod author_mail;
mod message;
mod recipient;
mod wording;

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use mimorya_db::Database;
use mimorya_db::repositories::approval_notices::{
    ApprovalNotice, NOTIFIED, NoticeRun, REMINDED, approval_notices, approval_reminder_runs,
    approvals_never_notified, approvals_to_remind, unnotified_approval_runs,
};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use serde_json::json;
use uuid::Uuid;

use crate::mail::Mailer;
use crate::services::access::AccessService;
use crate::services::background::BackgroundTasks;
use message::{ApprovalMail, approval_mail};
use recipient::{Recipient, recipient};
use wording::{distinct_risks, sent};

/// How long before their deadline requests are reminded of (ADR-062).
pub const REMIND_BEFORE: Duration = Duration::hours(24);

/// A message about requests: their first notice, or a reminder.
#[derive(Debug, Clone, Copy)]
enum Message {
    Notice,
    Reminder,
}

impl Message {
    /// The audited action, once per request.
    fn action(self) -> &'static str {
        match self {
            Self::Notice => NOTIFIED,
            Self::Reminder => REMINDED,
        }
    }

    /// Named in the logs.
    fn name(self) -> &'static str {
        match self {
            Self::Notice => "notice",
            Self::Reminder => "reminder",
        }
    }
}

#[derive(Clone)]
pub struct ApprovalNotifier {
    db: Database,
    access: Arc<AccessService>,
    mailer: Option<Arc<dyn Mailer>>,
    /// The web application's address, where the link leads.
    web_origin: String,
    product: String,
    background: Arc<BackgroundTasks>,
}

impl ApprovalNotifier {
    pub fn new(
        db: Database,
        access: Arc<AccessService>,
        mailer: Option<Arc<dyn Mailer>>,
        web_origin: String,
        product: String,
        background: Arc<BackgroundTasks>,
    ) -> Self {
        Self {
            db,
            access,
            mailer,
            web_origin,
            product,
            background,
        }
    }

    /// E-mails, in the background, the managers whom the requests
    /// `approval_ids` of the paused run `run_id` await, if they still do,
    /// and the author of the scheduled task that started it, of the ones
    /// they decide.
    pub fn notify(&self, tenant_id: Uuid, run_id: Uuid, approval_ids: Vec<Uuid>) {
        let Some(mailer) = self.mailer.clone() else {
            return;
        };
        let notifier = self.clone();
        self.background.spawn(async move {
            let sent = notifier.send(mailer.as_ref(), tenant_id, run_id, &approval_ids);
            if let Err(error) = sent.await {
                tracing::warn!(error = %error, run_id = %run_id, "Approval notices not sent");
            }
        });
    }

    async fn send(
        &self,
        mailer: &dyn Mailer,
        tenant_id: Uuid,
        run_id: Uuid,
        approval_ids: &[Uuid],
    ) -> anyhow::Result<()> {
        let mut tx = self.db.begin(tenant_id).await?;
        let notices = approval_notices(&mut tx, run_id, approval_ids).await?;
        tx.commit().await?;
        let managers = self
            .send_each(mailer, tenant_id, &notices, Message::Notice)
            .await;
        let author = self
            .tell_author(mailer, tenant_id, run_id, approval_ids)
            .await;
        managers.and(author)
    }

    /// Tells managers, and scheduled tasks' authors (ADR-082), in the
    /// background, of the requests a stop of the service kept from them
    /// (ADR-070); how many. Called at startup, before runs are taken up or
    /// decisions served: the requests are listed now, so that none a run
    /// makes later is told of twice. One list failing leaves the other.
    pub async fn catch_up(&self, now: DateTime<Utc>) -> anyhow::Result<usize> {
        if self.mailer.is_none() {
            return Ok(0);
        }
        let managers = self.catch_up_managers(now).await;
        let authors = self.catch_up_authors(now).await;
        Ok(managers? + authors?)
    }

    /// The managers' requests pending in a paused run, never notified, and
    /// not due within `REMIND_BEFORE` of `now` (those are reminded instead,
    /// a minute later at most).
    async fn catch_up_managers(&self, now: DateTime<Utc>) -> anyhow::Result<usize> {
        let due_after = now + REMIND_BEFORE;
        let mut lost = 0;
        for run in unnotified_approval_runs(self.db.pool(), due_after).await? {
            // One run failing leaves the others to catch up.
            match self.catch_up_run(run, due_after).await {
                Ok(count) => lost += count,
                Err(error) => tracing::warn!(
                    run_id = %run.run_id,
                    error = format!("{error:#}"),
                    "Lost approval notices not caught up"
                ),
            }
        }
        Ok(lost)
    }

    async fn catch_up_run(
        &self,
        run: NoticeRun,
        due_after: DateTime<Utc>,
    ) -> anyhow::Result<usize> {
        let mut tx = self.db.begin(run.tenant_id).await?;
        let requests = approvals_never_notified(&mut tx, run.run_id, due_after).await?;
        tx.commit().await?;
        let ids = requests.iter().map(|request| request.approval_id).collect();
        self.notify(run.tenant_id, run.run_id, ids);
        Ok(requests.len())
    }

    /// Reminds managers of the requests due within `REMIND_BEFORE` of `now`
    /// that they have not been reminded of; how many requests. Nothing
    /// without a mail relay.
    pub async fn remind_due(&self, now: DateTime<Utc>) -> anyhow::Result<usize> {
        let Some(mailer) = self.mailer.clone() else {
            return Ok(0);
        };
        let until = now + REMIND_BEFORE;
        let mut reminded = 0;
        for run in approval_reminder_runs(self.db.pool(), now, until).await? {
            // One run failing leaves the others to remind.
            match self.remind_run(mailer.as_ref(), run, now, until).await {
                Ok(count) => reminded += count,
                Err(error) => tracing::warn!(
                    run_id = %run.run_id,
                    error = format!("{error:#}"),
                    "Approval reminders not sent"
                ),
            }
        }
        Ok(reminded)
    }

    async fn remind_run(
        &self,
        mailer: &dyn Mailer,
        run: NoticeRun,
        now: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> anyhow::Result<usize> {
        let mut tx = self.db.begin(run.tenant_id).await?;
        let due = approvals_to_remind(&mut tx, run.run_id, now, until).await?;
        tx.commit().await?;
        self.send_each(mailer, run.tenant_id, &due, Message::Reminder)
            .await?;
        Ok(due.len())
    }

    /// One `message` per manager: `requests` are ordered by manager.
    async fn send_each(
        &self,
        mailer: &dyn Mailer,
        tenant_id: Uuid,
        requests: &[ApprovalNotice],
        message: Message,
    ) -> anyhow::Result<()> {
        for theirs in requests.chunk_by(|a, b| a.approver_user_id == b.approver_user_id) {
            self.send_one(mailer, tenant_id, theirs, message).await?;
        }
        Ok(())
    }

    /// Tells one manager of `requests`, all theirs, or whom they named to
    /// stand in for them while away (ADR-071), and audits each.
    async fn send_one(
        &self,
        mailer: &dyn Mailer,
        tenant_id: Uuid,
        requests: &[ApprovalNotice],
        message: Message,
    ) -> anyhow::Result<()> {
        let Some(first) = requests.first() else {
            return Ok(());
        };
        let risks = distinct_risks(requests.iter().map(|r| r.risk.as_str()));
        let to = recipient(&self.db, &self.access, tenant_id, first).await?;
        let link = format!("{}/approvals", self.web_origin);
        let mail = approval_mail(&ApprovalMail {
            product: &self.product,
            company: &first.company,
            manager: &to.name,
            email: &to.email,
            absent: to.absent.as_deref(),
            requested_for: &first.requested_for_name,
            count: requests.len(),
            risks: &risks,
            link: &link,
            expires_at: requests.iter().filter_map(|r| r.expires_at).min(),
            reminder: matches!(message, Message::Reminder),
        });
        let outcome = sent(mailer, &mail, first.run_id, message.name()).await;
        let mut tx = self.db.begin(tenant_id).await?;
        for request in requests {
            let grouped = requests.len();
            let entry = audit_entry(tenant_id, request, &to, grouped, message, outcome);
            append_audit(&mut tx, entry).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

/// The notice or reminder of `request`, sent to `to` in one message with
/// `grouped` requests: the stand-in named when the manager was away.
fn audit_entry(
    tenant_id: Uuid,
    request: &ApprovalNotice,
    to: &Recipient,
    grouped: usize,
    message: Message,
    outcome: &'static str,
) -> AuditEntry {
    let mut metadata = json!({ "approverId": request.approver_user_id, "grouped": grouped });
    if to.user_id != request.approver_user_id {
        metadata["delegateId"] = json!(to.user_id);
    }
    AuditEntry::new(tenant_id, "system", message.action(), outcome)
        .resource("approval", request.approval_id)
        .run(request.run_id)
        .on_behalf_of(request.requested_for_user_id)
        .metadata(metadata)
}
