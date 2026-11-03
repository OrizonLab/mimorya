//! The code an authorization's return brings, exchanged for tokens at the
//! token endpoint kept in the declaration (ADR-171): with the PKCE verifier,
//! the same return address, the resource the tokens are for (RFC 8707), and
//! the client proving itself as the declaration says (in the body, in the
//! Basic header, or by its identifier alone). The answer is read within its
//! bounds and taken only if a server could be sent its access token as
//! written: a Bearer token in the alphabet of RFC 6750, which the platform
//! can mask, and one that expires only with a refresh token. A refusal is
//! said by its error code, never by what the server wrote around it; no
//! code, secret nor token is ever in a message.

use std::time::Duration;

use serde::Deserialize;
use url::form_urlencoded::{Serializer, byte_serialize};

use super::declaration::OAuthDeclaration;
use super::fetch::Fetcher;
use super::protected_resource::endpoint_problem;
use super::quoted::quoted;
use super::scope::suggested_scopes;
use super::server_metadata::ClientAuth;
use crate::outbound::NetworkPolicy;

const TIMEOUT: Duration = Duration::from_secs(10);
/// A JWT access token is a few kilobytes at most.
pub const MAX_OAUTH_TOKEN_CHARS: usize = 8192;

/// What the return brings and the attempt kept.
pub struct CodeExchange<'a> {
    pub code: &'a str,
    pub verifier: &'a str,
    pub redirect_uri: &'a str,
    /// The client's secret, if the declaration says it has one.
    pub client_secret: Option<&'a str>,
}

/// The tokens granted. No `Debug`: they are secrets.
#[derive(Clone, PartialEq, Eq)]
pub struct Granted {
    pub access_token: String,
    /// Seconds from now, if the server says when it expires.
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    /// The scopes granted, if the server says so (else those asked).
    pub scopes: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    token_type: Option<String>,
    expires_in: Option<serde_json::Value>,
    refresh_token: Option<String>,
    scope: Option<String>,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: Option<String>,
}

/// The tokens the token endpoint of `declaration` grants for `exchange`.
pub async fn exchange_code(
    declaration: &OAuthDeclaration,
    exchange: CodeExchange<'_>,
    policy: NetworkPolicy,
) -> Result<Granted, String> {
    let endpoint = &declaration.token_endpoint;
    if let Some(problem) = endpoint_problem(endpoint, "Point de jeton", policy) {
        return Err(problem);
    }
    // Built whole before anything is awaited: the serializer is not `Send`.
    let (form, basic) = {
        let mut form = Serializer::new(String::new());
        form.append_pair("grant_type", "authorization_code")
            .append_pair("code", exchange.code)
            .append_pair("redirect_uri", exchange.redirect_uri)
            .append_pair("code_verifier", exchange.verifier)
            .append_pair("resource", &declaration.resource);
        let secret = || {
            exchange
                .client_secret
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
        (form.finish(), basic)
    };
    let fetcher = Fetcher::new(policy, TIMEOUT)?;
    let basic = basic
        .as_ref()
        .map(|(id, secret)| (id.as_str(), secret.as_str()));
    let (status, body) = fetcher.form(endpoint, form, basic).await?;
    match status {
        200 => granted(&body),
        400 | 401 => Err(refusal(&body)),
        _ => Err(format!(
            "« {} » a répondu {status} à la demande de jetons",
            quoted(endpoint)
        )),
    }
}

/// The tokens of a successful answer, if the platform may keep them.
fn granted(body: &[u8]) -> Result<Granted, String> {
    let answer: TokenResponse = serde_json::from_slice(body)
        .map_err(|_| "Réponse du point de jeton illisible".to_string())?;
    let access_token = answer
        .access_token
        .ok_or("Réponse du point de jeton sans jeton d’accès")?;
    if let Some(problem) = access_token_problem(&access_token) {
        return Err(problem);
    }
    let bearer = answer
        .token_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("bearer"));
    if !bearer {
        return Err(
            "Jeton d’un autre type que Bearer : la plateforme ne sait pas l’envoyer".into(),
        );
    }
    let refresh_token = answer.refresh_token;
    if refresh_token
        .as_deref()
        .is_some_and(|token| !visible_token(token))
    {
        return Err("Jeton de renouvellement illisible".into());
    }
    let expires_in = match answer.expires_in {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::Number(n)) => {
            Some(n.as_u64().ok_or("Échéance du jeton illisible")?)
        }
        Some(serde_json::Value::String(s)) => Some(
            s.parse()
                .map_err(|_| "Échéance du jeton illisible".to_string())?,
        ),
        Some(_) => return Err("Échéance du jeton illisible".into()),
    };
    if expires_in.is_some() && refresh_token.is_none() {
        return Err(
            "Jeton d’accès qui expire sans jeton de renouvellement : la plateforme le perdrait \
             (demandez la portée offline_access si le serveur l’annonce)"
                .into(),
        );
    }
    Ok(Granted {
        access_token,
        expires_in,
        refresh_token,
        scopes: answer.scope.as_deref().and_then(suggested_scopes),
    })
}

