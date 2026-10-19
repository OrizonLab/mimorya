//! Reset links sent by account administrators (ADR-029): instead of a
//! temporary password, the account receives a one-time link by e-mail,
//! valid 24 hours, and chooses its new password there. The administrator
//! never sees the link: without a relay, or when sending fails, nothing is
//! left behind and the temporary password remains the way out. The current
//! password works until the link is used.

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::password_resets::{
    NewPasswordReset, delete_password_reset, lock_reset_recipient, put_password_reset,
};
use serde_json::json;
use uuid::Uuid;

use super::{LinkSender, audit_entry, begin_change, refuse_self};
use crate::auth::credentials::new_session_token;
use crate::services::AccessContext;
use crate::services::password_reset::{ResetMail, SENT_RESET_VALIDITY, reset_link, reset_mail};

const PERMISSION: &str = "user:manage";
const NO_RELAY: &str =
    "Aucun serveur d’e-mail n’est configuré : donnez plutôt un mot de passe provisoire";

/// Sends the account a reset link; returns when the link expires.
pub async fn send_reset_link(
    sender: &LinkSender<'_>,
    access: &AccessContext,
    user_id: Uuid,
) -> anyhow::Result<DateTime<Utc>> {
    access.require_tenant_wide(PERMISSION)?;
    refuse_self(access, user_id)?;
    let Some(mailer) = sender.mailer else {
        return Err(DomainError::unavailable(NO_RELAY).into());
    };
    let (token, token_hash) = new_session_token();
    let expires_at = Utc::now() + SENT_RESET_VALIDITY;
    let mut tx = begin_change(sender.db, access, PERMISSION).await?;
    // The account's row locked before its link, as a use of the link, a
    // password change or a disabling.
    let Some(recipient) = lock_reset_recipient(&mut tx, user_id).await? else {
        return Err(DomainError::not_found("Compte introuvable").into());
    };
    match (recipient.status.as_str(), recipient.has_password) {
        ("active", true) => {}
        ("invited", _) => {
            return Err(DomainError::conflict(
                "Ce compte n’a pas encore choisi son mot de passe : renvoyez-lui plutôt son \
                 invitation",
            )
            .into());
        }
        ("active", false) => {
            return Err(
                DomainError::conflict("Ce compte n’a pas de mot de passe à réinitialiser").into(),
            );
        }
        _ => {
            return Err(
                DomainError::conflict("Ce compte est désactivé : réactivez-le d’abord").into(),
            );
        }
    }
    put_password_reset(
        &mut tx,
        NewPasswordReset {
            tenant_id: access.subject.tenant_id,
            user_id,
            token_hash: &token_hash,
            expires_at,
        },
    )
    .await?;
    append_audit(
        &mut tx,
        audit_entry(access, "user.password_reset_link_sent", "user", user_id)
            .metadata(json!({ "expiresAt": expires_at })),
    )
    .await?;
    tx.commit().await?;

    let link = reset_link(sender.web_origin, &token);
    let mail = reset_mail(&ResetMail {
        product: sender.product,
        company: &recipient.company,
        name: &recipient.display_name,
        email: &recipient.email,
        link: &link,
        expires_at,
        sent_by: Some(&access.profile.display_name),
    });
    if let Err(error) = mailer.send(&mail).await {
        // The relay's answer may name the recipient: kept for the
        // administrator, not for the logs. The link nobody received goes.
        tracing::warn!(user = %user_id, "Password reset e-mail not sent");
        let mut tx = sender.db.begin(access.subject.tenant_id).await?;
        delete_password_reset(&mut tx, &token_hash).await?;
        append_audit(
            &mut tx,
            audit_entry(access, "user.password_reset_not_sent", "user", user_id),
        )
        .await?;
        tx.commit().await?;
        return Err(DomainError::unavailable(format!(
            "L’e-mail n’a pas pu être envoyé ({error}) : donnez plutôt un mot de passe provisoire"
        ))
        .into());
    }
    Ok(expires_at)
}
