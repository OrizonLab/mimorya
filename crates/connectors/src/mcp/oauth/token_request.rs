//! A request to the token endpoint kept in a declaration (ADR-171), for a
//! code or a refresh token: the resource the tokens are for (RFC 8707), the
//! client proving itself as the declaration says (in the body, in the Basic
//! header, or by its identifier alone), the answer read within its bounds.
//! A refusal (400, 401) is said by its error code, never by what the server
//! wrote around it, and told apart from a server that could not answer; no
//! code, secret nor token is ever in a message.

use std::time::Duration;

use serde::Deserialize;
use url::form_urlencoded::{Serializer, byte_serialize};

use super::declaration::OAuthDeclaration;
use super::fetch::Fetcher;
use super::protected_resource::endpoint_problem;
use super::quoted::quoted;
use super::server_metadata::ClientAuth;
use crate::outbound::NetworkPolicy;

const TIMEOUT: Duration = Duration::from_secs(10);

/// What the token endpoint answered.
pub(super) enum TokenAnswer {
    /// Its body, to be read as granted tokens.
    Granted(Vec<u8>),
    /// Refused (RFC 6749 §5.2): the grant, or the client, is no longer good.
    Refused(String),
    /// Not answered as a token endpoint does, or not reached: why.
    Failed(String),
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: Option<String>,
}

/// Asks the declared token endpoint for tokens with `grant` (its own
/// parameters), the client proving itself with `client_secret` if the
/// declaration says it has one.
pub(super) async fn request_tokens(
    declaration: &OAuthDeclaration,
    grant: &[(&str, &str)],
    client_secret: Option<&str>,
    policy: NetworkPolicy,
) -> TokenAnswer {
    let endpoint = &declaration.token_endpoint;
    if let Some(problem) = endpoint_problem(endpoint, "Point de jeton", policy) {
        return TokenAnswer::Failed(problem);
    }
    let (form, basic) = match form(declaration, grant, client_secret) {
        Ok(built) => built,
        Err(problem) => return TokenAnswer::Failed(problem),
    };
    let fetcher = match Fetcher::new(policy, TIMEOUT) {
        Ok(fetcher) => fetcher,
        Err(problem) => return TokenAnswer::Failed(problem),
    };
    let basic = basic
        .as_ref()
        .map(|(id, secret)| (id.as_str(), secret.as_str()));
    match fetcher.form(endpoint, form, basic).await {
        Ok((200, body)) => TokenAnswer::Granted(body),
        Ok((400 | 401, body)) => TokenAnswer::Refused(refusal(&body)),
        Ok((status, _)) => TokenAnswer::Failed(format!(
            "« {} » a répondu {status} à la demande de jetons",
            quoted(endpoint)
        )),
        Err(problem) => TokenAnswer::Failed(problem),
    }
}

/// The form and the Basic credentials, built whole before anything is
/// awaited: the serializer is not `Send`.
#[allow(clippy::type_complexity)]
fn form(
    declaration: &OAuthDeclaration,
    grant: &[(&str, &str)],
    client_secret: Option<&str>,
) -> Result<(String, Option<(String, String)>), String> {
    let mut form = Serializer::new(String::new());
    form.extend_pairs(grant)
        .append_pair("resource", &declaration.resource);
    let secret = || {
        client_secret
            .ok_or_else(|| "Secret du client absent : déclarez OAuth de nouveau".to_string())
    };
    // RFC 6749 §2.3.1: the identifier and secret form-encoded, then Basic.
    let encoded = |value: &str| byte_serialize(value.as_bytes()).collect::<String>();
    let basic = match declaration.client_auth {
        ClientAuth::None => {
            form.append_pair("client_id", &declaration.client_id);
            None
        }
        ClientAuth::ClientSecretPost => {
            form.append_pair("client_id", &declaration.client_id)
                .append_pair("client_secret", secret()?);
            None
        }
        ClientAuth::ClientSecretBasic => {
            Some((encoded(&declaration.client_id), encoded(secret()?)))
        }
    };
    Ok((form.finish(), basic))
}

/// A refusal as RFC 6749 §5.2 writes one: its error code only, if it reads
/// as one.
pub(super) fn refusal(body: &[u8]) -> String {
    let code = serde_json::from_slice::<ErrorResponse>(body)
        .ok()
        .and_then(|answer| answer.error)
        .filter(|code| {
            (1..=40).contains(&code.len())
                && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        });
    match code {
        Some(code) => format!("Refusée par le serveur d’autorisation ({code})"),
        None => "Refusée par le serveur d’autorisation".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_is_said_by_its_code_alone() {
        let said = refusal(br#"{"error":"invalid_grant","error_description":"Appelez le 01 23"}"#);
        assert_eq!(
            said,
            "Refusée par le serveur d’autorisation (invalid_grant)"
        );
        let odd = refusal(br#"{"error":"Appelez le 01 23 45 67 89 maintenant"}"#);
        assert_eq!(odd, "Refusée par le serveur d’autorisation");
    }
}
