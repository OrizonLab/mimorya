//! What an administrator declares of an MCP server's OAuth (ADR-171), kept
//! in the connector's configuration: where the discovery found it is
//! authorized (the resource tokens are for, the issuer and its endpoints,
//! whether its return names the issuer), the client the company registered
//! there or the platform registered itself (ADR-173), how that client
//! proves itself, and the scopes it asks for. The
//! client's secret is not here: it is sealed apart, and only whether there
//! is one decides how the client proves itself. Until it is authorized, a
//! declared server starts no session.

use serde::{Deserialize, Serialize};

use super::discovery::Discovery;
use super::registration::Registration;
use super::scope::scopes_problem;
use super::server_metadata::ClientAuth;

/// What the connector says until an administrator authorizes it.
pub const OAUTH_TO_AUTHORIZE: &str = "Autorisation OAuth à donner : un administrateur doit \
     autoriser ce connecteur auprès de son serveur d’autorisation";

/// What a server each person authorizes for themself says when started
/// without a person's authorization (ADR-174): no session opens with the
/// company's, only with that of the person a run serves.
pub const OAUTH_EACH_PERSON: &str = "Autorisation OAuth donnée par chaque personne pour \
     elle-même : aucune session ne s’ouvre sans celle de la personne servie";

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
    /// The client, if the platform registered it itself (RFC 7591,
    /// ADR-173): absent for one an administrator typed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registered: Option<RegisteredClient>,
    /// Who authorizes the server (ADR-174): the company, absent from the
    /// declarations kept before each person could, and written only for
    /// each person, so that a process that knows only the company's reads
    /// such a declaration as unreadable and starts nothing.
    #[serde(default, skip_serializing_if = "OAuthHolder::is_company")]
    pub holder: OAuthHolder,
    /// Whether the authorization server must prove, by an identity token,
    /// that each person authorizes with their own account (ADR-175): only
    /// when each person authorizes. Written only when true, so that a
    /// process that does not know it reads the declaration as unreadable
    /// rather than keep an authorization unverified.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub verifies_identity: bool,
}

/// Who an MCP server's OAuth authorization is given by (ADR-174): an
/// administrator once for the company, every run then acting under the
/// account that authorized (ADR-171), or each person for themself, a run
/// then acting under the account of the person it serves.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OAuthHolder {
    #[default]
    Company,
    Person,
}

impl OAuthHolder {
    pub fn is_company(&self) -> bool {
        *self == Self::Company
    }
}

/// What a declaration keeps of the client the platform registered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisteredClient {
    /// When its secret expires, in seconds since the epoch, if it does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_expires_at: Option<u64>,
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
        let scopes = asked_scopes(&discovery, scopes)?;
        let client_auth = discovery.server.client_auth(has_secret)?;
        Ok(Self::of(discovery, client_id, client_auth, scopes, None))
    }

    /// The declaration of the client the platform registered at the
    /// authorization server `discovery` found, for `scopes`.
    pub fn registered(
        discovery: Discovery,
        registration: &Registration,
        scopes: Vec<String>,
    ) -> Self {
        let registered = RegisteredClient {
            secret_expires_at: registration.secret_expires_at,
        };
        let (client_id, client_auth) = (&registration.client_id, registration.client_auth);
        Self::of(discovery, client_id, client_auth, scopes, Some(registered))
    }

    fn of(
        discovery: Discovery,
        client_id: &str,
        client_auth: ClientAuth,
        scopes: Vec<String>,
        registered: Option<RegisteredClient>,
    ) -> Self {
        let server = discovery.server;
        Self {
            client_auth,
            issuer: discovery.issuer,
            resource: discovery.resource,
            authorization_endpoint: server.authorization_endpoint,
            token_endpoint: server.token_endpoint,
            names_issuer_on_return: server.names_issuer_on_return,
            revocation_endpoint: server.revocation_endpoint,
            client_id: client_id.to_string(),
            scopes,
            registered,
            holder: OAuthHolder::Company,
            verifies_identity: false,
        }
    }
}

/// The scopes to ask for: `scopes`, if they can be sent, else those the
/// server suggests.
pub fn asked_scopes(
    discovery: &Discovery,
    scopes: Option<Vec<String>>,
) -> Result<Vec<String>, String> {
    match scopes {
        Some(scopes) => match scopes_problem(&scopes) {
            Some(problem) => Err(problem),
            None => Ok(scopes),
        },
        None => Ok(discovery.suggested_scopes.clone()),
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
