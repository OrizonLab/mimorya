//! Issuing a reset link asked for from the login page, off the response
//! path: the caller gets the same answer, after the same time, whether the
//! address belongs to an account or not. Only an active account that has a
//! password, in an active company, gets a link; the former one stops
//! working. The link is never stored in clear, logged nor audited.

use std::sync::Arc;

use chrono::Utc;
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::auth::find_login;
use mimorya_db::repositories::password_resets::{
    NewPasswordReset, lock_reset_recipient, put_password_reset,
};
use serde_json::json;
use uuid::Uuid;

use super::{RESET_VALIDITY, ResetMail, reset_link, reset_mail, reset_refusal};
use crate::auth::credentials::new_session_token;
use crate::mail::Mailer;

/// What issues a link, owned: it outlives the request.
#[derive(Clone)]
pub struct ResetSender {
    pub db: Database,
    pub mailer: Arc<dyn Mailer>,
    /// The web application's address, where the link leads.
    pub web_origin: String,
    pub product: String,
}

pub struct ResetRequest {
    /// Lower-cased.
    pub email: String,
    pub ip: Option<String>,
    pub request_id: String,
}

/// Issues and sends the link if the address designates an account that may
/// have one. Failures are logged, never with the address or the link.
pub async fn send_requested_link(sender: ResetSender, request: ResetRequest) {
    if let Err(error) = issue(&sender, &request).await {
        tracing::warn!(error = %error, "Password reset link not issued");
    }
}

/// Recorded as the platform's: whoever asked is anonymous, not necessarily
/// the account's holder.
fn audit(tenant_id: Uuid, user_id: Uuid, action: &str, request: &ResetRequest) -> AuditEntry {
    let mut entry = AuditEntry::new(tenant_id, "system", action, "success")
        .resource("user", user_id)
        .ip(request.ip.clone());
    entry.request_id = Some(request.request_id.clone());
    entry
}

async fn issue(sender: &ResetSender, request: &ResetRequest) -> anyhow::Result<()> {
    let Some(login) = find_login(sender.db.pool(), &request.email).await? else {
        return Ok(());
    };
    let (token, token_hash) = new_session_token();
    let expires_at = Utc::now() + RESET_VALIDITY;
    let mut tx = sender.db.begin(login.tenant_id).await?;
    // The account's row locked before its link, as a completion, a password
    // change or a disabling: what is read here holds until the link is stored.
    let Some(recipient) = lock_reset_recipient(&mut tx, login.user_id).await? else {
        return Ok(());
    };
    // Whatever the reason, the caller learns nothing.
    if reset_refusal(&recipient).is_some() {
        return Ok(());
    }
    put_password_reset(
        &mut tx,
        NewPasswordReset {
            tenant_id: login.tenant_id,
            user_id: login.user_id,
            token_hash: &token_hash,
            expires_at,
        },
    )
    .await?;
    append_audit(
        &mut tx,
        audit(
            login.tenant_id,
            login.user_id,
            "user.password_reset_requested",
            request,
        )
        .metadata(json!({ "expiresAt": expires_at })),
    )
    .await?;
    tx.commit().await?;

    let link = reset_link(&sender.web_origin, &token);
    let mail = reset_mail(&ResetMail {
        product: &sender.product,
        company: &recipient.company,
        name: &recipient.display_name,
        email: &recipient.email,
        link: &link,
        expires_at,
        sent_by: None,
    });
    if sender.mailer.send(&mail).await.is_err() {
        // The relay's answer may name the recipient: not logged.
        tracing::warn!(user = %login.user_id, "Password reset e-mail not sent");
        let mut tx = sender.db.begin(login.tenant_id).await?;
        let entry = audit(
            login.tenant_id,
            login.user_id,
            "user.password_reset_not_sent",
            request,
        );
        append_audit(&mut tx, entry).await?;
        tx.commit().await?;
    }
    Ok(())
}
