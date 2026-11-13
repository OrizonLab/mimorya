//! What a company declares once (ADR-116, ADR-121, ADR-205): the
//! applications its people link their accounts at, and its standard mail,
//! one of each kind per company, declared by its connector administrators
//! under the administration lock, an application's secret sealed in the
//! secret store (never shown nor audited), the declaration audited. Each
//! kind checks its own declaration (`microsoft365`, `google_workspace`,
//! `mail_server`).

use mimorya_connectors::outbound::token_problem;
use mimorya_connectors::standard_mail::MAIL_SERVER;
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

/// What is declared once per company.
pub(super) enum Declared {
    /// An application people link their accounts at, with its secret.
    Application(LinkKind),
    /// The standard mail, which holds no secret.
    MailServer,
}

impl Declared {
    fn connector_type(&self) -> &'static str {
        match self {
            Self::Application(kind) => kind.connector_type(),
            Self::MailServer => MAIL_SERVER,
        }
    }

    /// As a sentence says it, in lower case.
    fn said(&self) -> String {
        match self {
            Self::Application(kind) => format!("l’application {}", kind.product()),
            Self::MailServer => "la messagerie standard".into(),
        }
    }
}

/// A declaration, checked by its kind.
pub(super) struct Declaration {
    pub declared: Declared,
    pub name: String,
    pub enabled: bool,
    pub config: Value,
    /// An application's secret; the standard mail has none.
    pub secret: Option<String>,
    /// What the audit keeps of it besides its name, type and activation:
    /// its identifiers or servers, never a secret.
    pub audited: Map<String, Value>,
}

pub(super) async fn declare_once(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    declaration: Declaration,
) -> anyhow::Result<Uuid> {
    let said = declaration.declared.said();
    let connector_type = declaration.declared.connector_type();
    // As any connector's secret, written as a bearer token is.
    if let Some(secret) = &declaration.secret {
        if let Some(problem) = token_problem(secret) {
            return Err(DomainError::validation(problem).into());
        }
        admin.secrets.require()?;
    }
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(admin.db, access, PERMISSION).await?;
    let declared = list_connector_instances(&mut tx, false)
        .await?
        .iter()
        .any(|instance| instance.connector_type == connector_type);
    if declared {
        return Err(DomainError::conflict(format!(
            "{} de l’entreprise est déjà déclarée : modifiez-la, ou supprimez-la avant d’en \
             déclarer une autre",
            capitalized(&said)
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
                "Un connecteur porte déjà ce nom, ou {said} est déjà déclarée"
            ))
            .into());
        }
        Err(error) => return Err(error.into()),
    };
    if let Some(secret) = &declaration.secret {
        admin
            .secrets
            .put(&mut tx, tenant_id, &token_secret_name(id), secret)
            .await?;
    }
    let mut metadata = json!({
        "name": declaration.name,
        "type": connector_type,
        "enabled": declaration.enabled,
        "tokenStored": declaration.secret.is_some(),
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

/// `text` with its first letter in upper case.
fn capitalized(text: &str) -> String {
    let mut letters = text.chars();
    letters
        .next()
        .map(|first| first.to_uppercase().chain(letters).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_what_is_declared_once() {
        let microsoft = Declared::Application(LinkKind::Microsoft365);
        assert_eq!(
            capitalized(&microsoft.said()),
            "L’application Microsoft 365"
        );
        assert_eq!(
            capitalized(&Declared::MailServer.said()),
            "La messagerie standard"
        );
        assert_eq!(Declared::MailServer.connector_type(), MAIL_SERVER);
    }
}
