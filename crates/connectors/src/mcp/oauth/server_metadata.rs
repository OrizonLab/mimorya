//! An authorization server's metadata (RFC 8414, or OpenID Connect
//! discovery): where to send the browser and where to ask for tokens. It is
//! taken only if it names the very issuer it was asked for (RFC 8414 §3.3),
//! if each endpoint is an address the network rules allow (ADR-025), if it
//! offers the authorization code with PKCE S256, which the MCP
//! specification requires, and how the client proves itself is chosen from
//! what it lists, as for the identity providers.

use serde::{Deserialize, Serialize};
use url::Url;

use super::protected_resource::issuer_problem;
use super::scope::scopes_problem;
use crate::outbound::NetworkPolicy;

/// How the platform proves to the token endpoint it is the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientAuth {
    /// A public client: its id only.
    None,
    ClientSecretBasic,
    ClientSecretPost,
}

/// What the platform keeps of the metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationServer {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub registration_endpoint: Option<String>,
    pub revocation_endpoint: Option<String>,
    pub scopes_supported: Vec<String>,
    /// As listed; `None` when the metadata does not say (RFC 8414: then
    /// `client_secret_basic`).
    pub auth_methods: Option<Vec<String>>,
    /// Whether the return names the issuer (RFC 9207), and must then.
    pub names_issuer_on_return: bool,
}

#[derive(Deserialize)]
struct Document {
    issuer: Option<String>,
    authorization_endpoint: Option<String>,
    token_endpoint: Option<String>,
    registration_endpoint: Option<String>,
    revocation_endpoint: Option<String>,
    scopes_supported: Option<Vec<String>>,
    response_types_supported: Option<Vec<String>>,
    code_challenge_methods_supported: Option<Vec<String>>,
    token_endpoint_auth_methods_supported: Option<Vec<String>>,
    #[serde(default)]
    authorization_response_iss_parameter_supported: bool,
}

/// Where the metadata of `issuer` may be, in the order the MCP
/// specification tries them: RFC 8414 with the well-known path inserted,
/// then OpenID Connect discovery inserted, then appended.
pub fn metadata_locations(issuer: &str) -> Vec<String> {
    let Ok(parsed) = Url::parse(issuer) else {
        return Vec::new();
    };
    let origin = parsed.origin().ascii_serialization();
    let path = parsed.path().trim_end_matches('/');
    let mut locations = vec![
        format!("{origin}/.well-known/oauth-authorization-server{path}"),
        format!("{origin}/.well-known/openid-configuration{path}"),
    ];
    if !path.is_empty() {
        locations.push(format!("{origin}{path}/.well-known/openid-configuration"));
    }
    locations
}

/// The metadata in `body`, if it is that of `issuer` and the platform can
/// use it.
pub fn read_server_metadata(
    body: &[u8],
    issuer: &str,
    policy: NetworkPolicy,
) -> Result<AuthorizationServer, String> {
    let document: Document = serde_json::from_slice(body)
        .map_err(|_| "Métadonnées du serveur d’autorisation illisibles".to_string())?;
    if document.issuer.as_deref() != Some(issuer) {
        return Err(format!(
            "Les métadonnées du serveur d’autorisation nomment un autre émetteur que « {issuer} »"
        ));
    }
    let endpoint = |name: &str, url: Option<String>| -> Result<String, String> {
        let url =
            url.ok_or_else(|| format!("Le serveur d’autorisation ne donne pas son {name}"))?;
        match issuer_problem(&url, policy) {
            Some(problem) => Err(format!("{name} : {problem}")),
            None => Ok(url),
        }
    };
    let authorization_endpoint = endpoint("point d’autorisation", document.authorization_endpoint)?;
    let token_endpoint = endpoint("point de jeton", document.token_endpoint)?;
    let registration_endpoint = document
        .registration_endpoint
        .map(|url| endpoint("point d’enregistrement", Some(url)))
        .transpose()?;
    let revocation_endpoint = document
        .revocation_endpoint
        .map(|url| endpoint("point de révocation", Some(url)))
        .transpose()?;
    if !lists(&document.response_types_supported, "code") {
        return Err("Le serveur d’autorisation n’offre pas le code d’autorisation".into());
    }
    if !lists(&document.code_challenge_methods_supported, "S256") {
        return Err(
            "Le serveur d’autorisation n’offre pas PKCE avec S256, que la spécification MCP \
             exige"
                .into(),
        );
    }
    let scopes = document.scopes_supported.unwrap_or_default();
    Ok(AuthorizationServer {
        issuer: issuer.to_string(),
        authorization_endpoint,
        token_endpoint,
        registration_endpoint,
        revocation_endpoint,
        scopes_supported: if scopes_problem(&scopes).is_none() {
            scopes
        } else {
            Vec::new()
        },
        auth_methods: document.token_endpoint_auth_methods_supported,
        names_issuer_on_return: document.authorization_response_iss_parameter_supported,
    })
}

