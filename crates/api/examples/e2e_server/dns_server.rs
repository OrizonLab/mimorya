//! The DNS of the end-to-end tests (ADR-069), at an address of its own
//! (`DNS_ADDRESS`): a test publishes a domain's TXT record there
//! (`PUT /records/{name}`, its value as the body), which the API reads when
//! the domain is proved.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::put;

use crate::fake_dns::FakeDns;

pub async fn start_on(address: &str, dns: Arc<FakeDns>) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(address).await?;
    let router = Router::new()
        .route("/records/{name}", put(publish))
        .with_state(dns);
    tokio::spawn(async move { axum::serve(listener, router).await });
    Ok(())
}

async fn publish(
    State(dns): State<Arc<FakeDns>>,
    Path(name): Path<String>,
    value: String,
) -> StatusCode {
    dns.publish(&name, &value);
    StatusCode::NO_CONTENT
}
