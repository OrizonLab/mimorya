//! The code an authorization's return brings, exchanged for tokens at the
//! token endpoint kept in the declaration (ADR-171): with the PKCE verifier
//! and the same return address (`token_request`). The answer, as a renewal's
//! (`refresh`), is taken only if a server could be sent its access token as
//! written: a Bearer token in the alphabet of RFC 6750, which the platform
//! can mask, one that expires only with a refresh token, and lasts long
//! enough for a session to open.
use serde::Deserialize;

use super::declaration::OAuthDeclaration;
use super::scope::scopes_problem;
use super::token_request::{TokenAnswer, request_tokens};
use crate::mcp::expiry::MIN_GRANTED_LIFE;
use crate::outbound::NetworkPolicy;
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
    /// The scopes granted, if the server says so (else those asked); never
    /// a list it said that could not be read.
    pub scopes: Option<Vec<String>>,
    /// The identity token that came with them, if any (OpenID Connect):
    /// verified only where a declaration asks who authorized (ADR-175).
    pub id_token: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    token_type: Option<String>,
    expires_in: Option<serde_json::Value>,
    refresh_token: Option<String>,
    scope: Option<String>,
    id_token: Option<String>,
}

/// The tokens the token endpoint of `declaration` grants for `exchange`.
pub async fn exchange_code(
    declaration: &OAuthDeclaration,
    exchange: CodeExchange<'_>,
    policy: NetworkPolicy,
) -> Result<Granted, String> {
    let grant = [
        ("grant_type", "authorization_code"),
        ("code", exchange.code),
        ("redirect_uri", exchange.redirect_uri),
        ("code_verifier", exchange.verifier),
    ];
    match request_tokens(declaration, &grant, exchange.client_secret, policy).await {
        TokenAnswer::Granted(body) => granted(&body, false),
        TokenAnswer::Refused { said, .. } | TokenAnswer::Failed(said) => Err(said),
    }
}

/// The tokens of a successful answer, if the platform may keep them;
/// `renewing` one whose refresh token is kept, that may not be given again
/// (RFC 6749 §6).
pub(super) fn granted(body: &[u8], renewing: bool) -> Result<Granted, String> {
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
    if expires_in.is_some() && refresh_token.is_none() && !renewing {
        return Err(
            "Jeton d’accès qui expire sans jeton de renouvellement : la plateforme le perdrait \
             (demandez la portée offline_access si le serveur l’annonce)"
                .into(),
        );
    }
    if expires_in.is_some_and(|seconds| seconds < MIN_GRANTED_LIFE.as_secs()) {
        return Err(format!(
            "Jeton d’accès valable moins de {} minutes : la plateforme n’aurait pas le temps \
             d’ouvrir une session avec lui",
            MIN_GRANTED_LIFE.as_secs() / 60
        ));
    }
    // Not said: those asked (RFC 6749 §5.1); said, kept as the platform
    // could ask them, or refused saying why.
    let scopes = match answer.scope.as_deref() {
        None => None,
        Some(list) => {
            let scopes: Vec<String> = list
                .split(' ')
                .filter(|scope| !scope.is_empty())
                .map(String::from)
                .collect();
            if let Some(problem) = scopes_problem(&scopes) {
                return Err(format!("Portées accordées refusées : {problem}"));
            }
            Some(scopes)
        }
    };
    Ok(Granted {
        access_token,
        expires_in,
        refresh_token,
        scopes,
        id_token: answer.id_token,
    })
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
    use crate::mcp::oauth::ClientAuth;

    #[test]
    fn a_token_is_kept_only_if_it_can_be_sent_and_renewed() {
        let ok = br#"{"access_token":"jeton.abc-123","token_type":"bearer","expires_in":"3600","refresh_token":"r-1","scope":"a b"}"#;
        let kept = granted(ok, false).expect("granted");
        assert_eq!(kept.expires_in, Some(3600));
        assert_eq!(kept.scopes, Some(vec!["a".into(), "b".into()]));
        for (body, said) in [
            (&br#"{"token_type":"Bearer"}"#[..], "sans jeton"),
            (br#"{"access_token":"jeton,abc-123","token_type":"Bearer"}"#, "RFC 6750"),
            (br#"{"access_token":"jeton-abc-123","token_type":"MAC"}"#, "Bearer"),
            (br#"{"access_token":"jeton-abc-123","token_type":"Bearer","expires_in":60}"#, "renouvellement"),
            (br#"{"access_token":"jeton-abc-123","token_type":"Bearer","expires_in":-1,"refresh_token":"r"}"#, "Échéance"),
            (br#"{"access_token":"jeton-abc-123","token_type":"Bearer","expires_in":299,"refresh_token":"r"}"#, "moins de 5 minutes"),
            (b"<html>", "illisible"),
            (br#"{"access_token":"jeton-abc-123","token_type":"Bearer","scope":"a \"b"}"#, "guillemet"),
            (br#"{"access_token":"jeton-abc-123","token_type":"Bearer","scope":"a a"}"#, "deux fois"),
        ] {
            let refused = granted(body, false).err().expect("refused");
            assert!(refused.contains(said), "{refused}");
        }
        // Well formed but more than the platform keeps: said as such.
        let many: Vec<String> = (0..21).map(|i| format!("s{i}")).collect();
        let body = format!(
            r#"{{"access_token":"jeton-abc-123","token_type":"Bearer","scope":"{}"}}"#,
            many.join(" ")
        );
        let refused = granted(body.as_bytes(), false).err().expect("refused");
        assert!(refused.contains("20 portées au plus"), "{refused}");
        // Without an expiry, nothing to renew.
        let lasting = br#"{"access_token":"jeton-abc-123","token_type":"Bearer"}"#;
        assert!(granted(lasting, false).is_ok());
    }

    #[test]
    fn the_exchange_can_be_awaited_by_a_request_handler() {
        fn sendable<T: Send>(_: T) {}
        let declaration = OAuthDeclaration {
            revocation_endpoint: None,
            registered: None,
            holder: Default::default(),
            verifies_identity: false,
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
}
