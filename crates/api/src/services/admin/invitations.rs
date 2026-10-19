//! Invitation links issued by account administrators (ADR-027): an account
//! that never chose its password gets a one-time link, valid for seven days,
//! which replaces any former one. It is e-mailed when a relay is configured;
//! otherwise, or when sending fails, it is shown once to the administrator,
//! who transmits it. The link is never stored in clear, logged nor audited.

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::users::get_user;
use serde_json::json;
use uuid::Uuid;

use super::{LinkSender, audit_entry, begin_change};
use crate::services::AccessContext;
use crate::services::invitations::{
    InvitationMail, RenewedInvitation, invitation_link, invitation_mail, renew_invitation,
};

const PERMISSION: &str = "user:manage";
const NO_RELAY: &str = "Aucun serveur d’e-mail n’est configuré : transmettez ce lien vous-même";

pub struct IssuedInvitation {
    pub expires_at: DateTime<Utc>,
    pub emailed: bool,
    /// For the administrator to transmit, when no e-mail went out.
    pub link: Option<String>,
    /// Why no e-mail went out.
    pub problem: Option<String>,
}

pub async fn issue_invitation(
    sender: &LinkSender<'_>,
    access: &AccessContext,
    user_id: Uuid,
) -> anyhow::Result<IssuedInvitation> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = begin_change(sender.db, access, PERMISSION).await?;
    let RenewedInvitation { token, expires_at } =
        renew_invitation(&mut tx, user_id, Some(access.subject.user_id)).await?;
    let Some(user) = get_user(&mut tx, user_id).await? else {
        return Err(DomainError::not_found("Compte introuvable").into());
    };
    let delivery = if sender.mailer.is_some() {
        "email"
    } else {
        "link"
    };
    append_audit(
        &mut tx,
        audit_entry(access, "user.invited", "user", user_id)
            .metadata(json!({ "expiresAt": expires_at, "delivery": delivery })),
    )
    .await?;
    tx.commit().await?;

    let link = invitation_link(sender.web_origin, &token);
    let not_sent = |problem: String| IssuedInvitation {
        expires_at,
        emailed: false,
        link: Some(link.clone()),
        problem: Some(problem),
    };
    let Some(mailer) = sender.mailer else {
        return Ok(not_sent(NO_RELAY.into()));
    };
    let mail = invitation_mail(&InvitationMail {
        product: sender.product,
        company: &access.tenant.name,
        inviter: &access.profile.display_name,
        invited_name: &user.display_name,
        invited_email: &user.email,
        link: &link,
        expires_at,
    });
    match mailer.send(&mail).await {
        Ok(()) => Ok(IssuedInvitation {
            expires_at,
            emailed: true,
            link: None,
            problem: None,
        }),
        Err(error) => {
            // The relay's answer may name the recipient: kept for the
            // administrator, not for the logs.
            tracing::warn!(user = %user_id, "Invitation e-mail not sent");
            let mut tx = sender.db.begin(access.subject.tenant_id).await?;
            append_audit(
                &mut tx,
                audit_entry(access, "user.invitation_not_sent", "user", user_id),
            )
            .await?;
            tx.commit().await?;
            Ok(not_sent(format!(
                "L’e-mail n’a pas pu être envoyé : transmettez ce lien vous-même ({error})"
            )))
        }
    }
}
