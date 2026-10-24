//! Tells a scheduled task's author by e-mail that its run awaits their
//! validation (ADR-082): it started while nobody watched (ADR-080), and
//! would otherwise wait for them unseen until its requests expire. Once per
//! pause, of the requests they decide themselves (those routed to a manager
//! are told to that manager, ADR-060); one a stop of the service lost is
//! sent by the periodic catch-up (ADR-070, ADR-086). Each request notified is audited as a
//! manager's notice is, without the address nor the relay's answer.

use chrono::{DateTime, Utc};
use mimorya_db::repositories::approval_notices::{NOTIFIED, NoticeRun};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::scheduled_run_notices::{
    AuthorNotice, author_notices, author_notices_never_sent, unnotified_scheduled_runs,
};
use serde_json::json;
use uuid::Uuid;

use super::ApprovalNotifier;
use super::author_mail::{AuthorMail, author_mail};
use super::wording::{distinct_risks, sent};
use crate::mail::Mailer;

impl ApprovalNotifier {
    /// Tells the author of the scheduled task that started `run_id` of the
    /// requests `approval_ids` they decide, if they still await them.
    pub(super) async fn tell_author(
        &self,
        mailer: &dyn Mailer,
        tenant_id: Uuid,
        run_id: Uuid,
        approval_ids: &[Uuid],
    ) -> anyhow::Result<()> {
        let mut tx = self.db.begin(tenant_id).await?;
        let requests = author_notices(&mut tx, run_id, approval_ids).await?;
        tx.commit().await?;
        self.send_to_author(mailer, tenant_id, &requests).await
    }

    /// Tells authors of the requests a stop of the service kept from them,
    /// their run paused by `lost_before`, not expired by `now`, whatever
    /// their deadline (authors have no reminder); how many. With the
    /// managers' catch-up.
    pub(super) async fn catch_up_authors(
        &self,
        mailer: &dyn Mailer,
        now: DateTime<Utc>,
        lost_before: DateTime<Utc>,
    ) -> anyhow::Result<usize> {
        let mut lost = 0;
        for run in unnotified_scheduled_runs(self.db.pool(), now).await? {
            // One run failing leaves the others to catch up.
            match self.catch_up_author(mailer, run, now, lost_before).await {
                Ok(count) => lost += count,
                Err(error) => tracing::warn!(
                    run_id = %run.run_id,
                    error = format!("{error:#}"),
                    "Lost notice to a scheduled task's author not caught up"
                ),
            }
        }
        Ok(lost)
    }

    async fn catch_up_author(
        &self,
        mailer: &dyn Mailer,
        run: NoticeRun,
        now: DateTime<Utc>,
        lost_before: DateTime<Utc>,
    ) -> anyhow::Result<usize> {
        let mut tx = self.db.begin(run.tenant_id).await?;
        let requests = author_notices_never_sent(&mut tx, run.run_id, now, lost_before).await?;
        tx.commit().await?;
        self.send_to_author(mailer, run.tenant_id, &requests)
            .await?;
        Ok(requests.len())
    }

    /// One message for `requests`, all of one run, then each audited.
    async fn send_to_author(
        &self,
        mailer: &dyn Mailer,
        tenant_id: Uuid,
        requests: &[AuthorNotice],
    ) -> anyhow::Result<()> {
        let Some(first) = requests.first() else {
            return Ok(());
        };
        let risks = distinct_risks(requests.iter().map(|r| r.risk.as_str()));
        let link = match first.conversation_id {
            Some(conversation) => format!("{}/chat/{conversation}", self.web_origin),
            None => format!("{}/approvals", self.web_origin),
        };
        let mail = author_mail(&AuthorMail {
            product: &self.product,
            company: &first.company,
            author: &first.author_name,
            email: &first.author_email,
            task: &first.task_title,
            count: requests.len(),
            risks: &risks,
            link: &link,
            expires_at: requests.iter().filter_map(|r| r.expires_at).min(),
        });
        let outcome = sent(mailer, &mail, first.run_id, "author notice").await;
        let mut tx = self.db.begin(tenant_id).await?;
        for request in requests {
            let metadata =
                json!({ "approverId": request.author_user_id, "grouped": requests.len() });
            let entry = AuditEntry::new(tenant_id, "system", NOTIFIED, outcome)
                .resource("approval", request.approval_id)
                .run(request.run_id)
                .on_behalf_of(request.author_user_id)
                .metadata(metadata);
            append_audit(&mut tx, entry).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}
