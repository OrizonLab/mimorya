//! TLS as the platform speaks it to what companies connect (ADR-162,
//! ADR-205): ring, the protocol versions rustls deems safe, the webpki roots
//! (beside them, in tests alone, the roots of their simulated servers),
//! never a client certificate. A certificate that does not chain to these
//! roots, or does not name the host, ends the connection.

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use rustls::pki_types::ServerName;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;

use super::failure::{Cause, TOO_SLOW};
use super::network::NetworkPolicy;
use super::tcp::Connection;

/// What a connection that could not be secured is told.
pub const NOT_SECURE: &str =
    "Connexion chiffrée impossible : le certificat du serveur est refusé ou TLS n’aboutit pas";
/// How long a TLS handshake may take.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// The configuration without the tests' roots, made once for the process:
/// its sessions may then be resumed.
static WEBPKI_ONLY: LazyLock<Result<Arc<rustls::ClientConfig>, String>> = LazyLock::new(|| {
    client_config(NetworkPolicy::default())
        .map(Arc::new)
        .map_err(|error| error.to_string())
});

/// The TLS configuration of every client of what companies connect.
pub fn client_config(policy: NetworkPolicy) -> anyhow::Result<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    for root in policy.test_roots {
        roots.add(root.clone())?;
    }
    Ok(rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_root_certificates(roots)
    .with_no_client_auth())
}

/// `connection` secured for `host`: its certificate chains to the roots and
/// names the host (a name or a literal address). Why it could not be, if
/// it could not.
pub async fn secured(
    connection: Connection,
    host: &str,
    policy: NetworkPolicy,
) -> Result<Connection, Cause> {
    let refused = |detail: String| Cause {
        summary: NOT_SECURE,
        detail,
    };
    let name = ServerName::try_from(host.to_string())
        .map_err(|_| refused(format!("{host} : nom invalide pour TLS")))?;
    let config = if policy.test_roots.is_empty() {
        WEBPKI_ONLY.clone().map_err(refused)?
    } else {
        Arc::new(client_config(policy).map_err(|error| refused(error.to_string()))?)
    };
    let handshake = TlsConnector::from(config).connect(name, connection);
    match timeout(HANDSHAKE_TIMEOUT, handshake).await {
        Err(_) => Err(Cause {
            summary: TOO_SLOW,
            detail: "négociation TLS".into(),
        }),
        Ok(Err(error)) => Err(refused(error.to_string())),
        Ok(Ok(secured)) => Ok(Box::new(secured)),
    }
}
