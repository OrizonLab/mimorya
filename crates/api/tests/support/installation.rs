//! The installation check's tests (ADR-223): its configuration, read as the
//! api service reads it, and a site answering as the stack's does.

use axum::Router;
use axum::routing::get;
use mimorya_api::AppConfig;
use mimorya_api::services::installation::{Finding, Report};
use mimorya_connectors::outbound::NetworkPolicy;

use super::app_url;

/// The platform's own address, reached through no proxy.
pub const DIRECT: NetworkPolicy = NetworkPolicy {
    allow_private_network: true,
    allow_stdio: false,
    proxy: None,
    test_servers: &[],
    test_roots: &[],
};

/// The configuration of a check, the tests' database and no model but
/// `vars` say otherwise.
pub fn check_config(vars: &[(&str, &str)]) -> AppConfig {
    let mut all = vec![
        ("MIMORYA_ENV", "test".to_string()),
        ("DATABASE_APP_URL", app_url().to_string()),
        ("LLM_PROVIDER", "none".to_string()),
    ];
    all.extend(vars.iter().map(|(key, value)| (*key, (*value).to_string())));
    AppConfig::from_vars(|key| {
        all.iter()
            .rev()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.clone())
    })
    .expect("config")
}

/// The finding of `topic` in `report`.
pub fn found<'a>(report: &'a Report, topic: &str) -> &'a Finding {
    report
        .findings
        .iter()
        .find(|finding| finding.topic == topic)
        .unwrap_or_else(|| panic!("no {topic} in {report:?}"))
}

/// What the stack answers at `/api/health`.
pub fn health() -> Router {
    Router::new().route("/api/health", get(|| async { "ok" }))
}
