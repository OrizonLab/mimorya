//! The version of an MCP server's OAuth as its administrators see it
//! (ADR-176): what is declared on it, or nothing, when its client's secret
//! was written (sealed apart, never in the declaration), and what declaring
//! it again would take: the company's authorization and those people gave
//! for themselves. The connectors list and a declaration read it alike
//! (`OAuthSeen`). A declaration naming the version its form was opened on
//! is refused once that changed: what its administrator saw, and was told
//! declaring again would take, is no longer what is there.

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_db::repositories::connector_authorizations::{
    ConnectorAuthorizationRow, get_connector_authorization,
};
use mimorya_db::repositories::connector_person_authorizations::count_person_authorizations;
use mimorya_db::repositories::secrets::secret_written_at;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::services::connectors::client_secret_name;

/// A declaration naming a version that is no longer the current one.
pub const DECLARATION_CHANGED: &str = "La déclaration OAuth de ce serveur a changé depuis \
                                       l’ouverture de ce formulaire : fermez-le et rouvrez-le";

/// What an administrator sees of an MCP server's OAuth besides what its
/// configuration stores: never a secret nor a token.
pub struct OAuthSeen {
    /// The company's authorization, if it was given.
    pub authorization: Option<ConnectorAuthorizationRow>,
    /// How many people authorized the server for themselves.
    pub persons: i64,
    /// When its client's secret was last written, if one is stored.
    pub secret_written_at: Option<DateTime<Utc>>,
}

impl OAuthSeen {
    /// As `conn` reads it for connector `id`.
    pub async fn read(conn: &mut PgConnection, id: Uuid) -> anyhow::Result<Self> {
        Ok(Self {
            authorization: get_connector_authorization(conn, id).await?,
            persons: count_person_authorizations(conn, id).await?,
            secret_written_at: secret_written_at(conn, &client_secret_name(id)).await?,
        })
    }

    /// The version of the server's OAuth, `stored` the `oauth` of its
    /// configuration (absent, a declaration or one that cannot be read).
    pub fn version(&self, stored: Option<&Value>) -> String {
        let authorized_at = self.authorization.as_ref().map(|row| &row.authorized_at);
        let secret = self.secret_written_at.as_ref();
        version_of(stored, authorized_at, self.persons, secret)
    }
}

/// Any change to any of them changes it.
fn version_of(
    stored: Option<&Value>,
    authorized_at: Option<&DateTime<Utc>>,
    persons: i64,
    secret_written_at: Option<&DateTime<Utc>>,
) -> String {
    let seen = json!({
        "oauth": stored.unwrap_or(&Value::Null),
        "authorizedAt": authorized_at.map(DateTime::timestamp_micros),
        "personAuthorizations": persons,
        "secretWrittenAt": secret_written_at.map(DateTime::timestamp_micros),
    });
    format!("{:x}", Sha256::digest(seen.to_string()))
}

/// Refused if a declaration names a version of connector `id`'s OAuth,
/// `replaces`, other than the one `conn` reads, `stored` what its
/// configuration stores of it; one naming none is not checked.
pub async fn unchanged_since(
    conn: &mut PgConnection,
    id: Uuid,
    stored: Option<&Value>,
    replaces: Option<&str>,
) -> anyhow::Result<()> {
    if let Some(replaces) = replaces {
        let current = OAuthSeen::read(conn, id).await?.version(stored);
        replaces_current(replaces, &current)?;
    }
    Ok(())
}

/// Refused when the version a declaration names, `replaces`, is not
/// `current`.
fn replaces_current(replaces: &str, current: &str) -> Result<(), DomainError> {
    if replaces == current {
        Ok(())
    } else {
        Err(DomainError::conflict(DECLARATION_CHANGED))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_seen_changing_changes_the_version() {
        let declared = json!({ "clientId": "a" });
        let at = DateTime::from_timestamp(1_700_000_000, 0).expect("date");
        let later = at + chrono::Duration::microseconds(1);
        let version = version_of(Some(&declared), Some(&at), 2, Some(&at));
        assert_eq!(version.len(), 64);
        assert_eq!(
            version,
            version_of(Some(&declared), Some(&at), 2, Some(&at))
        );
        for other in [
            version_of(Some(&json!({ "clientId": "b" })), Some(&at), 2, Some(&at)),
            version_of(Some(&json!("illisible")), Some(&at), 2, Some(&at)),
            version_of(None, Some(&at), 2, Some(&at)),
            version_of(Some(&declared), None, 2, Some(&at)),
            version_of(Some(&declared), Some(&later), 2, Some(&at)),
            version_of(Some(&declared), Some(&at), 3, Some(&at)),
            // Its client's secret written again, or gone.
            version_of(Some(&declared), Some(&at), 2, Some(&later)),
            version_of(Some(&declared), Some(&at), 2, None),
        ] {
            assert_ne!(version, other);
        }
        // Nothing declared, or null: alike.
        assert_eq!(
            version_of(None, None, 0, None),
            version_of(Some(&Value::Null), None, 0, None)
        );
    }

    #[test]
    fn only_another_version_named_is_refused() {
        assert!(replaces_current("v1", "v1").is_ok());
        let refused = replaces_current("v0", "v1").expect_err("changed");
        assert_eq!(refused.message, DECLARATION_CHANGED);
    }
}
