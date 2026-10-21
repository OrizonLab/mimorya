//! Tells a manager by e-mail that a request awaits their validation
//! (ADR-060), off the run's path: the run pauses whether the message leaves
//! or not. Without a mail relay, nothing is sent and the manager finds the
//! request in « Validations ». Each attempt is audited, without the address
//! nor the relay's answer, which may name it.

mod message;

use std::sync::Arc;

use mimorya_db::Database;
use mimorya_db::repositories::approval_notices::{ApprovalNotice, approval_notice};
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

    /// E-mails, in the background, the manager the request `approval_id`
    /// awaits, if it still awaits them.
    pub fn notify(&self, tenant_id: Uuid, approval_id: Uuid) {
        let Some(mailer) = self.mailer.clone() else {
            return;
        };
        let notifier = self.clone();
        self.background.spawn(async move {
            if let Err(error) = notifier.send(mailer.as_ref(), tenant_id, approval_id).await {
                tracing::warn!(error = %error, approval = %approval_id, "Approval notice not sent");
            }
        });
    }

    async fn send(
        &self,
        mailer: &dyn Mailer,
        tenant_id: Uuid,
        approval_id: Uuid,
    ) -> anyhow::Result<()> {
        let mut tx = self.db.begin(tenant_id).await?;
        let notice = approval_notice(&mut tx, approval_id).await?;
        tx.commit().await?;
        let Some(notice) = notice else {
            return Ok(());
        };
        let link = format!("{}/approvals", self.web_origin);
        let mail = approval_mail(&ApprovalMail {
            product: &self.product,
            company: &notice.company,
            manager: &notice.approver_name,
            email: &notice.approver_email,
            requested_for: &notice.requested_for_name,
            risk: &notice.risk,
            link: &link,
            expires_at: notice.expires_at,
        });
        let outcome = match mailer.send(&mail).await {
            Ok(()) => "success",
            Err(_) => {
                tracing::warn!(approval = %approval_id, "Approval notice refused by the relay");
                "failure"
            }
        };
        let mut tx = self.db.begin(tenant_id).await?;
        append_audit(
            &mut tx,
            audit_entry(tenant_id, approval_id, &notice, outcome),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

fn audit_entry(
    tenant_id: Uuid,
    approval_id: Uuid,
    notice: &ApprovalNotice,
    outcome: &'static str,
) -> AuditEntry {
    AuditEntry::new(tenant_id, "system", "approval.notified", outcome)
        .resource("approval", approval_id)
        .run(notice.run_id)
        .on_behalf_of(notice.requested_for_user_id)
        .metadata(json!({ "approverId": notice.approver_user_id }))
}
