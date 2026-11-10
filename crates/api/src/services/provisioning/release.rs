//! What a suspended company declared at other services, withdrawn there
//! by the operator before it is deleted (ADR-193), as its administration
//! would: every OAuth authorization of its MCP servers, the company's and
//! each person's, taken and revoked at the authorization server that
//! granted it (RFC 7009, ADR-172, ADR-174), and each of its Telegram bots
//! withdrawn, its webhook first (ADR-133). It runs where the master key is
//! (the `api` service), with the application's role, in the company's
//! transaction, its row locked: a reactivation or a deletion waits for it.
//! Without the master key, it takes nothing. Audited in the company's log
//! by the `system` actor, each bot as its removal is, never a token; how
//! each went is told to the operator. What a server does not revoke stays granted
//! there, to withdraw there; what was taken is gone here all the same.
//! The accounts its people linked and its Slack and Teams applications are
//! not the platform's to withdraw: they stay, as at a person's withdrawal
//! (ADR-116) or a gateway's (ADR-125).

use std::sync::Arc;

use mimorya_connectors::outbound::http_client;
use mimorya_connectors::telegram::{TELEGRAM_API, TelegramApi};
use mimorya_core::DomainError;
use mimorya_core::gateways::GatewayKind;
use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connectors::list_connector_instances;
use mimorya_db::repositories::gateways::{list_gateways, lock_gateway};
use mimorya_db::repositories::tenants::{find_tenant_by_slug, lock_tenant_status};
use serde_json::json;
use uuid::Uuid;

use super::suspension::release_everywhere;
use super::system_entry;
use crate::app::Services;
use crate::config::AppConfig;
use crate::secrets::SecretStore;
use crate::services::account_links::AccountLinkService;
use crate::services::admin::{
    ConnectorAdmin, revoke_all_waited, take_company_authorizations, withdraw_gateway,
};
use crate::services::connectors::{ConnectorService, ConnectorSettings};
use crate::services::invalidations::Invalidations;

/// How an MCP server's authorizations went.
#[derive(Debug)]
pub struct ServerReleased {
    pub name: String,
    /// The company's authorization, if it had one: revoked, or why not.
    pub company: Option<Result<(), String>>,
    pub persons_revoked: usize,
    pub persons_failed: usize,
    /// Why the first person's was not revoked, if one was not.
    pub persons_reason: Option<String>,
}

/// How a Telegram bot's webhook went: withdrawn, or why not.
#[derive(Debug)]
pub struct BotReleased {
    pub name: String,
    pub webhook: Result<(), String>,
}

/// What the release did.
#[derive(Debug)]
pub struct Released {
    pub tenant_id: Uuid,
    pub servers: Vec<ServerReleased>,
    pub bots: Vec<BotReleased>,
}

/// Releases the company whose slug is `slug`, with what a running
/// application holds: its connectors' network and its Telegram's address.
pub async fn release_company(services: &Services, slug: &str) -> anyhow::Result<Released> {
    let secrets = SecretStore::new(services.config.secrets.clone());
    let admin = ConnectorAdmin {
        db: &services.db,
        connectors: &services.connectors,
        secrets: &secrets,
        catalog: &[],
        account_links: &services.account_links,
    };
    release(&admin, services.gateways.telegram(), slug).await
}

/// `release_company`, from the operator's command line: what it needs
/// built from the API's configuration.
pub async fn release_company_with(
    db: &Database,
    config: &AppConfig,
    slug: &str,
) -> anyhow::Result<Released> {
    let secrets = SecretStore::new(config.secrets.clone());
    // No process listens to it: the release announces through PostgreSQL.
    let (invalidations, _unheard) = Invalidations::new();
    let connectors = ConnectorService::new(
        db.clone(),
        secrets.clone(),
        ConnectorSettings {
            network: config.mcp_network,
            start_timeout: config.mcp_start_timeout,
            start_patience: None,
        },
        invalidations,
    );
    let account_links = Arc::new(AccountLinkService::new(
        db.clone(),
        config.secrets.clone(),
        config.mcp_network,
        &config.web_origin,
        None,
    )?);
    let admin = ConnectorAdmin {
        db,
        connectors: &connectors,
        secrets: &secrets,
        catalog: &[],
        account_links: &account_links,
    };
    let telegram = TelegramApi::new(TELEGRAM_API, http_client(config.mcp_network)?)?;
    release(&admin, &telegram, slug).await
}

