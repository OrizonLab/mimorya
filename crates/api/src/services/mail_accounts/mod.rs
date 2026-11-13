//! The account each person links at their company's standard mail
//! (ADR-205): what they see of it in their account (`mail_account_of`),
//! linking it once the company's servers accepted it (`link`), and removing
//! it. Its password is sealed by the server's master key for this company,
//! connector and person alone, never shown nor written to the audit.

mod link;

pub use link::{MailLinking, link_mail_account};

use chrono::{DateTime, Utc};
use mimorya_connectors::standard_mail::{MAIL_SERVER, MailServerConfig};
use mimorya_core::DomainError;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::connectors::{ConnectorInstanceRow, list_connector_instances};
use mimorya_db::repositories::mail_accounts::{delete_mail_account, get_mail_account};
use mimorya_db::{Database, TenantTx};
use uuid::Uuid;

use crate::services::AccessContext;
use crate::services::connectors::CONNECTOR_USE;

/// The company's standard mail as a person sees it, with their account.
pub struct MailAccountSeen {
    pub connector_id: Uuid,
    pub name: String,
    /// Enabled, readable, and the person may use connectors: they may
    /// link an account.
    pub available: bool,
    /// The hosts the person's password would go to.
    pub hosts: Vec<String>,
    pub linked: Option<(String, DateTime<Utc>)>,
}

/// The company's standard mail, if declared, whatever its state.
async fn mail_server_of(tx: &mut TenantTx) -> anyhow::Result<Option<ConnectorInstanceRow>> {
    Ok(list_connector_instances(tx, false)
        .await?
        .into_iter()
        .find(|instance| instance.connector_type == MAIL_SERVER))
}

fn no_mail_server() -> DomainError {
    DomainError::not_found("Aucune messagerie standard n’est déclarée par votre entreprise")
}

/// The company's standard mail and the person's account there: `None`
/// when there is none they may link an account at, nor one they linked.
pub async fn mail_account_of(
    db: &Database,
    access: &AccessContext,
) -> anyhow::Result<Option<MailAccountSeen>> {
    let mut tx = db.begin(access.subject.tenant_id).await?;
    let Some(instance) = mail_server_of(&mut tx).await? else {
        tx.commit().await?;
        return Ok(None);
    };
    let account = get_mail_account(&mut tx, instance.id, access.subject.user_id).await?;
    tx.commit().await?;
    let config = MailServerConfig::read(&instance.config).ok();
    let available = instance.enabled && config.is_some() && access.has(CONNECTOR_USE);
    if !available && account.is_none() {
        return Ok(None);
    }
    Ok(Some(MailAccountSeen {
        connector_id: instance.id,
        name: instance.name,
        available,
        hosts: config.map(|config| config.hosts()).unwrap_or_default(),
        linked: account.map(|account| (account.username, account.linked_at)),
    }))
}

/// Removes the person's account at their company's standard mail: its
/// password goes with it, nothing here can use it any more.
pub async fn unlink_mail_account(db: &Database, access: &AccessContext) -> anyhow::Result<()> {
    let (tenant_id, user_id) = (access.subject.tenant_id, access.subject.user_id);
    let mut tx = db.begin(tenant_id).await?;
    let instance = mail_server_of(&mut tx).await?.ok_or_else(no_mail_server)?;
    if !delete_mail_account(&mut tx, instance.id, user_id).await? {
        return Err(DomainError::not_found("Aucun compte de messagerie relié").into());
    }
    append_audit(
        &mut tx,
        AuditEntry::new(tenant_id, "user", "mail_account.removed", "success")
            .by_user(user_id)
            .resource("connector", instance.id),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
