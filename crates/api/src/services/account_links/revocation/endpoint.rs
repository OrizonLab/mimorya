//! Where a provider revokes a grant, and what it answers (ADR-194): the
//! revocation endpoint its OpenID Connect discovery names (Google's does,
//! RFC 8414), read from the issuer its people signed in at, which the
//! declaration must name; one refresh token sent there alone (`token`), as
//! Google takes it, its whole grant going with it. A provider is reached
//! by the client that reaches it for sign-ins (`ProviderHttp`): public
//! addresses only, bounded in time and size. What it wrote around its
//! error code is never repeated, and a token never is.

use openidconnect::{AsyncHttpClient, http};
use serde::Deserialize;

use crate::services::sso::ProviderHttp;

/// What is kept of a provider's discovery here.
#[derive(Deserialize)]
struct Discovery {
    issuer: String,
    revocation_endpoint: Option<String>,
}

/// The revocation endpoint `issuer`'s discovery names, or why there is
/// none (in French).
pub(super) async fn revocation_endpoint(
    http: &ProviderHttp,
    issuer: &str,
    provider: &str,
) -> Result<String, String> {
    let address = format!(
        "{}/.well-known/openid-configuration",
        issuer.trim_end_matches('/')
    );
    let request = http::Request::get(&address)
        .header(http::header::ACCEPT, "application/json")
        .body(Vec::new())
        .map_err(|_| format!("Adresse de {provider} invalide"))?;
    let response = http
        .call(request)
        .await
        .map_err(|error| format!("{provider} n’a pas répondu : {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "{provider} n’a pas dit où révoquer (réponse {})",
            response.status().as_u16()
        ));
    }
    endpoint_in(response.body(), issuer, provider)
}

/// The revocation endpoint a discovery document names, if it is the
/// issuer's own.
fn endpoint_in(body: &[u8], issuer: &str, provider: &str) -> Result<String, String> {
    let discovery: Discovery =
        serde_json::from_slice(body).map_err(|_| format!("Déclaration de {provider} illisible"))?;
    if discovery.issuer != issuer {
        return Err(format!(
            "La déclaration lue n’est pas celle de {provider} : retirez l’accord dans le \
             compte {provider}"
        ));
    }
    discovery.revocation_endpoint.ok_or_else(|| {
        format!(
            "{provider} ne nomme aucun point de révocation : retirez l’accord dans le compte \
             {provider}"
        )
    })
}

/// Asks `endpoint` to revoke the grant of `token`, a refresh token.
pub(super) async fn revoke_token(
    http: &ProviderHttp,
    endpoint: &str,
    token: &str,
    provider: &str,
) -> Result<(), String> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("token", token)
        .finish();
    let request = http::Request::post(endpoint)
        .header(
            http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .header(http::header::ACCEPT, "application/json")
        .body(body.into_bytes())
        .map_err(|_| format!("Point de révocation de {provider} invalide"))?;
    let response = http
        .call(request)
        .await
        .map_err(|error| format!("{provider} n’a pas répondu : {error}"))?;
    answered(response.status().as_u16(), response.body(), provider)
}

/// Whether the provider revoked the grant: it says so, or that the token
/// was no longer one (RFC 7009 answers that as revoked, Google as
/// `invalid_token`): nothing is left to use.
fn answered(status: u16, body: &[u8], provider: &str) -> Result<(), String> {
    if (200..300).contains(&status) {
        return Ok(());
    }
    let code = error_code(body);
    if status == 400 && code.as_deref() == Some("invalid_token") {
        return Ok(());
    }
    Err(match code {
        Some(code) => format!("{provider} a refusé la révocation ({code})"),
        None => format!("{provider} a refusé la révocation (réponse {status})"),
    })
}

/// The error code of an OAuth error answer, if it is one (RFC 6749 §5.2):
/// a short word, never what surrounds it.
fn error_code(body: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    struct Answer {
        error: String,
    }
    let answer: Answer = serde_json::from_slice(body).ok()?;
    let word = answer.error.len() <= 64
        && !answer.error.is_empty()
        && (answer.error.chars()).all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
    word.then_some(answer.error)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ISSUER: &str = "https://accounts.google.com";

    #[test]
    fn the_endpoint_is_the_one_the_issuer_s_own_discovery_names() {
        let body = br#"{"issuer":"https://accounts.google.com",
            "revocation_endpoint":"https://oauth2.googleapis.com/revoke"}"#;
        assert_eq!(
            endpoint_in(body, ISSUER, "Google").as_deref(),
            Ok("https://oauth2.googleapis.com/revoke")
        );
        // Another issuer's: not trusted with the person's token.
        let other = br#"{"issuer":"https://evil.example",
            "revocation_endpoint":"https://evil.example/revoke"}"#;
        assert!(
            endpoint_in(other, ISSUER, "Google")
                .unwrap_err()
                .contains("n’est pas celle de Google")
        );
        // None named: said, the grant to be withdrawn there.
        let none = br#"{"issuer":"https://accounts.google.com"}"#;
        assert!(
            endpoint_in(none, ISSUER, "Google")
                .unwrap_err()
                .contains("aucun point de révocation")
        );
        assert!(endpoint_in(b"<html>", ISSUER, "Google").is_err());
    }

    #[test]
    fn a_token_no_longer_valid_is_revoked_and_a_refusal_says_its_code_only() {
        assert_eq!(answered(200, b"", "Google"), Ok(()));
        assert_eq!(
            answered(400, br#"{"error":"invalid_token"}"#, "Google"),
            Ok(())
        );
        assert_eq!(
            answered(
                400,
                br#"{"error":"invalid_request","error_description":"secret 1234"}"#,
                "Google"
            ),
            Err("Google a refusé la révocation (invalid_request)".into())
        );
        // Not a code: the status only.
        assert_eq!(
            answered(503, br#"{"error":"no <b>way</b>"}"#, "Google"),
            Err("Google a refusé la révocation (réponse 503)".into())
        );
        assert_eq!(
            answered(500, b"jeton-de-renouvellement-x", "Google"),
            Err("Google a refusé la révocation (réponse 500)".into())
        );
    }
}
