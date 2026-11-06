//! The version of an MCP server's OAuth as its administrators see it
//! (ADR-176): what is declared on it, or nothing, when its client's secret
//! was written (sealed apart, never in the declaration), and what declaring
//! it again would take: the company's authorization and those people gave
//! for themselves, each counted by when it was given, so that one withdrawn
//! and another given still changes it, never by who gave it. The
//! connectors list and a declaration read it alike (`OAuthSeen`). A
//! declaration naming the version its form was opened on is refused once
//! that changed: what its administrator saw, and was told declaring again
//! would take, is no longer what is there.

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_db::repositories::connector_authorizations::{
    ConnectorAuthorizationRow, get_connector_authorization,
};
use mimorya_db::repositories::connector_person_authorizations::person_authorization_times;
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
    /// When each of them did, in order: never who.
    person_times: Vec<DateTime<Utc>>,
    /// When its client's secret was last written, if one is stored.
    pub secret_written_at: Option<DateTime<Utc>>,
}

impl OAuthSeen {
    /// As `conn` reads it for connector `id`.
    pub async fn read(conn: &mut PgConnection, id: Uuid) -> anyhow::Result<Self> {
        let person_times = person_authorization_times(conn, id).await?;
        Ok(Self {
            authorization: get_connector_authorization(conn, id).await?,
            persons: i64::try_from(person_times.len()).unwrap_or(i64::MAX),
            person_times,
            secret_written_at: secret_written_at(conn, &client_secret_name(id)).await?,
        })
    }

    /// The version of the server's OAuth, `stored` the `oauth` of its
    /// configuration (absent, a declaration or one that cannot be read).
    pub fn version(&self, stored: Option<&Value>) -> String {
        let authorized_at = self.authorization.as_ref().map(|row| &row.authorized_at);
        let secret = self.secret_written_at.as_ref();
        version_of(stored, authorized_at, &self.person_times, secret)
    }
}

/// Any change to any of them changes it.
fn version_of(
    stored: Option<&Value>,
    authorized_at: Option<&DateTime<Utc>>,
    person_times: &[DateTime<Utc>],
    secret_written_at: Option<&DateTime<Utc>>,
) -> String {
    let persons: Vec<i64> = person_times
        .iter()
        .map(DateTime::timestamp_micros)
        .collect();
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
        let latest = later + chrono::Duration::microseconds(1);
        let two = [at, later];
        let seen = |stored: Option<&Value>, authorized, people: &[_], secret| {
            version_of(stored, authorized, people, secret)
        };
        let version = seen(Some(&declared), Some(&at), &two, Some(&at));
        assert_eq!(version.len(), 64);
        assert_eq!(version, seen(Some(&declared), Some(&at), &two, Some(&at)));
        for other in [
            seen(
                Some(&json!({ "clientId": "b" })),
                Some(&at),
                &two,
                Some(&at),
            ),
            seen(Some(&json!("illisible")), Some(&at), &two, Some(&at)),
            seen(None, Some(&at), &two, Some(&at)),
            seen(Some(&declared), None, &two, Some(&at)),
            seen(Some(&declared), Some(&later), &two, Some(&at)),
            // One more person, one fewer, or one withdrawn and another given.
            seen(Some(&declared), Some(&at), &[at, later, latest], Some(&at)),
            seen(Some(&declared), Some(&at), &[at], Some(&at)),
            seen(Some(&declared), Some(&at), &[at, latest], Some(&at)),
            // Its client's secret written again, or gone.
            seen(Some(&declared), Some(&at), &two, Some(&later)),
            seen(Some(&declared), Some(&at), &two, None),
        ] {
            assert_ne!(version, other);
        }
        // Nothing declared, or null: alike.
        assert_eq!(
            seen(None, None, &[], None),
            seen(Some(&Value::Null), None, &[], None)
        );
    }

    #[test]
    fn only_another_version_named_is_refused() {
        assert!(replaces_current("v1", "v1").is_ok());
        let refused = replaces_current("v0", "v1").expect_err("changed");
        assert_eq!(refused.message, DECLARATION_CHANGED);
    }
}
