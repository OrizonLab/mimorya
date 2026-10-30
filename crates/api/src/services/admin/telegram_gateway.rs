//! The company's Telegram bot, declared by its connector administrators
//! (ADR-133), one per company. Its token is checked with Telegram before
//! anything is kept (`getMe`): the bot it is, its identifier and username,
//! is kept. Telegram is then told, under the administration lock, to send
//! the bot's updates to the gateway's address with a secret the platform
//! makes, which every update must carry back: the webhook is the gateway's
//! once it is kept, and a refusal keeps nothing. Token and secret are
//! sealed in the secret store, never shown nor audited. A new token must be
//! the same bot's: the identities people linked are its users'. Audited.

use argon2::password_hash::rand_core::{OsRng, RngCore};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use mimorya_connectors::telegram::TelegramBot;
use mimorya_core::DomainError;
use mimorya_core::gateways::telegram::bot_token;
use mimorya_core::gateways::{GatewayKind, gateway_name};
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::gateways::{
    GatewayPatch, GatewayRow, NewGateway, create_gateway, lock_gateway, update_gateway,
};
use mimorya_db::{TenantTx, is_unique_violation};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::connectors::PERMISSION;
use super::gateways::GatewayChanges;
use super::{audit_entry, begin_change};
use crate::secrets::SecretRead;
use crate::services::AccessContext;
use crate::services::gateways::{GatewayService, bot_token_name, webhook_secret_name};

const NOT_FOUND: &str = "Passerelle introuvable";

pub struct NewTelegramGatewayInput {
    pub name: String,
    pub bot_token: String,
    pub enabled: bool,
}

pub async fn declare_telegram_gateway(
    gateways: &GatewayService,
    access: &AccessContext,
    input: NewTelegramGatewayInput,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    let name = gateway_name(&input.name).map_err(DomainError::validation)?;
    let token = bot_token(&input.bot_token).map_err(DomainError::validation)?;
    gateways.secrets().require()?;
    // Asked of Telegram before the lock: a slow answer holds no change.
    let bot = bot_of(gateways, &token).await?;
    let secret = webhook_secret();
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(gateways.db(), access, PERMISSION).await?;
    let bot_id = bot.id.to_string();
    let created = create_gateway(
        &mut tx,
        NewGateway {
            tenant_id,
            kind: GatewayKind::Telegram.as_str(),
            name: &name,
            enabled: input.enabled,
            workspace_id: &bot_id,
            workspace_name: &bot.username,
            bot_user_id: &bot_id,
            granted_scopes: None,
        },
    )
    .await;
    let id = match created {
        Ok(id) => id,
        Err(error) if is_unique_violation(&error) => {
            return Err(DomainError::conflict(
                "Le bot Telegram de l’entreprise est déjà déclaré : modifiez-le, ou supprimez-le \
                 avant d’en déclarer un autre",
            )
            .into());
        }
        Err(error) => return Err(error.into()),
    };
    let secrets = gateways.secrets();
    secrets
        .put(&mut tx, tenant_id, &bot_token_name(id), &token)
        .await?;
    secrets
        .put(&mut tx, tenant_id, &webhook_secret_name(id), &secret)
        .await?;
    let entry = audit_entry(access, "gateway.created", "gateway", id).metadata(json!({
        "name": name,
        "type": GatewayKind::Telegram.as_str(),
        "enabled": input.enabled,
        "botId": bot.id,
        "botUsername": bot.username,
        "secretsStored": true,
    }));
    append_audit(&mut tx, entry).await?;
    point_webhook(gateways, &token, id, &secret).await?;
    tx.commit().await?;
    Ok(id)
}

