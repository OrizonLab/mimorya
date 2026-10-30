//! The company's Microsoft Teams bot, declared by its connector
//! administrators (ADR-138), one per company: its Azure bot's application
//! (Microsoft App ID), the Microsoft Entra directory both the bot and the
//! company's people belong to, and the application's client secret. The
//! secret is tried with Entra before anything is kept (a token for the Bot
//! Framework): a refusal keeps nothing. The secret is sealed in the secret
//! store, never shown nor audited. The application and the directory never
//! change: the identities people linked are that directory's people,
//! written to by that bot; another bot is another gateway. A new secret is
//! tried alike. Audited.

use mimorya_core::DomainError;
use mimorya_core::gateways::teams::{TeamsApplication, client_secret_of, teams_application};
use mimorya_core::gateways::{GatewayKind, gateway_name};
use mimorya_db::is_unique_violation;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::gateways::{
    GatewayPatch, NewGateway, create_gateway, lock_gateway, update_gateway,
};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::connectors::PERMISSION;
use super::gateways::GatewayChanges;
use super::{audit_entry, begin_change};
use crate::services::AccessContext;
use crate::services::gateways::{GatewayService, client_secret_name};

const NOT_FOUND: &str = "Passerelle introuvable";

pub struct NewTeamsGatewayInput {
    pub name: String,
    pub app_id: String,
    pub directory_id: String,
    pub client_secret: String,
    pub enabled: bool,
}

pub async fn declare_teams_gateway(
    gateways: &GatewayService,
    access: &AccessContext,
    input: NewTeamsGatewayInput,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    let name = gateway_name(&input.name).map_err(DomainError::validation)?;
    let app = teams_application(&input.app_id, &input.directory_id, &input.client_secret)
        .map_err(DomainError::validation)?;
    gateways.secrets().require()?;
    // Asked of Entra before the lock: a slow answer holds no change.
    tried(gateways, &app).await?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(gateways.db(), access, PERMISSION).await?;
    let created = create_gateway(
        &mut tx,
        NewGateway {
            tenant_id,
            kind: GatewayKind::Teams.as_str(),
            name: &name,
            enabled: input.enabled,
            workspace_id: &app.directory_id,
            workspace_name: &app.directory_id,
            bot_user_id: &app.app_id,
            granted_scopes: None,
        },
    )
    .await;
    let id = match created {
        Ok(id) => id,
        Err(error) if is_unique_violation(&error) => {
            return Err(DomainError::conflict(
                "Le bot Teams de l’entreprise est déjà déclaré : modifiez-le, ou supprimez-le \
                 avant d’en déclarer un autre",
            )
            .into());
        }
        Err(error) => return Err(error.into()),
    };
    gateways
        .secrets()
        .put(
            &mut tx,
            tenant_id,
            &client_secret_name(id),
            &app.client_secret,
        )
        .await?;
    let entry = audit_entry(access, "gateway.created", "gateway", id).metadata(json!({
        "name": name,
        "type": GatewayKind::Teams.as_str(),
        "enabled": input.enabled,
        "appId": app.app_id,
        "directoryId": app.directory_id,
        "secretsStored": true,
    }));
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(id)
}

/// Changes the Teams gateway `id`, locked: its name, whether it is enabled,
/// its client secret (tried with Entra for the same application).
pub(super) async fn change_teams_gateway(
    gateways: &GatewayService,
    access: &AccessContext,
    id: Uuid,
    changes: GatewayChanges,
) -> anyhow::Result<()> {
    if changes.bot_token.is_some() || changes.signing_secret.is_some() {
        return Err(DomainError::validation(
            "Un bot Teams n’a ni jeton ni secret de signature : changez son secret client",
        )
        .into());
    }
    let name = (changes.name.as_deref().map(gateway_name))
        .transpose()
        .map_err(DomainError::validation)?;
    let secret = (changes.client_secret.as_deref().map(client_secret_of))
        .transpose()
        .map_err(DomainError::validation)?;
    let tenant_id = access.subject.tenant_id;
    if let Some(secret) = &secret {
        gateways.secrets().require()?;
        // Tried before the lock with what the gateway was declared with:
        // neither changes.
        let mut tx = gateways.db().begin(tenant_id).await?;
        let gateway = mimorya_db::repositories::gateways::get_gateway(&mut tx, id).await?;
        tx.commit().await?;
        let Some(gateway) = gateway else {
            return Err(DomainError::not_found(NOT_FOUND).into());
        };
        let app = TeamsApplication {
            app_id: gateway.bot_user_id,
            directory_id: gateway.workspace_id,
            client_secret: secret.clone(),
        };
        tried(gateways, &app).await?;
    }
    let mut tx = begin_change(gateways.db(), access, PERMISSION).await?;
    if lock_gateway(&mut tx, id).await?.is_none() {
        return Err(DomainError::not_found(NOT_FOUND).into());
    }
    let patch = GatewayPatch {
        name: name.as_deref(),
        enabled: changes.enabled,
        ..GatewayPatch::default()
    };
    update_gateway(&mut tx, id, patch).await?;
    if let Some(secret) = &secret {
        gateways
            .secrets()
            .put(&mut tx, tenant_id, &client_secret_name(id), secret)
            .await?;
    }
    let mut metadata = Map::new();
    if let Some(name) = &name {
        metadata.insert("name".into(), json!(name));
    }
    if let Some(enabled) = changes.enabled {
        metadata.insert("enabled".into(), json!(enabled));
    }
    metadata.insert("clientSecretChanged".into(), json!(secret.is_some()));
    let entry =
        audit_entry(access, "gateway.updated", "gateway", id).metadata(Value::Object(metadata));
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(())
}

/// Whether Entra gives `app` a token for the Bot Framework; its refusal
/// said in French.
async fn tried(gateways: &GatewayService, app: &TeamsApplication) -> anyhow::Result<()> {
    gateways
        .teams()
        .try_application(app)
        .await
        .map_err(|failure| DomainError::validation(failure.message()).into())
}
