//! The API as the end-to-end tests drive it (`apps/web/e2e`): the real
//! server and database, but a scripted model playing the reference scenario
//! (ADR-031) and e-mails written to a directory instead of sent. An example,
//! not a binary: it is never built into a release.
//!
//!   cargo run -p mimorya-api --example e2e_server
//!
//! Reads the usual configuration (`DATABASE_APP_URL`, `WEB_ORIGIN`…) and
//! `E2E_MAIL_DIR`, where each e-mail becomes a JSON file.

mod mailbox;
mod scenario;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use mimorya_agent::llm::LlmProvider;
use mimorya_agent::testing::{ScriptedProvider, ScriptedStep};
use mimorya_api::mail::Mailer;
use mimorya_api::services::ProviderFactory;
use mimorya_api::{AppConfig, BuildOptions, build_app, log_filter};

/// Reference scenarios queued for the platform model.
const SCENARIOS: usize = 20;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::from_env()?;
    tracing_subscriber::fmt()
        .with_env_filter(log_filter(&config.log_level)?)
        .init();
    let mail_dir = PathBuf::from(
        std::env::var("E2E_MAIL_DIR").unwrap_or_else(|_| "target/e2e/mail".to_string()),
    );
    std::fs::create_dir_all(&mail_dir)?;

    let model = Arc::new(ScriptedProvider::new(Vec::new()));
    for _ in 0..SCENARIOS {
        model.push(scenario::reference());
    }
    let platform: Arc<dyn LlmProvider> = model;
    // A company's own provider answers « OK » (its connection test).
    let factory: ProviderFactory = Arc::new(|_config| {
        let provider: Arc<dyn LlmProvider> = Arc::new(ScriptedProvider::new(
            (0..SCENARIOS).map(|_| ScriptedStep::text("OK")).collect(),
        ));
        Ok(provider)
    });
    let mailer: Arc<dyn Mailer> = Arc::new(mailbox::DirectoryMailer::new(mail_dir));
    let address = format!("{}:{}", config.host, config.port);
    let app = build_app(
        config,
        BuildOptions {
            provider: Some(Some(platform)),
            provider_factory: Some(factory),
            mailer: Some(Some(mailer)),
        },
    )
    .await?;
    let listener = tokio::net::TcpListener::bind(&address).await?;
    tracing::info!(address, "End-to-end API listening");
    axum::serve(
        listener,
        app.router
            .clone()
            .into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
