//! Linking a person's account at their company's standard mail (ADR-205):
//! prepared without reaching anything (a master key to seal it, the
//! permission to use connectors, the company's mail enabled, an identifier
//! and a password of a valid form), then the company's IMAP server and its
//! SMTP server each signed in to, over TLS, before anything is kept. A
//! refused account is never kept. The password is sealed for this company,
//! connector and person alone, and kept only while the servers it was
//! checked against are still the company's: a change made meanwhile is
//! refused, never followed.

use mimorya_connectors::outbound::{Cause, NetworkPolicy};
use mimorya_connectors::standard_mail::{
    CREDENTIALS_REFUSED, FORM_REFUSED, MAIL_SERVER, MailLogin, MailServerConfig, check_imap,
    check_smtp,
};
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::mail_accounts::{lock_mail_connector, put_mail_account};
use serde_json::json;
use uuid::Uuid;

use super::{mail_server_of, no_mail_server};
use crate::secrets::SecretStore;
use crate::services::AccessContext;
use crate::services::connectors::CONNECTOR_USE;

/// What linking an account needs.
pub struct MailLinking<'a> {
    pub db: &'a Database,
    pub secrets: &'a SecretStore,
    pub network: NetworkPolicy,
}

/// An account ready to be tried at the company's servers.
pub struct PreparedLink {
    tenant_id: Uuid,
    user_id: Uuid,
    connector_id: Uuid,
    config: MailServerConfig,
    login: MailLogin,
}

/// The name a password is sealed under: it opens for this company,
/// connector and person only.
fn sealed_name(connector_id: Uuid, user_id: Uuid) -> String {
    format!("mail-account/{connector_id}/{user_id}")
}

impl MailLinking<'_> {
    /// Checks all that needs no server; nothing is reached yet.
    pub async fn prepare(
        &self,
        access: &AccessContext,
        username: &str,
        password: &str,
    ) -> anyhow::Result<PreparedLink> {
        self.secrets.require()?;
        access.require(CONNECTOR_USE)?;
        let (tenant_id, user_id) = (access.subject.tenant_id, access.subject.user_id);
        let mut tx = self.db.begin(tenant_id).await?;
        let instance = mail_server_of(&mut tx).await?;
        tx.commit().await?;
        let instance = instance
            .filter(|instance| instance.enabled)
            .ok_or_else(no_mail_server)?;
        let config = MailServerConfig::read(&instance.config)
            .map_err(|error| DomainError::unavailable(error.to_string()))?;
        let login = MailLogin::new(username, password).map_err(DomainError::validation)?;
        Ok(PreparedLink {
            tenant_id,
            user_id,
            connector_id: instance.id,
            config,
            login,
        })
    }
}

/// Signs `link` in to the company's IMAP then SMTP server and keeps the
/// account once both accepted it.
pub async fn link_mail_account(
    linking: &MailLinking<'_>,
    link: PreparedLink,
) -> anyhow::Result<()> {
    let PreparedLink {
        tenant_id,
        user_id,
        connector_id,
        config,
        login,
    } = link;
    let checked = match check_imap(&config.imap, &login, linking.network).await {
        Ok(()) => check_smtp(&config.smtp, &login, linking.network)
            .await
            .map_err(|cause| ("smtp", cause)),
        Err(cause) => Err(("imap", cause)),
    };
    if let Err((server, cause)) = checked {
        let refused = AuditEntry::new(tenant_id, "user", "mail_account.refused", "denied")
            .by_user(user_id)
            .resource("connector", connector_id)
            .reason(cause.summary)
            .metadata(json!({ "server": server }));
        let mut tx = linking.db.begin(tenant_id).await?;
        append_audit(&mut tx, refused).await?;
        tx.commit().await?;
        return Err(refusal(server, &cause).into());
    }
    let cipher = linking.secrets.cipher()?;
    let name = sealed_name(connector_id, user_id);
    let sealed = login.sealed_with(|password| cipher.seal(tenant_id, &name, password))?;
    let mut tx = linking.db.begin(tenant_id).await?;
    // The servers it was checked against, still the company's.
    let current = lock_mail_connector(&mut tx, connector_id, MAIL_SERVER).await?;
    let unchanged = current.is_some_and(|(enabled, stored)| {
        enabled && MailServerConfig::read(&stored).is_ok_and(|now| now.hosts() == config.hosts())
    });
    if !unchanged {
        return Err(DomainError::conflict(
            "La messagerie de l’entreprise a changé pendant la vérification : reliez votre \
             compte de nouveau",
        )
        .into());
    }
    let replaced = put_mail_account(
        &mut tx,
        tenant_id,
        connector_id,
        user_id,
        login.username(),
        &sealed,
    )
    .await?;
    append_audit(
        &mut tx,
        AuditEntry::new(tenant_id, "user", "mail_account.linked", "success")
            .by_user(user_id)
            .resource("connector", connector_id)
            .metadata(json!({ "replaced": replaced })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// What the person is told of a refusal, naming the server: their account
/// refused is theirs to correct, anything else the server's.
fn refusal(server: &str, cause: &Cause) -> DomainError {
    let said = format!("Serveur {} : {}", server.to_uppercase(), cause.summary);
    if [CREDENTIALS_REFUSED, FORM_REFUSED].contains(&cause.summary) {
        DomainError::validation(said)
    } else {
        DomainError::unavailable(said)
    }
}

#[cfg(test)]
mod tests {
    use mimorya_core::ErrorCode;

    use super::*;
    use crate::secrets::SecretCipher;

    #[test]
    fn a_password_opens_for_its_company_connector_and_person_only() {
        let cipher = SecretCipher::new(&[9; 32]).unwrap();
        let (tenant, connector, user) =
            (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let sealed = cipher
            .seal(tenant, &sealed_name(connector, user), "secret-du-test")
            .unwrap();
        let open =
            |tenant, connector, user| cipher.open(tenant, &sealed_name(connector, user), &sealed);
        assert_eq!(open(tenant, connector, user).unwrap(), "secret-du-test");
        let other = Uuid::from_u128(4);
        assert!(open(other, connector, user).is_err());
        assert!(open(tenant, other, user).is_err());
        assert!(open(tenant, connector, other).is_err());
    }

    #[test]
    fn a_refused_account_is_the_person_s_to_correct() {
        let cause = |summary| Cause {
            summary,
            detail: String::new(),
        };
        let refused = refusal("imap", &cause(CREDENTIALS_REFUSED));
        assert_eq!(refused.code, ErrorCode::Validation);
        assert!(
            refused.message.starts_with("Serveur IMAP : "),
            "{refused:?}"
        );
        let silent = refusal("smtp", &cause(mimorya_connectors::outbound::TOO_SLOW));
        assert_eq!(silent.code, ErrorCode::Unavailable);
        assert!(silent.message.starts_with("Serveur SMTP : "), "{silent:?}");
    }
}
