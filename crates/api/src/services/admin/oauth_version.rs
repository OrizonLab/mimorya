//! The version of an MCP server's OAuth as its administrators see it
//! (ADR-176): what is declared on it, or nothing, and what declaring it
//! again would take: the company's authorization and those people gave
//! for themselves. A declaration naming the version its form was opened on
//! is refused once that changed: what its administrator saw, and was told
//! declaring again would take, is no longer what is there.

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_db::repositories::connector_authorizations::get_connector_authorization;
use mimorya_db::repositories::connector_person_authorizations::count_person_authorizations;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

/// A declaration naming a version that is no longer the current one.
pub const DECLARATION_CHANGED: &str = "La déclaration OAuth de ce serveur a changé depuis \
                                       l’ouverture de ce formulaire : fermez-le et rouvrez-le";

/// The version of `stored`, the `oauth` of an MCP configuration (absent,
/// a declaration or one that cannot be read), with the company's
/// authorization given at `authorized_at`, if it is, and `persons`
/// people's: any change to any of them changes it.
pub fn oauth_version(
    stored: Option<&Value>,
    authorized_at: Option<&DateTime<Utc>>,
    persons: i64,
) -> String {
    let seen = json!({
        "oauth": stored.unwrap_or(&Value::Null),
        "authorizedAt": authorized_at.map(DateTime::timestamp_micros),
        "personAuthorizations": persons,
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
        let current = read_oauth_version(conn, id, stored).await?;
        replaces_current(replaces, &current)?;
    }
    Ok(())
}

/// The version of connector `id`'s OAuth, `stored` what its configuration
/// stores of it, its authorizations as `conn` reads them.
async fn read_oauth_version(
    conn: &mut PgConnection,
    id: Uuid,
    stored: Option<&Value>,
) -> anyhow::Result<String> {
    let authorized = get_connector_authorization(conn, id).await?;
    let persons = count_person_authorizations(conn, id).await?;
    let authorized_at = authorized.as_ref().map(|row| &row.authorized_at);
    Ok(oauth_version(stored, authorized_at, persons))
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
        let version = oauth_version(Some(&declared), Some(&at), 2);
        assert_eq!(version.len(), 64);
        assert_eq!(version, oauth_version(Some(&declared), Some(&at), 2));
        for other in [
            oauth_version(Some(&json!({ "clientId": "b" })), Some(&at), 2),
            oauth_version(Some(&json!("illisible")), Some(&at), 2),
            oauth_version(None, Some(&at), 2),
            oauth_version(Some(&declared), None, 2),
            oauth_version(
                Some(&declared),
                Some(&(at + chrono::Duration::seconds(1))),
                2,
            ),
            oauth_version(Some(&declared), Some(&at), 3),
        ] {
            assert_ne!(version, other);
        }
        // Nothing declared, or null: alike.
        assert_eq!(
            oauth_version(None, None, 0),
            oauth_version(Some(&Value::Null), None, 0)
        );
    }

    #[test]
    fn only_another_version_named_is_refused() {
        assert!(replaces_current("v1", "v1").is_ok());
        let refused = replaces_current("v0", "v1").expect_err("changed");
        assert_eq!(refused.message, DECLARATION_CHANGED);
    }
}
