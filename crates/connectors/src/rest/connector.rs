//! The "REST API" connector type (ADR-112): a company's administrators
//! describe an API operation by operation, and each operation becomes a
//! tool. Starting it reads and checks the declaration (as when it was saved:
//! the operator may have narrowed the network policy since) and the token,
//! without calling the API: the first call tells whether it answers.

use anyhow::bail;
use async_trait::async_trait;
use mimorya_core::tools::ToolRef;
use reqwest_mcp::header::{AUTHORIZATION, HeaderName, HeaderValue};
use serde_json::Value;
use std::sync::Arc;

use super::checks::config_problem;
use super::declaration::RestConfig;
use super::tool::{Api, RestTool};
use crate::outbound::{Redactor, http_client, token_problem};
use crate::types::{Connector, ConnectorInfo, ConnectorRuntime, InstanceContext};

pub struct RestConnector;

#[async_trait]
impl Connector for RestConnector {
    fn info(&self) -> ConnectorInfo {
        ConnectorInfo {
            connector_type: "rest",
            name: "API REST",
            description: "Connecte une API REST décrite opération par opération. Chaque opération devient un outil, à associer à des permissions et à un niveau de risque par l’administrateur.",
            demo: false,
            permissions: &[],
            reaches_outside: true,
        }
    }

    async fn instantiate(
        &self,
        config: &Value,
        context: InstanceContext<'_>,
    ) -> anyhow::Result<ConnectorRuntime> {
        let config: RestConfig = serde_json::from_value(config.clone()).map_err(|error| {
            anyhow::anyhow!(
                "Déclaration de l’API illisible : supprimez-la puis ajoutez-la de nouveau (détail : {error})"
            )
        })?;
        if let Some(problem) = config_problem(&config, context.network) {
            bail!("{problem}");
        }
        // A token saved before the rule may hold what the masking cannot follow.
        if context
            .secret
            .is_some_and(|token| token_problem(token).is_some())
        {
            bail!(
                "Le jeton enregistré n’a plus un format accepté (RFC 6750) : saisissez un jeton conforme"
            );
        }
        let api = Api {
            prefix: config.prefix.clone(),
            url: config.url.clone(),
            client: http_client(context.network)?,
            token: context
                .secret
                .map(|token| token_header(config.token_header.as_deref(), token))
                .transpose()?,
            redactor: Redactor::new(context.secret),
            call_timeout: context.call_timeout,
        };
        let tools = config
            .operations
            .into_iter()
            .map(|operation| Ok(Arc::new(RestTool::new(api.clone(), operation)?) as ToolRef))
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(ConnectorRuntime {
            tools,
            closer: None,
        })
    }
}

/// The token as sent: `Authorization: Bearer …`, or as it is in the header
/// the API expects; never shown by the client's own traces.
fn token_header(header: Option<&str>, token: &str) -> anyhow::Result<(HeaderName, HeaderValue)> {
    let (name, value) = match header {
        None => (AUTHORIZATION, format!("Bearer {token}")),
        Some(header) => (
            HeaderName::from_bytes(header.as_bytes())?,
            token.to_string(),
        ),
    };
    let mut value = HeaderValue::from_str(&value)?;
    value.set_sensitive(true);
    Ok((name, value))
}
