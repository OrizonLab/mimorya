//! One operation of a REST API as a platform tool (ADR-112): conservative
//! until an administrator maps it, as an MCP tool is (risk `write`,
//! `connector:use`, ADR-025); its input is the operation's parameters; what
//! it sends is shown whole to whoever validates it; its call is bounded in
//! time and its answer in size, with the token masked. A status other than
//! 2xx is a failed call, whose body the model reads all the same.

use std::time::Duration;

use async_trait::async_trait;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{Tool, ToolContext, ToolInfo, ToolResult};
use reqwest_mcp::header::{ACCEPT, CONTENT_TYPE, HeaderName, HeaderValue, USER_AGENT};
use reqwest_mcp::{Client, Method};
use serde_json::Value;

use super::declaration::{HttpMethod, RestOperation, rest_tool_description, rest_tool_name};
use super::request::{build, described, input_schema, parse_input};
use super::response::read;
use crate::outbound::{CONNECTOR_USE, Redactor, TOO_SLOW, explained, request_cause};

const ACCEPTED: &str = "application/json, text/plain;q=0.9, */*;q=0.5";
const AGENT: &str = concat!("Mimorya/", env!("CARGO_PKG_VERSION"));

/// What every operation of an API shares.
#[derive(Clone)]
pub(super) struct Api {
    pub prefix: String,
    pub url: String,
    pub client: Client,
    /// The header carrying the token, marked sensitive.
    pub token: Option<(HeaderName, HeaderValue)>,
    pub redactor: Redactor,
    pub call_timeout: Duration,
}

pub(super) struct RestTool {
    info: ToolInfo,
    operation: RestOperation,
    schema: Value,
    api: Api,
}

impl RestTool {
    pub fn new(api: Api, operation: RestOperation) -> anyhow::Result<Self> {
        let name = rest_tool_name(&api.prefix, &operation.name);
        let description = rest_tool_description(&api.prefix, &operation.description);
        let info = ToolInfo::try_new(&name, &operation.title, &description, ToolRisk::Write)
            .map_err(anyhow::Error::msg)?
            .requires([CONNECTOR_USE]);
        Ok(Self {
            info,
            schema: input_schema(&operation),
            operation,
            api,
        })
    }

    fn summary(&self, status: u16) -> String {
        let prefix = &self.api.prefix;
        match status {
            200..=299 => format!("Réponse de l’API « {prefix} » (HTTP {status})"),
            300..=399 => format!("Redirection non suivie par la plateforme (HTTP {status})"),
            401 | 403 => format!("Accès refusé par l’API « {prefix} » (HTTP {status})"),
            _ => format!("Erreur de l’API « {prefix} » (HTTP {status})"),
        }
    }
}

fn method(method: HttpMethod) -> Method {
    match method {
        HttpMethod::Get => Method::GET,
        HttpMethod::Post => Method::POST,
        HttpMethod::Put => Method::PUT,
        HttpMethod::Patch => Method::PATCH,
        HttpMethod::Delete => Method::DELETE,
    }
}

#[async_trait]
impl Tool for RestTool {
    fn info(&self) -> &ToolInfo {
        &self.info
    }

    fn input_schema(&self) -> Value {
        self.schema.clone()
    }

    fn parse_input(&self, raw: &Value) -> Result<Value, Vec<String>> {
        let input = parse_input(&self.operation, raw)?;
        // A request that cannot be made is refused now, not once validated.
        build(&self.api.url, &self.operation, &input).map_err(|problem| vec![problem])?;
        Ok(input)
    }

    fn describe_action(&self, input: &Value) -> Option<String> {
        let built = build(&self.api.url, &self.operation, input).ok()?;
        Some(format!(
            "Appeler « {} » (API REST « {} ») : {}",
            self.operation.title,
            self.api.prefix,
            described(&built, self.operation.method.as_str())
        ))
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let built = build(&self.api.url, &self.operation, &input).map_err(anyhow::Error::msg)?;
        let mut request = self
            .api
            .client
            .request(method(self.operation.method), built.url)
            .header(ACCEPT, ACCEPTED)
            .header(USER_AGENT, AGENT);
        if let Some((name, value)) = &self.api.token {
            request = request.header(name.clone(), value.clone());
        }
        if let Some(body) = &built.body {
            request = request
                .header(CONTENT_TYPE, "application/json")
                .body(serde_json::to_vec(body)?);
        }
        let margin = self.api.redactor.longest();
        let call = async { read(request.send().await?, margin).await };
        let answer = match tokio::time::timeout(self.api.call_timeout, call).await {
            Ok(Ok(answer)) => answer,
            Ok(Err(error)) => return Err(explained(request_cause(&error), &self.api.redactor)),
            Err(_) => {
                let seconds = self.api.call_timeout.as_secs_f32().to_string();
                anyhow::bail!("{TOO_SLOW} (délai de {} s)", seconds.replace('.', ","));
            }
        };
        Ok(ToolResult {
            content: answer.content(&self.api.redactor),
            is_error: !(200..300).contains(&answer.status),
            summary: Some(self.summary(answer.status)),
            // Neither citations nor quality criteria: an API only answers.
            ..ToolResult::default()
        })
    }
}
