//! A company's Microsoft 365 application, declared by its connector
//! administrators (ADR-116): its directory and client IDs, checked here,
//! and its secret, declared as any application people link their accounts
//! at (`link_applications`). Its people then link their own accounts
//! (`services::account_links`). Its name, activation and secret change as
//! any connector's (`connectors`), as does whether its tools may write
//! (ADR-119); a link goes with the connector deleted.

use mimorya_connectors::microsoft365::Microsoft365Config;
use mimorya_core::DomainError;
use mimorya_core::account_links::{LinkKind, MICROSOFT_GRAPH, microsoft365_application};
use serde_json::{Map, json};
use uuid::Uuid;

use super::connectors::{ConnectorAdmin, PERMISSION};
use super::link_applications::{Declaration, declare_application};
use crate::services::AccessContext;

pub struct NewMicrosoft365 {
    pub name: String,
    pub directory: String,
    pub client_id: String,
    pub client_secret: String,
    pub enabled: bool,
    /// Its tools may send e-mail and create events (ADR-119).
    pub write_tools: bool,
}

pub async fn create_microsoft365_connector(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    input: NewMicrosoft365,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    let application =
        microsoft365_application(&input.directory, &input.client_id, &input.client_secret)
            .map_err(DomainError::validation)?;
    let config = serde_json::to_value(Microsoft365Config {
        directory: application.directory,
        client_id: application.client_id.to_string(),
        issuer: application.issuer,
        graph: MICROSOFT_GRAPH.into(),
        write_tools: input.write_tools,
    })?;
    let declaration = Declaration {
        kind: LinkKind::Microsoft365,
        name: input.name,
        enabled: input.enabled,
        config,
        secret: input.client_secret,
        audited: Map::from_iter([
            ("directory".into(), json!(application.directory)),
            ("clientId".into(), json!(application.client_id)),
            ("writeTools".into(), json!(input.write_tools)),
        ]),
    };
    declare_application(admin, access, declaration).await
}
