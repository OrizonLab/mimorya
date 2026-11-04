//! What an administrator declares of an MCP server's OAuth (ADR-171), kept
//! in the connector's configuration: where the discovery found it is
//! authorized (the resource tokens are for, the issuer and its endpoints,
//! whether its return names the issuer), the client the company registered
//! there, how that client proves itself, and the scopes it asks for. The
//! client's secret is not here: it is sealed apart, and only whether there
//! is one decides how the client proves itself. Until it is authorized, a
//! declared server starts no session.

use serde::{Deserialize, Serialize};

use super::discovery::Discovery;
use super::scope::scopes_problem;
use super::server_metadata::ClientAuth;

/// What the connector says until an administrator authorizes it.
pub const OAUTH_TO_AUTHORIZE: &str = "Autorisation OAuth à donner : un administrateur doit \
     autoriser ce connecteur auprès de son serveur d’autorisation";

pub const MAX_CLIENT_ID_CHARS: usize = 255;
pub const MAX_CLIENT_SECRET_CHARS: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OAuthDeclaration {
    pub issuer: String,
    /// The resource tokens are asked for (RFC 8707).
    pub resource: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    /// Whether the return names the issuer (RFC 9207), and must then.
    pub names_issuer_on_return: bool,
    pub client_id: String,
    pub client_auth: ClientAuth,
    pub scopes: Vec<String>,
    /// Where the platform asks for the tokens it lets go of to be revoked
    /// (RFC 7009, ADR-172), if the server names one: absent from the
    /// declarations kept before it was read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation_endpoint: Option<String>,
}

impl OAuthDeclaration {
    /// The declaration of `client_id` at the authorization server
    /// `discovery` found, with a secret or without, asking for `scopes`,
    /// else for those the server suggests.
    pub fn new(
        discovery: Discovery,
        client_id: &str,
        has_secret: bool,
        scopes: Option<Vec<String>>,
    ) -> Result<Self, String> {
        if let Some(problem) = client_id_problem(client_id) {
            return Err(problem);
        }
        let scopes = match scopes {
            Some(scopes) => match scopes_problem(&scopes) {
                Some(problem) => return Err(problem),
                None => scopes,
            },
            None => discovery.suggested_scopes,
        };
        let server = discovery.server;
        Ok(Self {
            client_auth: server.client_auth(has_secret)?,
            issuer: discovery.issuer,
            resource: discovery.resource,
            authorization_endpoint: server.authorization_endpoint,
            token_endpoint: server.token_endpoint,
            names_issuer_on_return: server.names_issuer_on_return,
            revocation_endpoint: server.revocation_endpoint,
            client_id: client_id.to_string(),
            scopes,
        })
    }
}

/// Why `client_id` cannot be sent as written, if it cannot: visible ASCII
/// characters and spaces (RFC 6749, appendix A.1), within a bound.
pub fn client_id_problem(client_id: &str) -> Option<String> {
    credential_problem(client_id, MAX_CLIENT_ID_CHARS).then(|| {
        format!(
            "Identifiant du client : de 1 à {MAX_CLIENT_ID_CHARS} caractères sans accent ni \
             caractère invisible"
        )
    })
}

/// Why `secret` cannot be sent as written, if it cannot: the same rule
/// (RFC 6749, appendix A.2). The secret itself is never quoted.
pub fn client_secret_problem(secret: &str) -> Option<String> {
    credential_problem(secret, MAX_CLIENT_SECRET_CHARS).then(|| {
        format!(
            "Secret du client : de 1 à {MAX_CLIENT_SECRET_CHARS} caractères sans accent ni \
             caractère invisible"
        )
    })
}

/// Whether `value` breaks the rule of RFC 6749's `VSCHAR`, or is blank at
/// either end, or longer than `max`.
fn credential_problem(value: &str, max: usize) -> bool {
    let length = value.chars().count();
    !(1..=max).contains(&length)
        || value.starts_with(' ')
        || value.ends_with(' ')
        || !value.chars().all(|c| matches!(c, ' '..='~'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_is_written_in_visible_ascii() {
        for good in [
            "mimorya",
            "0oa1b2c3d4",
            "https://mimorya.example/client",
            "a b",
        ] {
            assert_eq!(client_id_problem(good), None, "{good}");
        }
        for bad in [
            "",
            " mimorya",
            "mimorya ",
            "mimoryà",
            "mi\nmorya",
            "mi\u{200b}morya",
        ] {
            assert!(client_id_problem(bad).is_some(), "{bad:?}");
        }
        assert!(client_id_problem(&"a".repeat(MAX_CLIENT_ID_CHARS + 1)).is_some());
        assert_eq!(
            client_secret_problem(&"s".repeat(MAX_CLIENT_SECRET_CHARS)),
            None
        );
        let refused = client_secret_problem("secret\u{202e}").expect("refused");
        assert!(!refused.contains("secret\u{202e}"), "{refused}");
    }
}
