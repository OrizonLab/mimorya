//! The client an MCP server's OAuth declaration names (ADR-171, ADR-173):
//! one an administrator registered at the authorization server by hand and
//! types, checked as RFC 6749 writes it, or one the platform registers
//! there itself (RFC 7591), once the discovery has found the server, for
//! the platform's return address and the scopes to be asked for; then
//! only if the platform can seal what it is granted, before anything is
//! sent. Its secret, typed or granted, is handed back to be sealed apart,
//! never in the declaration.

use mimorya_connectors::mcp::oauth::{
    Discovery, OAuthDeclaration, asked_scopes, client_id_problem, client_secret_problem,
    register_client,
};
use mimorya_connectors::outbound::NetworkPolicy;
use mimorya_core::DomainError;

/// The client an administrator declares.
pub enum OAuthClient {
    /// Registered by hand at the authorization server: its identifier and,
    /// unless it is public, its secret.
    Typed {
        client_id: String,
        client_secret: Option<String>,
    },
    /// Registered by the platform itself.
    Register,
}

impl OAuthClient {
    /// The client a declaration's body names: the platform's own when it
    /// asks to `register`, then with no identifier nor secret given.
    pub fn chosen(
        register: bool,
        client_id: Option<String>,
        client_secret: Option<String>,
    ) -> Result<Self, DomainError> {
        match (register, client_id) {
            (true, None) if client_secret.is_none() => Ok(Self::Register),
            (true, _) => Err(DomainError::validation(
                "L’enregistrement dynamique choisit lui-même l’identifiant et le secret du \
                 client : ne les donnez pas",
            )),
            (false, Some(client_id)) => Ok(Self::Typed {
                client_id,
                client_secret,
            }),
            (false, None) => Err(DomainError::validation(
                "Identifiant du client requis, sauf si la plateforme s’enregistre elle-même",
            )),
        }
    }

    /// Why it cannot be declared as written, if it cannot: never quoting a
    /// secret.
    pub fn problem(&self) -> Option<String> {
        match self {
            Self::Typed {
                client_id,
                client_secret,
            } => client_id_problem(client_id)
                .or_else(|| client_secret.as_deref().and_then(client_secret_problem)),
            Self::Register => None,
        }
    }

    /// Whether the platform must be able to seal secrets to declare it: a
    /// typed secret, or whatever registering grants (and the tokens, which
    /// the platform keeps sealed).
    pub fn needs_sealing(&self) -> bool {
        matches!(
            self,
            Self::Typed {
                client_secret: Some(_),
                ..
            } | Self::Register
        )
    }
}

/// What the declaration of `client` at the server `found` needs.
pub struct Declared<'a> {
    pub scopes: Option<Vec<String>>,
    /// The platform's return address, which a registered client is given.
    pub redirect_uri: &'a str,
    pub network: NetworkPolicy,
}

/// The declaration of `client` at the server `found`, and the secret to
/// seal, if any: the platform's own client registered there first.
pub async fn declared(
    found: Discovery,
    client: OAuthClient,
    with: Declared<'_>,
) -> Result<(OAuthDeclaration, Option<String>), String> {
    match client {
        OAuthClient::Typed {
            client_id,
            client_secret,
        } => {
            let has_secret = client_secret.is_some();
            let declaration = OAuthDeclaration::new(found, &client_id, has_secret, with.scopes)?;
            Ok((declaration, client_secret))
        }
        OAuthClient::Register => {
            let scopes = asked_scopes(&found, with.scopes)?;
            let registration =
                register_client(&found.server, with.redirect_uri, &scopes, with.network).await?;
            let declaration = OAuthDeclaration::registered(found, &registration, scopes);
            Ok((declaration, registration.client_secret))
        }
    }
}
