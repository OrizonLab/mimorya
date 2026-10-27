//! Accounts each person links at a provider their company declared
//! (ADR-116): their Microsoft 365 mailbox and calendar, reached later by
//! the connector's tools with the permissions they granted themselves. A
//! person who may use the company's connectors starts a link here; the
//! browser signs in at Microsoft and comes back (`finish`); the identity
//! token must name the person's own account; the refresh token is sealed in
//! the link, which the person may remove at any time. The provider's
//! declaration is read at each step, never kept: a link is rare.

mod attempt;
mod finish;
mod grant;
mod start;

pub use attempt::{LINK_COOKIE, cleared_link_cookie, link_cookie};
pub use finish::{LinkFailure, LinkRefused};

use std::sync::Arc;

use mimorya_connectors::outbound::NetworkPolicy;
use mimorya_core::DomainError;
use mimorya_core::account_links::MICROSOFT365;
use mimorya_db::Database;
use mimorya_db::repositories::account_links::{
    AccountLinkRow, delete_account_link, list_account_links_of,
};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::connectors::{ConnectorInstanceRow, list_connector_instances};
use serde_json::json;
use uuid::Uuid;

use crate::secrets::{SecretCipher, SecretStore};
use crate::services::AccessContext;
use crate::services::connectors::CONNECTOR_USE;
use crate::services::sso::http::ProviderHttp;

/// Where the provider sends the browser back, under the web origin.
pub const CALLBACK_PATH: &str = "/api/account/links/callback";

/// A connector the person may link an account at, and their link there.
pub struct Linkable {
    pub connector_id: Uuid,
    pub name: String,
    pub connector_type: String,
    /// Disabled, or no longer theirs to use: their link stays theirs to
    /// remove, but no new one starts.
    pub available: bool,
    pub link: Option<AccountLinkRow>,
}

/// Sent to the provider: its address, and the attempt the browser keeps.
pub struct LinkStart {
    pub redirect_url: String,
    pub attempt: String,
}

pub struct AccountLinkService {
    db: Database,
    cipher: Option<Arc<SecretCipher>>,
    secrets: SecretStore,
    http: ProviderHttp,
    network: NetworkPolicy,
    redirect_uri: String,
}

impl AccountLinkService {
    pub fn new(
        db: Database,
        cipher: Option<Arc<SecretCipher>>,
        network: NetworkPolicy,
        web_origin: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            db,
            secrets: SecretStore::new(cipher.clone()),
            cipher,
            http: ProviderHttp::new(network)?,
            network,
            redirect_uri: format!("{web_origin}{CALLBACK_PATH}"),
        })
    }

    /// The address to register in the company's application.
    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    /// The connectors the person may link an account at, enabled, and
    /// those where they have one, whatever became of the connector.
    pub async fn list(&self, access: &AccessContext) -> anyhow::Result<Vec<Linkable>> {
        let may_use = access.has(CONNECTOR_USE);
        let mut tx = self.db.begin(access.subject.tenant_id).await?;
        let instances = list_connector_instances(&mut tx, false).await?;
        let links = list_account_links_of(&mut tx, access.subject.user_id).await?;
        tx.commit().await?;
        let mut linkables: Vec<Linkable> = instances
            .into_iter()
            .filter(|instance| instance.connector_type == MICROSOFT365)
            .filter_map(|instance| {
                let link = links
                    .iter()
                    .find(|link| link.connector_id == instance.id)
                    .cloned();
                let available = may_use && instance.enabled;
                (available || link.is_some()).then_some(Linkable {
                    connector_id: instance.id,
                    name: instance.name,
                    connector_type: instance.connector_type,
                    available,
                    link,
                })
            })
            .collect();
        linkables.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(linkables)
    }

    /// Removes the person's link at this connector: its refresh token goes
    /// with it. The grant stays at the provider until the person revokes it
    /// there; nothing here can use it any more.
    pub async fn unlink(&self, access: &AccessContext, connector_id: Uuid) -> anyhow::Result<()> {
        let (tenant_id, user_id) = (access.subject.tenant_id, access.subject.user_id);
        let mut tx = self.db.begin(tenant_id).await?;
        if !delete_account_link(&mut tx, connector_id, user_id).await? {
            return Err(DomainError::not_found("Aucun compte relié à ce connecteur").into());
        }
        append_audit(
            &mut tx,
            AuditEntry::new(tenant_id, "user", "account.link.removed", "success")
                .by_user(user_id)
                .resource("connector", connector_id),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

/// The company's Microsoft 365 connector of that id, enabled.
fn linkable(instance: Option<ConnectorInstanceRow>) -> Option<ConnectorInstanceRow> {
    instance.filter(|i| i.connector_type == MICROSOFT365 && i.enabled)
}

fn connector_unknown() -> DomainError {
    DomainError::not_found("Connecteur introuvable")
}

/// What the audit keeps of a link: never its address nor a token.
fn link_metadata(replaced: bool) -> serde_json::Value {
    json!({ "replaced": replaced })
}
