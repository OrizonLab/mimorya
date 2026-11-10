//! A gateway withdrawn, as its administrator removes it (ADR-125) and as
//! the operator's release of a suspended company does (ADR-193): a
//! Telegram bot first told, under the gateway's lock, to send its updates
//! nowhere (ADR-133), at best; then the gateway, its people's identities
//! with it, and its secrets.

use mimorya_connectors::telegram::TelegramApi;
use mimorya_core::gateways::GatewayKind;
use mimorya_db::TenantTx;
use mimorya_db::repositories::gateways::{GatewayRow, delete_gateway};
use uuid::Uuid;

use crate::secrets::{SecretRead, SecretStore};
use crate::services::gateways::{
    bot_token_name, client_secret_name, signing_secret_name, webhook_secret_name,
};

/// Said when the bot's token cannot be read: its webhook stays.
const TOKEN_UNREADABLE: &str =
    "Jeton du bot illisible pour la plateforme : son webhook reste déclaré chez Telegram";

/// Deletes `gateway`, locked in `tx`, with its secrets: a Telegram bot's
/// webhook withdrawn first. How that went, for a Telegram bot (the reason
/// its webhook stays, never the token); `None` for another gateway, which
/// declares nothing of the platform's at its service.
pub(crate) async fn withdraw_gateway(
    tx: &mut TenantTx,
    secrets: &SecretStore,
    telegram: &TelegramApi,
    tenant_id: Uuid,
    gateway: &GatewayRow,
) -> anyhow::Result<Option<Result<(), String>>> {
    let id = gateway.id;
    let webhook = if GatewayKind::of(&gateway.kind) == Some(GatewayKind::Telegram) {
        // Under the lock: a bot declared again meanwhile keeps its own
        // webhook. At best: its updates then reach an unknown gateway,
        // answered as such.
        Some(
            match secrets.read(tx, tenant_id, &bot_token_name(id)).await? {
                SecretRead::Value(token) => {
                    telegram.delete_webhook(&token).await.map_err(|failure| {
                        let reason = failure.message();
                        tracing::warn!(gateway = %id, %reason, "Telegram webhook not removed");
                        reason
                    })
                }
                _ => {
                    tracing::warn!(gateway = %id, "Telegram bot token unreadable: webhook not removed");
                    Err(TOKEN_UNREADABLE.to_string())
                }
            },
        )
    } else {
        None
    };
    delete_gateway(tx, id).await?;
    for name in [
        bot_token_name(id),
        signing_secret_name(id),
        webhook_secret_name(id),
        client_secret_name(id),
    ] {
        secrets.delete(tx, &name).await?;
    }
    Ok(webhook)
}
