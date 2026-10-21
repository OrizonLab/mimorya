//! Tells managers by e-mail that requests await their validation
//! (ADR-060), once a run pauses on them: one message per manager and per
//! pause, however many requests the step made. Sent off the run's path: the
//! run is paused whether the message leaves or not. Without a mail relay,
//! nothing is sent and managers find the requests in « Validations ». Each
//! request notified is audited, without the address nor the relay's answer,
//! which may name it; a request is notified once.

mod message;

use std::sync::Arc;

use mimorya_db::Database;
use mimorya_db::repositories::approval_notices::{ApprovalNotice, approval_notices};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use serde_json::json;
use uuid::Uuid;

use crate::mail::Mailer;
use crate::services::background::BackgroundTasks;
use message::{ApprovalMail, approval_mail};

#[derive(Clone)]
pub struct ApprovalNotifier {
    db: Database,
    mailer: Option<Arc<dyn Mailer>>,
    /// The web application's address, where the link leads.
    web_origin: String,
    product: String,
    background: Arc<BackgroundTasks>,
}

impl ApprovalNotifier {
    pub fn new(
        db: Database,
        mailer: Option<Arc<dyn Mailer>>,
        web_origin: String,
        product: String,
        background: Arc<BackgroundTasks>,
    ) -> Self {
        Self {
            db,
            mailer,
            web_origin,
            product,
            background,
        }
    }

    /// E-mails, in the background, the managers whom the requests
    /// `approval_ids` of the paused run `run_id` await, if they still do.
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
        // Ordered by manager: one message each.
        for requests in notices.chunk_by(|a, b| a.approver_user_id == b.approver_user_id) {
            self.send_one(mailer, tenant_id, requests).await?;
        }
        Ok(())
    }

    /// Tells one manager of `requests`, all theirs, and audits each.
    async fn send_one(
        &self,
        mailer: &dyn Mailer,
        tenant_id: Uuid,
        requests: &[ApprovalNotice],
    ) -> anyhow::Result<()> {
        let Some(first) = requests.first() else {
            return Ok(());
        };
        let mut risks: Vec<&str> = Vec::new();
        for request in requests {
            if !risks.contains(&request.risk.as_str()) {
                risks.push(&request.risk);
            }
        }
        let link = format!("{}/approvals", self.web_origin);
        let mail = approval_mail(&ApprovalMail {
            product: &self.product,
            company: &first.company,
            manager: &first.approver_name,
            email: &first.approver_email,
            requested_for: &first.requested_for_name,
            count: requests.len(),
            risks: &risks,
            link: &link,
            expires_at: requests.iter().filter_map(|r| r.expires_at).min(),
        });
        let outcome = match mailer.send(&mail).await {
            Ok(()) => "success",
            Err(_) => {
                tracing::warn!(run_id = %first.run_id, "Approval notice refused by the relay");
                "failure"
            }
        };
        let mut tx = self.db.begin(tenant_id).await?;
        for request in requests {
            let entry = audit_entry(tenant_id, request, requests.len(), outcome);
            append_audit(&mut tx, entry).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

/// The notice of `request`, sent in one message with `grouped` requests.
fn audit_entry(
    tenant_id: Uuid,
    request: &ApprovalNotice,
    grouped: usize,
    outcome: &'static str,
) -> AuditEntry {
    AuditEntry::new(tenant_id, "system", "approval.notified", outcome)
        .resource("approval", request.approval_id)
        .run(request.run_id)
        .on_behalf_of(request.requested_for_user_id)
        .metadata(json!({ "approverId": request.approver_user_id, "grouped": grouped }))
}
