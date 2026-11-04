//! An MCP server whose OAuth declaration no longer reads, written at start
//! as an older format or a damaged row would leave it: `mcp-oauth.spec.ts`
//! withdraws it from its card, then deletes the server (ADR-171). Disabled,
//! so that no run of the tests before it names it unavailable.

use mimorya_api::app::App;
use mimorya_core::demo::tenant;
use mimorya_db::repositories::connectors::{NewConnectorInstance, create_connector_instance};
use serde_json::json;

/// Its name on the card, which the test looks for.
const NAME: &str = "Serveur OAuth illisible";

pub async fn leave(app: &App) -> anyhow::Result<()> {
    let config = json!({
        "transport": "http",
        "prefix": "illisible",
        "url": "https://mcp.illisible.example/mcp",
        // An issuer that is no address: the declaration does not read.
        "oauth": { "issuer": 1 },
    });
    let mut tx = app.services.db.begin(tenant::ID).await?;
    create_connector_instance(
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
    tx.commit().await?;
    Ok(())
}