fn lists(values: &Option<Vec<String>>, wanted: &str) -> bool {
    values
        .as_ref()
        .is_some_and(|values| values.iter().any(|v| v == wanted))
}

impl AuthorizationServer {
    /// How a client with or without a secret proves itself: in the request
    /// body when the server lists it (some refuse the encoded form of a
    /// secret in the Basic header), else in that header; a public client
    /// only if the server lists `none`.
    pub fn client_auth(&self, has_secret: bool) -> Result<ClientAuth, String> {
        let listed = |method: &str| {
            self.auth_methods
                .as_ref()
                .is_some_and(|methods| methods.iter().any(|m| m == method))
        };
        if !has_secret {
            return if listed("none") {
                Ok(ClientAuth::None)
            } else {
                Err(
                    "Ce serveur d’autorisation n’accepte pas de client public : donnez le secret \
                     du client"
                        .into(),
                )
            };
        }
        if listed("client_secret_post") {
            Ok(ClientAuth::ClientSecretPost)
        } else if self.auth_methods.is_none() || listed("client_secret_basic") {
            Ok(ClientAuth::ClientSecretBasic)
        } else {
            Err(
                "Ce serveur d’autorisation n’accepte pas de secret du client : laissez-le vide \
                 s’il accepte un client public"
                    .into(),
            )
        }
    }

    /// Whether the server issues tokens that can be renewed when asked for
    /// `offline_access`.
    pub fn offers_offline_access(&self) -> bool {
        self.scopes_supported.iter().any(|s| s == "offline_access")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const ISSUER: &str = "https://auth.example.com/t1";

    fn metadata() -> Value {
        json!({
            "issuer": ISSUER,
            "authorization_endpoint": "https://auth.example.com/t1/authorize",
            "token_endpoint": "https://auth.example.com/t1/token",
            "response_types_supported": ["code"],
            "code_challenge_methods_supported": ["plain", "S256"],
            "scopes_supported": ["files:read", "offline_access"],
        })
    }

    fn read(document: &Value) -> Result<AuthorizationServer, String> {
        read_server_metadata(
            document.to_string().as_bytes(),
            ISSUER,
            NetworkPolicy::default(),
        )
    }

    #[test]
    fn the_metadata_is_looked_for_where_the_specification_says() {
        assert_eq!(
            metadata_locations(ISSUER),
            [
                "https://auth.example.com/.well-known/oauth-authorization-server/t1",
                "https://auth.example.com/.well-known/openid-configuration/t1",
                "https://auth.example.com/t1/.well-known/openid-configuration",
            ]
        );
        assert_eq!(metadata_locations("https://auth.example.com").len(), 2);
    }

    #[test]
    fn the_metadata_of_this_issuer_is_read() {
        let server = read(&metadata()).expect("read");
        assert_eq!(server.token_endpoint, "https://auth.example.com/t1/token");
        assert!(server.offers_offline_access());
        assert!(!server.names_issuer_on_return);
    }

    #[test]
    fn another_issuer_an_unreachable_endpoint_or_no_s256_is_refused() {
        let mut cases = Vec::new();
        for (field, value) in [
            ("issuer", json!("https://auth.example.com")),
            ("issuer", json!("https://evil.example.com/t1")),
            ("token_endpoint", json!("http://auth.example.com/token")),
            ("token_endpoint", json!("https://169.254.169.254/token")),
            (
                "authorization_endpoint",
                json!("https://auth.example.com/a#f"),
            ),
            ("registration_endpoint", json!("https://10.0.0.1/register")),
            ("code_challenge_methods_supported", json!(["plain"])),
            ("response_types_supported", json!(["token"])),
        ] {
            let mut document = metadata();
            document[field] = value;
            cases.push(document);
        }
        let mut without_s256 = metadata();
        without_s256
            .as_object_mut()
            .unwrap()
            .remove("code_challenge_methods_supported");
        cases.push(without_s256);
        for document in cases {
            assert!(read(&document).is_err(), "{document}");
        }
    }

    #[test]
    fn the_client_proves_itself_as_the_server_lists() {
        let mut server = read(&metadata()).expect("read");
        assert_eq!(server.client_auth(true), Ok(ClientAuth::ClientSecretBasic));
        assert!(server.client_auth(false).is_err());
        server.auth_methods = Some(vec![
            "client_secret_basic".into(),
            "client_secret_post".into(),
        ]);
        assert_eq!(server.client_auth(true), Ok(ClientAuth::ClientSecretPost));
        server.auth_methods = Some(vec!["none".into()]);
        assert_eq!(server.client_auth(false), Ok(ClientAuth::None));
        assert!(server.client_auth(true).is_err());
    }
}
