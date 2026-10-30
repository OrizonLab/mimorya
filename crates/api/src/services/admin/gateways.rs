//! The conversation gateways of a company, declared by its connector
//! administrators (ADR-125): its Slack application, one per company. Its
//! bot's token is checked with Slack before anything is kept (`auth.test`,
//! `gateway_check`): the workspace, bot user and permissions Slack names
//! are kept, a token lacking one the application asks for refused
//! (ADR-132), the token and the signing secret sealed in the secret store,
//! never shown nor audited. A new token must be the application's in the
//! same workspace: the identities people linked are that workspace's.
//! Under the administration lock, audited; deleting a gateway deletes its
//! people's identities. A Telegram bot (`telegram_gateway`, ADR-133) and a
//! Microsoft Teams bot (`teams_gateway`, ADR-138) are declared alike.

use mimorya_connectors::slack::SlackWorkspace;
use mimorya_core::DomainError;
use mimorya_core::gateways::slack::{bot_token, signing_secret, slack_application};
use mimorya_core::gateways::{GatewayKind, gateway_name};
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::gateways::{
    GatewayPatch, NewGateway, create_gateway, delete_gateway, get_gateway, lock_gateway,
    update_gateway,
};
use mimorya_db::{TenantTx, is_unique_violation};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::connectors::PERMISSION;
use super::gateway_check::{application_of, other_workspace, scopes_metadata};
use super::teams_gateway::change_teams_gateway;
use super::telegram_gateway::change_telegram_gateway;
use super::{audit_entry, begin_change};
use crate::secrets::SecretRead;
use crate::services::AccessContext;
use crate::services::gateways::{
    GatewayService, bot_token_name, client_secret_name, signing_secret_name, webhook_secret_name,
};

const NOT_FOUND: &str = "Passerelle introuvable";

pub struct NewSlackGatewayInput {
    pub name: String,
    pub bot_token: String,
    pub signing_secret: String,
    pub enabled: bool,
}

/// What a change sets: `None` leaves a field as it stands.
#[derive(Default)]
pub struct GatewayChanges {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub bot_token: Option<String>,
    pub signing_secret: Option<String>,
    /// A Teams bot's (ADR-138).
    pub client_secret: Option<String>,
}

pub async fn declare_slack_gateway(
    gateways: &GatewayService,
    access: &AccessContext,
    input: NewSlackGatewayInput,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    let name = gateway_name(&input.name).map_err(DomainError::validation)?;
    let application = slack_application(&input.bot_token, &input.signing_secret)
        .map_err(DomainError::validation)?;
    gateways.secrets().require()?;
    // Asked of Slack before the lock: a slow answer holds no change.
    let workspace = application_of(gateways, &application.bot_token).await?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(gateways.db(), access, PERMISSION).await?;
    let created = create_gateway(
        &mut tx,
        NewGateway {
            tenant_id,
            kind: GatewayKind::Slack.as_str(),
            name: &name,
            enabled: input.enabled,
            workspace_id: &workspace.workspace_id,
            workspace_name: &workspace.workspace_name,
            bot_user_id: &workspace.bot_user_id,
            granted_scopes: workspace.scopes.as_deref(),
        },
    )
    .await;
    let id = match created {
        Ok(id) => id,
        Err(error) if is_unique_violation(&error) => {
            return Err(DomainError::conflict(
                "La passerelle Slack de l’entreprise est déjà déclarée : modifiez-la, ou \
                 supprimez-la avant d’en déclarer une autre",
            )
            .into());
        }
        Err(error) => return Err(error.into()),
    };
    let secrets = gateways.secrets();
    secrets
        .put(
            &mut tx,
            tenant_id,
            &bot_token_name(id),
            &application.bot_token,
        )
        .await?;
    secrets
        .put(
            &mut tx,
            tenant_id,
            &signing_secret_name(id),
            &application.signing_secret,
        )
        .await?;
    let entry = audit_entry(access, "gateway.created", "gateway", id).metadata(json!({
        "name": name,
        "type": GatewayKind::Slack.as_str(),
        "enabled": input.enabled,
        "workspaceId": workspace.workspace_id,
        "workspaceName": workspace.workspace_name,
        "botUserId": workspace.bot_user_id,
        "scopes": scopes_metadata(&workspace.scopes),
        "secretsStored": true,
    }));
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(id)
}

