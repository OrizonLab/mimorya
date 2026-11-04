//! A request to the token endpoint kept in a declaration (ADR-171), for a
//! code or a refresh token: the resource the tokens are for (RFC 8707), the
//! client proving itself as the declaration says (in the body, in the Basic
//! header, or by its identifier alone), the answer read within its bounds.
//! The revocation endpoint is asked the same way (`post_form`, ADR-172).
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
    /// Refused (RFC 6749 §5.2): its error code, if it reads as one, and
    /// what to say.
    Refused { code: Option<String>, said: String },
    /// Not answered as a token endpoint does, or not reached: why.
    Failed(String),
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: Option<String>,
}

/// An endpoint of the authorization server the client posts a form to.
pub(super) struct FormEndpoint<'a> {
    pub url: &'a str,
    /// How a refusal of its address names it.
    pub named: &'static str,
    /// What was asked, as an unexpected status says it.
    pub asked: &'static str,
}

/// Asks the declared token endpoint for tokens with `grant` (its own
/// parameters), for the declared resource, the client proving itself with
/// `client_secret` if the declaration says it has one.
pub(super) async fn request_tokens(
    declaration: &OAuthDeclaration,
    grant: &[(&str, &str)],
    client_secret: Option<&str>,
    policy: NetworkPolicy,
) -> TokenAnswer {
    let mut params = grant.to_vec();
    params.push(("resource", &declaration.resource));
    let endpoint = FormEndpoint {
        url: &declaration.token_endpoint,
        named: "Point de jeton",
        asked: "la demande de jetons",
    };
    post_form(declaration, &endpoint, &params, client_secret, policy).await
}

/// Posts `params` to `endpoint` as the declaration's client.
pub(super) async fn post_form(
    declaration: &OAuthDeclaration,
    endpoint: &FormEndpoint<'_>,
    params: &[(&str, &str)],
    client_secret: Option<&str>,
    policy: NetworkPolicy,
) -> TokenAnswer {
    let (url, named, asked) = (endpoint.url, endpoint.named, endpoint.asked);
    if let Some(problem) = endpoint_problem(url, named, policy) {
        return TokenAnswer::Failed(problem);
    }
    let (form, basic) = match form(declaration, params, client_secret) {
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
    match fetcher.form(url, form, basic).await {
        Ok((200, body)) => TokenAnswer::Granted(body),
        Ok((400 | 401, body)) => {
            let code = error_code(&body);
            TokenAnswer::Refused {
                said: said(code.as_deref()),
                code,
            }
        }
        Ok((status, _)) => {
            TokenAnswer::Failed(format!("« {} » a répondu {status} à {asked}", quoted(url)))
        }
        Err(problem) => TokenAnswer::Failed(problem),
    }
}

/// The form and the Basic credentials, built whole before anything is
/// awaited: the serializer is not `Send`.
#[allow(clippy::type_complexity)]
fn form(
    declaration: &OAuthDeclaration,
    params: &[(&str, &str)],
    client_secret: Option<&str>,
) -> Result<(String, Option<(String, String)>), String> {
    let mut form = Serializer::new(String::new());
    form.extend_pairs(params);
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

/// The error code of a refusal as RFC 6749 §5.2 writes one, if it reads as
/// one: never what the server wrote around it.
pub(super) fn error_code(body: &[u8]) -> Option<String> {
    serde_json::from_slice::<ErrorResponse>(body)
        .ok()
        .and_then(|answer| answer.error)
        .filter(|code| {
            (1..=40).contains(&code.len())
                && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
}

/// A refusal, said by its code alone.
fn said(code: Option<&str>) -> String {
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
        let body = br#"{"error":"invalid_grant","error_description":"Appelez le 01 23"}"#;
        let code = error_code(body);
        assert_eq!(code.as_deref(), Some("invalid_grant"));
        assert_eq!(
            said(code.as_deref()),
            "Refusée par le serveur d’autorisation (invalid_grant)"
        );
        let odd = error_code(br#"{"error":"Appelez le 01 23 45 67 89 maintenant"}"#);
        assert_eq!(odd, None);
        assert_eq!(said(None), "Refusée par le serveur d’autorisation");
    }
}
