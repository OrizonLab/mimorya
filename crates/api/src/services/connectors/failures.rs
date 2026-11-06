//! What administrators see of a connector that cannot be used: its token
//! opened from the secret store or why it cannot be, and the recorded
//! message of a failed start (bounded, the token masked once more),
//! recorded by the start that failed, a start that panicked an interrupted
//! one (ADR-177); and whether a person's failed instance is theirs to
//! authorize again (ADR-174).

use std::future::Future;
use std::panic::AssertUnwindSafe;

use chrono::{DateTime, Utc};
use futures::FutureExt;
use mimorya_connectors::ConnectorRuntime;
use mimorya_connectors::mcp::{OAUTH_EXPIRED_PERSON, OAUTH_REFUSED_PERSON, OAUTH_UNUSABLE_PERSON};
use mimorya_connectors::outbound::Redactor;
use mimorya_db::repositories::connectors::report_connector_status;
use uuid::Uuid;

use super::UNOPENED_PERSON;
use super::cache::INTERRUPTED;
use crate::secrets::SecretRead;

const MAX_ERROR_CHARS: usize = 300;

/// The token of an instance, or why it cannot be used.
pub fn opened(read: SecretRead) -> anyhow::Result<Option<String>> {
    match read {
        SecretRead::Absent => Ok(None),
        SecretRead::Value(token) => Ok(Some(token)),
        SecretRead::NoMasterKey => {
            anyhow::bail!("Ce serveur n’a pas de clé maîtresse : le jeton ne peut pas être lu")
        }
        SecretRead::Unreadable => {
            anyhow::bail!(
                "Le jeton enregistré ne peut pas être déchiffré : saisissez-le de nouveau"
            )
        }
    }
}

/// The recorded message of a failed start. Connectors explain their own
/// failures in French; the token is masked again whatever the connector did.
pub fn failure(error: &anyhow::Error, secret: Option<&str>) -> String {
    Redactor::new(secret)
        .text(&format!("{error:#}"))
        .chars()
        .take(MAX_ERROR_CHARS)
        .collect()
}

/// What a start came to: its instance, or its failure's recorded message,
/// a start that panicked an interrupted one.
pub(super) async fn outcome(
    started: impl Future<Output = anyhow::Result<ConnectorRuntime>>,
    secret: Option<&str>,
) -> Result<ConnectorRuntime, String> {
    match AssertUnwindSafe(started).catch_unwind().await {
        Ok(started) => started.map_err(|error| failure(&error, secret)),
        Err(_) => Err(INTERRUPTED.to_string()),
    }
}

/// Records the company's instance as failing with `message`, for the
/// version that failed only; a failure to record it is logged.
pub(super) async fn record_failure(
    db: &mimorya_db::Database,
    tenant_id: Uuid,
    id: Uuid,
    version: DateTime<Utc>,
    message: &str,
) {
    let recorded = async {
        let mut tx = db.begin(tenant_id).await?;
        report_connector_status(&mut tx, id, version, Some(message)).await?;
        tx.commit().await?;
        anyhow::Ok(())
    };
    if let Err(error) = recorded.await {
        let error = format!("{error:#}");
        tracing::warn!(connector = %id, %error, "Connector failure not recorded");
    }
}

/// Whether a person's instance failed for their authorization's token,
/// expired, refused by the server, unusable or unopenable: theirs to give
/// anew (ADR-174).
pub(super) fn theirs_to_authorize(message: &str) -> bool {
    [
        OAUTH_EXPIRED_PERSON,
        OAUTH_REFUSED_PERSON,
        OAUTH_UNUSABLE_PERSON,
        UNOPENED_PERSON,
    ]
    .iter()
    .any(|said| message.starts_with(said))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recorded_failure_is_bounded_and_never_shows_the_token() {
        let error = anyhow::anyhow!("Accès refusé : jeton jeton-secret-123 refusé");
        let shown = failure(&error, Some("jeton-secret-123"));
        assert_eq!(shown, "Accès refusé : jeton *** refusé");
        let long = anyhow::anyhow!("{}", "é".repeat(1000));
        assert_eq!(failure(&long, None).chars().count(), MAX_ERROR_CHARS);
        assert!(opened(SecretRead::NoMasterKey).is_err());
        assert!(opened(SecretRead::Unreadable).is_err());
        assert_eq!(opened(SecretRead::Absent).unwrap(), None);
    }

    #[tokio::test]
    async fn a_start_that_panics_comes_to_an_interrupted_one() {
        let panicking = async { panic!("démarrage interrompu par le test") };
        let came = outcome(panicking, None).await;
        assert_eq!(came.err().as_deref(), Some(INTERRUPTED));
        let failing = async { anyhow::bail!("Jeton jeton-secret-123 refusé") };
        let came = outcome(failing, Some("jeton-secret-123")).await;
        assert_eq!(came.err().as_deref(), Some("Jeton *** refusé"));
    }

    #[test]
    fn a_person_s_own_token_failing_is_theirs_to_authorize_again() {
        for said in [
            OAUTH_EXPIRED_PERSON,
            OAUTH_REFUSED_PERSON,
            OAUTH_UNUSABLE_PERSON,
            UNOPENED_PERSON,
        ] {
            assert!(theirs_to_authorize(said), "{said}");
            let detailed = format!("{said} (détail : HTTP 401)");
            assert!(theirs_to_authorize(&detailed), "{detailed}");
        }
        // The server's own failures stay unavailable.
        for said in [
            "Le serveur MCP a renvoyé une erreur",
            "Délai dépassé",
            mimorya_connectors::mcp::OAUTH_REFUSED,
        ] {
            assert!(!theirs_to_authorize(said), "{said}");
        }
    }
}