pub async fn change_gateway(
    gateways: &GatewayService,
    access: &AccessContext,
    id: Uuid,
    changes: GatewayChanges,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    // A gateway's service never changes: read before the lock.
    match kind_of(gateways, access, id).await? {
        Some(GatewayKind::Telegram) => {
            return change_telegram_gateway(gateways, access, id, changes).await;
        }
        Some(GatewayKind::Teams) => {
            return change_teams_gateway(gateways, access, id, changes).await;
        }
        _ => {}
    }
    if changes.client_secret.is_some() {
        return Err(DomainError::validation(
            "Une application Slack n’a pas de secret client : changez son jeton ou son secret \
             de signature",
        )
        .into());
    }
    let name = changes
        .name
        .as_deref()
        .map(gateway_name)
        .transpose()
        .map_err(DomainError::validation)?;
    let token = changes
        .bot_token
        .as_deref()
        .map(bot_token)
        .transpose()
        .map_err(DomainError::validation)?;
    let secret = changes
        .signing_secret
        .as_deref()
        .map(signing_secret)
        .transpose()
        .map_err(DomainError::validation)?;
    if token.is_some() || secret.is_some() {
        gateways.secrets().require()?;
    }
    let workspace = match &token {
        Some(token) => Some(application_of(gateways, token).await?),
        None => None,
    };
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(gateways.db(), access, PERMISSION).await?;
    let Some(gateway) = lock_gateway(&mut tx, id).await? else {
        return Err(DomainError::not_found(NOT_FOUND).into());
    };
    if let Some(workspace) = &workspace
        && workspace.workspace_id != gateway.workspace_id
    {
        return Err(other_workspace(workspace, &gateway).into());
    }
    apply(&mut tx, id, &name, changes.enabled, &workspace).await?;
    let secrets = gateways.secrets();
    if let Some(token) = &token {
        secrets
            .put(&mut tx, tenant_id, &bot_token_name(id), token)
            .await?;
    }
    if let Some(secret) = &secret {
        secrets
            .put(&mut tx, tenant_id, &signing_secret_name(id), secret)
            .await?;
    }
    let mut metadata = Map::new();
    if let Some(name) = &name {
        metadata.insert("name".into(), json!(name));
    }
    if let Some(enabled) = changes.enabled {
        metadata.insert("enabled".into(), json!(enabled));
    }
    if let Some(workspace) = &workspace {
        metadata.insert("botUserId".into(), json!(workspace.bot_user_id));
        metadata.insert("scopes".into(), scopes_metadata(&workspace.scopes));
    }
    metadata.insert("botTokenChanged".into(), json!(token.is_some()));
    metadata.insert("signingSecretChanged".into(), json!(secret.is_some()));
    let entry =
        audit_entry(access, "gateway.updated", "gateway", id).metadata(Value::Object(metadata));
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn remove_gateway(
    gateways: &GatewayService,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(gateways.db(), access, PERMISSION).await?;
    let Some(gateway) = lock_gateway(&mut tx, id).await? else {
        return Err(DomainError::not_found(NOT_FOUND).into());
    };
    let secrets = gateways.secrets();
    if GatewayKind::of(&gateway.kind) == Some(GatewayKind::Telegram) {
        // Told, under the lock, to send its updates nowhere: a bot declared
        // again meanwhile keeps its own webhook. At best: its updates then
        // reach an unknown gateway, answered as such.
        match secrets
            .read(&mut tx, tenant_id, &bot_token_name(id))
            .await?
        {
            SecretRead::Value(token) => {
                if let Err(failure) = gateways.telegram().delete_webhook(&token).await {
                    let reason = failure.message();
                    tracing::warn!(gateway = %id, %reason, "Telegram webhook not removed");
                }
            }
            _ => {
                tracing::warn!(gateway = %id, "Telegram bot token unreadable: webhook not removed")
            }
        }
    }
    delete_gateway(&mut tx, id).await?;
    secrets.delete(&mut tx, &bot_token_name(id)).await?;
    secrets.delete(&mut tx, &signing_secret_name(id)).await?;
    secrets.delete(&mut tx, &webhook_secret_name(id)).await?;
    secrets.delete(&mut tx, &client_secret_name(id)).await?;
    let entry = audit_entry(access, "gateway.deleted", "gateway", id).metadata(json!({
        "name": gateway.name,
        "type": gateway.kind,
        "workspaceId": gateway.workspace_id,
    }));
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(())
}

/// The service of the company's gateway `id`, if it has one.
async fn kind_of(
    gateways: &GatewayService,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<Option<GatewayKind>> {
    let mut tx = gateways.db().begin(access.subject.tenant_id).await?;
    let gateway = get_gateway(&mut tx, id).await?;
    tx.commit().await?;
    Ok(gateway.and_then(|gateway| GatewayKind::of(&gateway.kind)))
}

/// What a change writes of the gateway itself.
async fn apply(
    tx: &mut TenantTx,
    id: Uuid,
    name: &Option<String>,
    enabled: Option<bool>,
    workspace: &Option<SlackWorkspace>,
) -> anyhow::Result<()> {
    let patch = GatewayPatch {
        name: name.as_deref(),
        enabled,
        workspace_name: workspace.as_ref().map(|w| w.workspace_name.as_str()),
        bot_user_id: workspace.as_ref().map(|w| w.bot_user_id.as_str()),
        granted_scopes: workspace.as_ref().map(|w| w.scopes.as_deref()),
    };
    update_gateway(tx, id, patch).await?;
    Ok(())
}