/// Changes the Telegram gateway `gateway`, locked: its name, whether it is
/// enabled, its token (the same bot's, the webhook given it again).
pub(super) async fn change_telegram_gateway(
    gateways: &GatewayService,
    access: &AccessContext,
    id: Uuid,
    changes: GatewayChanges,
) -> anyhow::Result<()> {
    if changes.signing_secret.is_some() || changes.client_secret.is_some() {
        return Err(DomainError::validation(
            "Un bot Telegram n’a ni secret de signature ni secret client : la plateforme donne \
             le sien à Telegram, changez son jeton",
        )
        .into());
    }
    let name = (changes.name.as_deref().map(gateway_name))
        .transpose()
        .map_err(DomainError::validation)?;
    let token = (changes.bot_token.as_deref().map(bot_token))
        .transpose()
        .map_err(DomainError::validation)?;
    if token.is_some() {
        gateways.secrets().require()?;
    }
    let bot = match &token {
        Some(token) => Some(bot_of(gateways, token).await?),
        None => None,
    };
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(gateways.db(), access, PERMISSION).await?;
    let Some(gateway) = lock_gateway(&mut tx, id).await? else {
        return Err(DomainError::not_found(NOT_FOUND).into());
    };
    if let Some(bot) = &bot
        && bot.id.to_string() != gateway.bot_user_id
    {
        return Err(DomainError::validation(format!(
            "Ce jeton est celui d’un autre bot (@{}) : les comptes reliés sont ceux des \
             personnes qui écrivent à @{}. Supprimez la passerelle pour en déclarer une autre",
            bot.username, gateway.workspace_name
        ))
        .into());
    }
    apply(&mut tx, id, &name, changes.enabled, &bot).await?;
    if let Some(token) = &token {
        let secret = stored_secret(gateways, &mut tx, (tenant_id, &gateway)).await?;
        gateways
            .secrets()
            .put(&mut tx, tenant_id, &bot_token_name(id), token)
            .await?;
        point_webhook(gateways, token, id, &secret).await?;
    }
    let mut metadata = Map::new();
    if let Some(name) = &name {
        metadata.insert("name".into(), json!(name));
    }
    if let Some(enabled) = changes.enabled {
        metadata.insert("enabled".into(), json!(enabled));
    }
    if let Some(bot) = &bot {
        metadata.insert("botUsername".into(), json!(bot.username));
    }
    metadata.insert("botTokenChanged".into(), json!(token.is_some()));
    let entry =
        audit_entry(access, "gateway.updated", "gateway", id).metadata(Value::Object(metadata));
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(())
}

/// What a change writes of the gateway itself.
async fn apply(
    tx: &mut TenantTx,
    id: Uuid,
    name: &Option<String>,
    enabled: Option<bool>,
    bot: &Option<TelegramBot>,
) -> anyhow::Result<()> {
    let patch = GatewayPatch {
        name: name.as_deref(),
        enabled,
        workspace_name: bot.as_ref().map(|bot| bot.username.as_str()),
        ..GatewayPatch::default()
    };
    update_gateway(tx, id, patch).await?;
    Ok(())
}

/// The webhook's secret of `gateway`, which a new token keeps.
async fn stored_secret(
    gateways: &GatewayService,
    tx: &mut TenantTx,
    (tenant_id, gateway): (Uuid, &GatewayRow),
) -> anyhow::Result<String> {
    let name = webhook_secret_name(gateway.id);
    match gateways.secrets().read(tx, tenant_id, &name).await? {
        SecretRead::Value(secret) => Ok(secret),
        _ => Err(DomainError::validation(
            "Le secret des mises à jour de ce bot ne peut pas être lu : supprimez la \
             passerelle et déclarez-la de nouveau",
        )
        .into()),
    }
}

/// The bot `token` is, as Telegram says; its refusal said in French.
async fn bot_of(gateways: &GatewayService, token: &str) -> anyhow::Result<TelegramBot> {
    gateways
        .telegram()
        .bot(token)
        .await
        .map_err(|failure| DomainError::validation(failure.message()).into())
}

/// Tells Telegram to send the bot's updates to the gateway `id`, with
/// `secret`; its refusal said in French.
async fn point_webhook(
    gateways: &GatewayService,
    token: &str,
    id: Uuid,
    secret: &str,
) -> anyhow::Result<()> {
    let url = gateways.telegram_updates_url(id);
    gateways
        .telegram()
        .set_webhook(token, &url, secret)
        .await
        .map_err(|failure| DomainError::validation(failure.message()).into())
}

/// A webhook's secret: 384 random bits, 64 URL-safe characters.
fn webhook_secret() -> String {
    let mut bytes = [0_u8; 48];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mimorya_core::gateways::telegram::is_webhook_secret;

    #[test]
    fn a_webhook_s_secret_is_as_telegram_takes_it_and_never_twice_the_same() {
        let (first, second) = (webhook_secret(), webhook_secret());
        assert!(is_webhook_secret(&first), "{first}");
        assert_ne!(first, second);
    }
}
