//! The account a run's person linked at their company's standard mail, as
//! its tools reach it (ADR-205): opened for one connection, with the
//! declaration read in the same transaction, the servers its password was
//! checked against, never another (a host changed removes the accounts in
//! the transaction of the change). Removed once the company's server
//! refuses it, unless linked again since: a password changed elsewhere is
//! not tried again by every run, which could get the company's address
//! banned by its server.

use chrono::{DateTime, Utc};
use mimorya_core::tools::{AccountAccessError, MailAccount, MailServerRole};
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::mail_accounts::{delete_refused_mail_account, get_mail_account};
use mimorya_db::repositories::users::get_user;
use serde_json::json;
use uuid::Uuid;

use super::{mail_server_of, sealed_name};
use crate::secrets::SecretCipher;

/// The account removed once the company's server refused it.
pub const MAIL_ACCOUNT_REFUSED: &str = "mail_account.refused_by_server";

/// An internal failure, logged; the model is told only that the mail is
/// out of reach for now.
fn internal(error: impl std::fmt::Display) -> AccountAccessError {
    tracing::error!(error = %error, "Mail account unreachable");
    AccountAccessError::Unavailable(
        "Accès à la messagerie impossible pour l’instant (erreur interne)".into(),
    )
}

/// The account `user_id` linked at their company's enabled standard mail,
/// its password opened.
pub async fn opened_mail_account(
    db: &Database,
    cipher: Option<&SecretCipher>,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<MailAccount, AccountAccessError> {
    let cipher = cipher.ok_or_else(|| {
        AccountAccessError::Unavailable(
            "Le serveur n’a pas de clé maîtresse : aucun compte de messagerie ne s’ouvre".into(),
        )
    })?;
    let mut tx = db.begin(tenant_id).await.map_err(internal)?;
    let instance = mail_server_of(&mut tx)
        .await
        .map_err(internal)?
        .filter(|instance| instance.enabled)
        .ok_or_else(|| {
            AccountAccessError::Unavailable(
                "La messagerie standard de l’entreprise n’est pas active".into(),
            )
        })?;
    let account = get_mail_account(&mut tx, instance.id, user_id)
        .await
        .map_err(internal)?;
    // What the person sends from: their account's address.
    let person = get_user(&mut tx, user_id).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    let account = account.ok_or(AccountAccessError::NotLinked)?;
    let address = person.ok_or(AccountAccessError::NotLinked)?.email;
    let password = cipher
        .open(
            tenant_id,
            &sealed_name(instance.id, user_id),
            &account.password,
        )
        .map_err(internal)?;
    Ok(MailAccount::new(
        instance.config,
        address,
        account.username,
        password,
        account.linked_at,
    ))
}

/// Removes, audited, the account `user_id` linked on `linked_at`, which
/// the company's `server` refused; one linked again since stays.
pub async fn refused_mail_account(
    db: &Database,
    tenant_id: Uuid,
    user_id: Uuid,
    linked_at: DateTime<Utc>,
    server: MailServerRole,
) -> anyhow::Result<bool> {
    let mut tx = db.begin(tenant_id).await?;
    let Some(instance) = mail_server_of(&mut tx).await? else {
        tx.commit().await?;
        return Ok(false);
    };
    let removed = delete_refused_mail_account(&mut tx, instance.id, user_id, linked_at).await?;
    if removed {
        append_audit(
            &mut tx,
            AuditEntry::new(tenant_id, "system", MAIL_ACCOUNT_REFUSED, "success")
                .on_behalf_of(user_id)
                .resource("connector", instance.id)
                .metadata(json!({ "server": server.as_str() })),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(removed)
}
