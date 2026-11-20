//! Jev, TypeSafe AI's System One model, as an engine of decisions
//! (ADR-213): a direct REST call (`POST /v1/systemone`, the key as a bearer)
//! with the project's HTTP client, to the version the operator pins, never
//! an alias whose answers could change under the thresholds set. Every
//! failure leads to the fallback: none is shown as such, and the key never
//! appears in a log, an error or a debug output.

use std::time::Duration;

use async_trait::async_trait;

use super::answer::EngineKind;
use super::engine::{DecisionEngine, EngineAnswer, EngineFailure};
use super::jev_wire::{body, read};
use super::question::DecisionRequest;

pub const JEV_ENDPOINT: &str = "https://api.typesafe.ai";
/// The newest Jev on 2026-11-19 (<https://docs.typesafe.ai/models>).
pub const DEFAULT_JEV_MODEL: &str = "jev-1.14.0";
/// As usage is recorded.
pub const JEV_PROVIDER: &str = "typesafe";

/// Whether `model` names a version of Jev (`jev-1.14.0`), not an alias
/// (`jev-latest`, `jev-preview`).
pub fn is_versioned_jev(model: &str) -> bool {
    let Some(version) = model.strip_prefix("jev-") else {
        return false;
    };
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

/// No `Debug`: it would print the key.
pub struct JevEngine {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
}

/// The HTTP client Jev is asked with, built once and shared by every
/// engine, so that its connections are kept from one decision to the next.
#[derive(Debug, Clone)]
pub struct JevClient(reqwest::Client);

impl JevClient {
    pub fn new() -> Self {
        Self(
            reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .build()
                .expect("HTTP client"),
        )
    }
}

impl Default for JevClient {
    fn default() -> Self {
        Self::new()
    }
}

impl JevEngine {
    /// Jev at `base_url` (`JEV_ENDPOINT` outside the tests), asked with
    /// `http`.
    pub fn new(http: JevClient, api_key: String, model: &str, base_url: &str) -> Self {
        Self {
            http: http.0,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model: model.to_string(),
        }
    }
}

/// What a status says of the call.
fn failure_of(status: u16) -> EngineFailure {
    match status {
        429 | 529 | 500..=599 => EngineFailure::Unavailable,
        _ => EngineFailure::Refused,
    }
}

#[async_trait]
impl DecisionEngine for JevEngine {
    fn kind(&self) -> EngineKind {
        EngineKind::Jev
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn answer(&self, request: &DecisionRequest) -> Result<EngineAnswer, EngineFailure> {
        let response = self
            .http
            .post(format!("{}/v1/systemone", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body(&self.model, request))
            .send()
            .await
            .map_err(|_| EngineFailure::Unavailable)?;
        let status = response.status();
        if !status.is_success() {
            return Err(failure_of(status.as_u16()));
        }
        let read_body: serde_json::Value =
            response.json().await.map_err(|_| EngineFailure::Invalid)?;
        let (model, answers, usage) = read(&read_body).ok_or(EngineFailure::Invalid)?;
        Ok(EngineAnswer {
            provider: JEV_PROVIDER.into(),
            model,
            answers,
            usage,
        })
    }
}
