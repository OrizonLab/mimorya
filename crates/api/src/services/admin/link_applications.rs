//! The applications a company's people link their accounts at, declared by
//! its connector administrators (ADR-116, ADR-121): one of each kind per
//! company, under the administration lock, its secret sealed in the secret
//! store (never shown nor audited), the declaration audited. Each kind
//! checks its own declaration (`microsoft365`, `google_workspace`).

use mimorya_connectors::outbound::token_problem;
use mimorya_core::DomainError;
use mimorya_core::account_links::LinkKind;
use mimorya_db::is_unique_violation;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connectors::{
    NewConnectorInstance, create_connector_instance, list_connector_instances,
};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::connectors::{ConnectorAdmin, PERMISSION};
use super::{audit_entry, begin_change};
use crate::services::AccessContext;
use crate::services::connectors::token_secret_name;

/// An application, checked by its kind.
pub(super) struct Declaration {
    pub kind: LinkKind,
    pub name: String,
    pub enabled: bool,
    pub config: Value,
    pub secret: String,
    /// What the audit keeps of it besides its name, type and activation:
    /// its identifiers, never its secret.
    pub audited: Map<String, Value>,
}

pub(super) async fn declare_application(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    declaration: Declaration,
) -> anyhow::Result<Uuid> {
    let product = declaration.kind.product();
    let connector_type = declaration.kind.connector_type();
    // As any connector's secret, written as a bearer token is.
    if let Some(problem) = token_problem(&declaration.secret) {
        return Err(DomainError::validation(problem).into());
    }
    admin.secrets.require()?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(admin.db, access, PERMISSION).await?;
    let declared = list_connector_instances(&mut tx, false)
        .await?
        .iter()
        .any(|instance| instance.connector_type == connector_type);
    if declared {
        return Err(DomainError::conflict(format!(
            "L’application {product} de l’entreprise est déjà déclarée : modifiez-la, ou \
             supprimez-la avant d’en déclarer une autre"
        ))
        .into());
    }
    let created = create_connector_instance(
        &mut tx,
        NewConnectorInstance {
            tenant_id,
            connector_type,
            name: &declaration.name,
            enabled: declaration.enabled,
            config: &declaration.config,
        },
    )
    .await;
    let id = match created {
        Ok(id) => id,
        // The name, or another declaration made meanwhile.
        Err(error) if is_unique_violation(&error) => {
            return Err(DomainError::conflict(format!(
                "Un connecteur porte déjà ce nom, ou l’application {product} est déjà déclarée"
            ))
            .into());
        }
        Err(error) => return Err(error.into()),
    };
    admin
        .secrets
        .put(
            &mut tx,
            tenant_id,
            &token_secret_name(id),
            &declaration.secret,
        )
        .await?;
    let mut metadata = json!({
        "name": declaration.name,
        "type": connector_type,
        "enabled": declaration.enabled,
        "tokenStored": true,
    });
    if let Some(kept) = metadata.as_object_mut() {
        kept.extend(declaration.audited);
    }
    append_audit(
        &mut tx,
        audit_entry(access, "connector.created", "connector", id).metadata(metadata),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}
