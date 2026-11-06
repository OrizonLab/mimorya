//! An MCP server each person authorizes (ADR-174), written at start with
//! Lucas's own authorization, whose last renewal failed: `mcp-person-oauth.spec.ts`
//! sees the failure said in his account. Disabled, so that no run names it
//! and nothing renews it; Lucas keeps his authorization all the same, his
//! to withdraw.

use mimorya_api::app::App;
use mimorya_api::services::connectors::{person_access_sealed_as, person_refresh_sealed_as};
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::connector_person_authorizations::{
    NewPersonAuthorization, put_person_authorization,
};
use mimorya_db::repositories::connectors::{NewConnectorInstance, create_connector_instance};
use serde_json::json;

/// Its name in Lucas's account, which the test looks for.
const NAME: &str = "Serveur à renouveler";
const ISSUER: &str = "https://auth.renouveler.example";
const RESOURCE: &str = "https://mcp.renouveler.example/mcp";
/// Why its last renewal failed, as the platform says it.
pub const FAILURE: &str = "Serveur d’autorisation injoignable (délai dépassé)";

pub async fn leave(app: &App) -> anyhow::Result<()> {
    let config = json!({
        "transport": "http",
        "prefix": "renouveler",
        "url": RESOURCE,
        "oauth": {
            "issuer": ISSUER,
            "resource": RESOURCE,
            "authorizationEndpoint": format!("{ISSUER}/authorize"),
            "tokenEndpoint": format!("{ISSUER}/token"),
            "namesIssuerOnReturn": false,
            "clientId": "mimorya",
            "clientAuth": "none",
            "scopes": [],
            "holder": "person",
        },
    });
    let services = &app.services;
    let cipher = services
        .config
        .secrets
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no master key"))?;
    let mut tx = services.db.begin(tenant::ID).await?;
    let connector = create_connector_instance(
        &mut tx,
        NewConnectorInstance {
            tenant_id: tenant::ID,
            connector_type: "mcp",
            name: NAME,
            enabled: false,
            config: &config,
        },
    )
    .await?;
    let access = cipher.seal(
        tenant::ID,
        &person_access_sealed_as(connector, user::LUCAS),
        "jeton-d-acces-de-lucas",
    )?;
    let refresh = cipher.seal(
        tenant::ID,
        &person_refresh_sealed_as(connector, user::LUCAS),
        "jeton-de-renouvellement-de-lucas",
    )?;
    put_person_authorization(
        &mut tx,
        NewPersonAuthorization {
            tenant_id: tenant::ID,
            connector_id: connector,
            user_id: user::LUCAS,
            issuer: ISSUER,
            client_id: "mimorya",
            resource: RESOURCE,
            scopes: "",
            access_token: &access,
            access_expires_at: Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            refresh_token: Some(&refresh),
        },
    )
    .await?;
    sqlx::query(
        "update connector_person_authorizations set renewal_failure = $1 \
         where connector_id = $2 and user_id = $3",
    )
    .bind(FAILURE)
    .bind(connector)
    .bind(user::LUCAS)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
