//! API server entry point: configuration, logging, then, as its role says
//! (ADR-085), serve the HTTP API, execute runs, or both, until SIGINT or
//! SIGTERM, letting background runs finish before exiting.

use std::net::SocketAddr;

use mimorya_api::{App, AppConfig, BuildOptions, build_app, log_filter};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // A missing .env file is fine: the environment may come from elsewhere.
    let _ = dotenvy::dotenv();
    let config = AppConfig::from_env()?;
    tracing_subscriber::fmt()
        .with_env_filter(log_filter(&config.log_level)?)
        .init();

    let address = format!("{}:{}", config.host, config.port);
    let role = config.role;
    let app = build_app(config, BuildOptions::default()).await?;
    if role.executes_runs() && app.services.llm.platform().is_none() {
        tracing::warn!(
            "No platform LLM provider (LLM_PROVIDER): only companies with their own provider can converse."
        );
    }
    // Listening first: a process that cannot serve takes no run up.
    let listener = if role.serves_http() {
        Some(TcpListener::bind(&address).await?)
    } else {
        None
    };
    if role.executes_runs() {
        // A single instance executes runs (ADR-016): a second one stops here.
        app.services.runs.claim_execution().await?;
        match app.recover_interrupted_runs().await {
            Ok(0) => {}
            Ok(runs) => tracing::warn!(runs, "Runs interrupted by a stop of the service taken up"),
            Err(error) => tracing::error!(
                error = format!("{error:#}"),
                "Interrupted runs not taken up"
            ),
        }
    }
    match listener {
        Some(listener) => serve(&app, listener, &address).await?,
        None => {
            tracing::info!(role = ?role, "Executing runs");
            shutdown_signal().await;
            tracing::info!("Shutting down");
        }
    }
    app.shutdown().await;
    Ok(())
}

/// Serves the HTTP API until a shutdown signal; event streams end then, so
/// that their connections can close.
async fn serve(app: &App, listener: TcpListener, address: &str) -> anyhow::Result<()> {
    tracing::info!(address, role = ?app.services.config.role, "Listening");
    let service = app
        .router
        .clone()
        .into_make_service_with_connect_info::<SocketAddr>();
    let services = app.services.clone();
    axum::serve(listener, service)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            tracing::info!("Shutting down");
            services.shutdown.send_replace(true);
        })
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = interrupt => {}
        () = terminate => {}
    }
}