/// A refusal as RFC 6749 §5.2 writes one: its error code only, if it reads
/// as one.
fn refusal(body: &[u8]) -> String {
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

/// Why `token` cannot be sent and masked as written, if it cannot: the
/// `b64token` of RFC 6750, within a bound.
pub fn access_token_problem(token: &str) -> Option<String> {
    if !(8..=MAX_OAUTH_TOKEN_CHARS).contains(&token.len()) {
        return Some(format!(
            "Jeton d’accès de 8 à {MAX_OAUTH_TOKEN_CHARS} caractères attendu"
        ));
    }
    let body = token.trim_end_matches('=');
    let well_formed = !body.is_empty()
        && body
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-._~+/".contains(c));
    (!well_formed).then(|| {
        "Jeton d’accès hors de l’alphabet de RFC 6750 : la plateforme ne pourrait le masquer".into()
    })
}

/// Whether a refresh token is visible ASCII, within the bound.
fn visible_token(token: &str) -> bool {
    (1..=MAX_OAUTH_TOKEN_CHARS).contains(&token.len())
        && token.chars().all(|c| c.is_ascii_graphic())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_kept_only_if_it_can_be_sent_and_renewed() {
        let ok = br#"{"access_token":"jeton.abc-123","token_type":"bearer","expires_in":"3600","refresh_token":"r-1","scope":"a b"}"#;
        let kept = granted(ok).expect("granted");
        assert_eq!(kept.expires_in, Some(3600));
        assert_eq!(kept.scopes, Some(vec!["a".into(), "b".into()]));
        for (body, said) in [
            (&br#"{"token_type":"Bearer"}"#[..], "sans jeton"),
            (br#"{"access_token":"jeton,abc-123","token_type":"Bearer"}"#, "RFC 6750"),
            (br#"{"access_token":"jeton-abc-123","token_type":"MAC"}"#, "Bearer"),
            (br#"{"access_token":"jeton-abc-123","token_type":"Bearer","expires_in":60}"#, "renouvellement"),
            (br#"{"access_token":"jeton-abc-123","token_type":"Bearer","expires_in":-1,"refresh_token":"r"}"#, "Échéance"),
            (b"<html>", "illisible"),
        ] {
            let refused = granted(body).err().expect("refused");
            assert!(refused.contains(said), "{refused}");
        }
        // Without an expiry, nothing to renew.
        let lasting = br#"{"access_token":"jeton-abc-123","token_type":"Bearer"}"#;
        assert!(granted(lasting).is_ok());
    }

    #[test]
    fn the_exchange_can_be_awaited_by_a_request_handler() {
        fn sendable<T: Send>(_: T) {}
        let declaration = OAuthDeclaration {
            issuer: "https://auth.example.com".into(),
            resource: "https://mcp.example.com/mcp".into(),
            authorization_endpoint: "https://auth.example.com/authorize".into(),
            token_endpoint: "https://auth.example.com/token".into(),
            names_issuer_on_return: false,
            client_id: "mimorya".into(),
            client_auth: ClientAuth::ClientSecretPost,
            scopes: Vec::new(),
        };
        let exchange = CodeExchange {
            code: "code",
            verifier: "verifier",
            redirect_uri: "https://app.example.com/back",
            client_secret: Some("secret"),
        };
        sendable(exchange_code(
            &declaration,
            exchange,
            NetworkPolicy::default(),
        ));
    }

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
