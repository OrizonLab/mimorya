//! What Slack says of a gateway's application token (ADR-125, ADR-132):
//! its workspace, its bot user and the permissions it has
//! (`x-oauth-scopes`). A token given to declare or change a gateway is
//! refused when it lacks one the application asks for: it could not do what
//! it must (ADR-131). A gateway already declared is checked again on
//! demand with its stored token, whatever Slack then says kept with it and
//! audited, never refused: its administrators see what is missing, or more
//! than asked for, and what to do.

use mimorya_connectors::slack::SlackWorkspace;
use mimorya_core::DomainError;
use mimorya_core::gateways::slack::{extra_scopes, missing_scopes, missing_scopes_refusal};
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::gateways::update_gateway;
use mimorya_db::repositories::gateways::{GatewayPatch, GatewayRow, get_gateway, lock_gateway};
use serde_json::{Value, json};
use uuid::Uuid;

use super::connectors::PERMISSION;
use super::{audit_entry, begin_change};
use crate::secrets::SecretRead;
use crate::services::AccessContext;
use crate::services::gateways::{GatewayService, bot_token_name};

const NOT_FOUND: &str = "Passerelle introuvable";

/// What Slack says of `token`, which must have every permission the
/// application asks for, if Slack says them; Slack's refusal said in
/// French.
pub(super) async fn application_of(
    gateways: &GatewayService,
    token: &str,
) -> anyhow::Result<SlackWorkspace> {
    let workspace = asked_of_slack(gateways, token).await?;
    if let Some(granted) = &workspace.scopes {
        let missing = missing_scopes(granted);
        if !missing.is_empty() {
            return Err(DomainError::validation(missing_scopes_refusal(&missing)).into());
        }
    }
    Ok(workspace)
}

/// The refusal of a token of another workspace than the gateway's: the
/// identities people linked are that workspace's.
pub(super) fn other_workspace(workspace: &SlackWorkspace, gateway: &GatewayRow) -> DomainError {
    DomainError::validation(format!(
        "Ce jeton est celui d’une application d’un autre espace de travail Slack ({}) : les \
         comptes reliés sont ceux de l’espace « {} ». Supprimez la passerelle pour en déclarer \
         une autre",
        workspace.workspace_name, gateway.workspace_name
    ))
}

/// The permissions as audited: whether Slack said them, what is missing
/// and what is more than asked for. Never the token.
pub(super) fn scopes_metadata(scopes: &Option<Vec<String>>) -> Value {
    match scopes {
        Some(granted) => json!({
            "said": true,
            "missing": missing_scopes(granted),
            "extra": extra_scopes(granted),
        }),
        None => json!({ "said": false }),
    }
}

/// Asks Slack again what the gateway `id`'s stored token is: what it says
/// is kept, its workspace's name, bot user and permissions, and audited.
pub async fn check_gateway(
    gateways: &GatewayService,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    gateways.secrets().require()?;
    let tenant_id = access.subject.tenant_id;
    let token = {
        let mut tx = gateways.db().begin(tenant_id).await?;
        if get_gateway(&mut tx, id).await?.is_none() {
            return Err(DomainError::not_found(NOT_FOUND).into());
        }
        let read = (gateways.secrets())
            .read(&mut tx, tenant_id, &bot_token_name(id))
            .await?;
        tx.commit().await?;
        match read {
            SecretRead::Value(token) => token,
            _ => {
                return Err(DomainError::validation(
                    "Le jeton du bot de cette passerelle ne peut pas être lu : donnez-le à \
                     nouveau (« Modifier »)",
                )
                .into());
            }
        }
    };
    // Asked of Slack before the lock: a slow answer holds no change.
    let workspace = asked_of_slack(gateways, &token).await?;
    let mut tx = begin_change(gateways.db(), access, PERMISSION).await?;
    let Some(gateway) = lock_gateway(&mut tx, id).await? else {
        return Err(DomainError::not_found(NOT_FOUND).into());
    };
    if workspace.workspace_id != gateway.workspace_id {
        return Err(other_workspace(&workspace, &gateway).into());
    }
    let patch = GatewayPatch {
        workspace_name: Some(&workspace.workspace_name),
        bot_user_id: Some(&workspace.bot_user_id),
        granted_scopes: Some(workspace.scopes.as_deref()),
        ..GatewayPatch::default()
    };
    update_gateway(&mut tx, id, patch).await?;
    let entry = audit_entry(access, "gateway.checked", "gateway", id).metadata(json!({
        "workspaceName": workspace.workspace_name,
        "botUserId": workspace.bot_user_id,
        "scopes": scopes_metadata(&workspace.scopes),
    }));
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(())
}

/// What Slack says of `token`; its refusal said in French.
async fn asked_of_slack(gateways: &GatewayService, token: &str) -> anyhow::Result<SlackWorkspace> {
    gateways
        .slack()
        .workspace(token)
        .await
        .map_err(|failure| DomainError::validation(failure.message()).into())
}