async fn release(
    admin: &ConnectorAdmin<'_>,
    telegram: &TelegramApi,
    slug: &str,
) -> anyhow::Result<Released> {
    let slug = slug.trim();
    // Without it, nothing taken could be revoked: nothing is taken.
    if !admin.secrets.available() {
        return Err(DomainError::validation(
            "MIMORYA_MASTER_KEY est obligatoire : sans elle, les jetons ne peuvent pas être \
             révoqués ; rien n’a été retiré",
        )
        .into());
    }
    let unknown = || DomainError::not_found("Aucune entreprise ne porte ce sigle");
    let tenant_id = find_tenant_by_slug(admin.db.pool(), slug)
        .await?
        .ok_or_else(unknown)?;
    let mut tx = admin.db.begin(tenant_id).await?;
    match lock_tenant_status(&mut tx, tenant_id).await? {
        None => return Err(unknown().into()),
        Some(status) if status != "suspended" => {
            return Err(DomainError::conflict(
                "Seule une entreprise suspendue voit ses accords retirés chez les autres \
                 services : mimorya-admin suspend-company --slug <sigle> d’abord",
            )
            .into());
        }
        Some(_) => {}
    }
    let names: Vec<(Uuid, String)> = list_connector_instances(&mut tx, false)
        .await?
        .into_iter()
        .map(|instance| (instance.id, instance.name))
        .collect();
    let revocations = take_company_authorizations(&mut tx, admin, tenant_id).await?;
    let company = revocations.iter().filter(|r| r.company_taken()).count();
    let persons: usize = revocations.iter().map(|r| r.persons_taken()).sum();
    let mut bots = Vec::new();
    for listed in list_gateways(&mut tx).await? {
        if GatewayKind::of(&listed.kind) != Some(GatewayKind::Telegram) {
            continue;
        }
        let Some(gateway) = lock_gateway(&mut tx, listed.id).await? else {
            continue;
        };
        let Some(webhook) =
            withdraw_gateway(&mut tx, admin.secrets, telegram, tenant_id, &gateway).await?
        else {
            continue;
        };
        // Audited as its administrator's removal is, with how its webhook
        // went: never its token.
        let mut entry =
            system_entry(tenant_id, "gateway.deleted", "gateway", gateway.id).metadata(json!({
                "name": gateway.name,
                "type": gateway.kind,
                "workspaceId": gateway.workspace_id,
                "webhookWithdrawn": webhook.is_ok(),
            }));
        if let Err(reason) = &webhook {
            entry = entry.reason(reason.clone());
        }
        append_audit(&mut tx, entry).await?;
        bots.push(BotReleased {
            name: gateway.name,
            webhook,
        });
    }
    release_everywhere(&mut tx, tenant_id).await?;
    let entry = system_entry(tenant_id, "tenant.released", "tenant", tenant_id).metadata(json!({
        "slug": slug,
        "companyAuthorizations": company,
        "personAuthorizations": persons,
        "telegramBots": bots.len(),
    }));
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;

    // Sent once nothing is held, each within its bound, every one waited for.
    let outcomes = revoke_all_waited(admin, tenant_id, None, revocations).await;
    let named = |id: Uuid| {
        names
            .iter()
            .find(|(named, _)| *named == id)
            .map_or_else(|| id.to_string(), |(_, name)| name.clone())
    };
    let servers = outcomes
        .into_iter()
        .map(|outcome| ServerReleased {
            name: named(outcome.connector_id),
            company: outcome.company,
            persons_revoked: outcome.persons_revoked,
            persons_failed: outcome.persons_failed,
            persons_reason: outcome.persons_reason,
        })
        .collect();
    Ok(Released {
        tenant_id,
        servers,
        bots,
    })
}
