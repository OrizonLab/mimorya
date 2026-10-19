//! API server entry point: configuration, logging, then serve until SIGINT or
//! SIGTERM, letting background runs finish before exiting.

use std::net::SocketAddr;

use mimorya_api::{AppConfig, BuildOptions, build_app};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // A missing .env file is fine: the environment may come from elsewhere.
    let _ = dotenvy::dotenv();
    let config = AppConfig::from_env()?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_new(&config.log_level)?)
        .init();

    let address = format!("{}:{}", config.host, config.port);
    let app = build_app(config, BuildOptions::default()).await?;
    if app.services.llm.platform().is_none() {
        tracing::warn!(
            "No platform LLM provider (LLM_PROVIDER): only companies with their own provider can converse."
        );
    }
    let listener = TcpListener::bind(&address).await?;
    tracing::info!(address, "Listening");

    let service = app
        .router
        .clone()
        .into_make_service_with_connect_info::<SocketAddr>();
    let services = app.services.clone();
    axum::serve(listener, service)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            tracing::info!("Shutting down");
            // Event streams end so that their connections can close.
            services.shutdown.send_replace(true);
        })
        .await?;
    app.shutdown().await;
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
