//! Invitation links issued by account administrators (ADR-027): an account
//! that never chose its password gets a one-time link, valid for seven days,
//! which replaces any former one. It is e-mailed when a relay is configured;
//! otherwise, or when sending fails, it is shown once to the administrator,
//! who transmits it. The link is never stored in clear, logged nor audited.

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::auth::lock_login_state;
use mimorya_db::repositories::invitations::{NewInvitation, put_invitation};
use mimorya_db::repositories::users::get_user;
use serde_json::json;
use uuid::Uuid;

use super::{audit_entry, begin_change};
use crate::auth::credentials::new_session_token;
use crate::mail::Mailer;
use crate::services::AccessContext;
use crate::services::invitations::{
    INVITATION_VALIDITY, InvitationMail, invitation_link, invitation_mail,
};

const PERMISSION: &str = "user:manage";
const NO_RELAY: &str = "Aucun serveur d’e-mail n’est configuré : transmettez ce lien vous-même";

/// What sends a one-time link (an invitation, a reset link) besides the
/// database.
pub struct LinkSender<'a> {
    pub db: &'a Database,
    pub mailer: Option<&'a dyn Mailer>,
    /// The web application's address, where the link leads.
    pub web_origin: &'a str,
    pub product: &'a str,
}

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
    let (token, token_hash) = new_session_token();
    let expires_at = Utc::now() + INVITATION_VALIDITY;
    let mut tx = begin_change(sender.db, access, PERMISSION).await?;
    // The account's row locked before its invitation, as an acceptance does:
    // the status read here holds until the new link is stored.
    let Some((_, status)) = lock_login_state(&mut tx, user_id).await? else {
        return Err(DomainError::not_found("Compte introuvable").into());
    };
    let Some(user) = get_user(&mut tx, user_id).await? else {
        return Err(DomainError::not_found("Compte introuvable").into());
    };
    if status != "invited" {
        return Err(DomainError::conflict(
            "Ce compte a déjà choisi son mot de passe ou est désactivé : pas d’invitation",
        )
        .into());
    }
    put_invitation(
        &mut tx,
        NewInvitation {
            tenant_id: access.subject.tenant_id,
            user_id,
            token_hash: &token_hash,
            created_by_user_id: access.subject.user_id,
            expires_at,
        },
    )
    .await?;
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
