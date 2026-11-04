//! The platform registered as a client at an MCP server's authorization
//! server (RFC 7591, ADR-173), when an administrator asks for it rather
//! than typing a client registered by hand: at the registration endpoint
//! the server's metadata names, for the platform's return address alone,
//! the authorization code and refresh token grants and the scopes to be
//! asked for, the client proving itself with a secret if the platform can
//! seal one and the server takes one, else by its identifier alone. The
//! answer is read within its bounds and checked as a typed client would
//! be: an identifier and a secret RFC 6749 can carry, a way to prove itself
//! the platform knows and that matches whether a secret came, the return
//! address kept, a secret lasting at least a day. A refusal is said by its
//! error code; no secret is ever in a message.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::declaration::{client_id_problem, client_secret_problem};
use super::fetch::Fetcher;
use super::protected_resource::endpoint_problem;
use super::quoted::quoted;
use super::server_metadata::{AuthorizationServer, ClientAuth};
use super::token_request::error_code;
use crate::outbound::NetworkPolicy;

/// The name the authorization server shows for the platform's client.
pub const CLIENT_NAME: &str = "Mimorya";

/// What is said when the server offers no registration.
pub const NO_REGISTRATION_ENDPOINT: &str = "Ce serveur d’autorisation n’offre pas \
     l’enregistrement dynamique des clients : enregistrez le client chez lui et saisissez son \
     identifiant";

/// What is said when the server registers only with an initial token.
pub const REGISTRATION_TOKEN_NEEDED: &str = "Ce serveur d’autorisation n’enregistre de client \
     qu’avec un jeton d’enregistrement initial : enregistrez le client chez lui et saisissez son \
     identifiant";

const TIMEOUT: Duration = Duration::from_secs(10);

/// A secret expiring sooner is refused: the authorization would not last.
const MIN_SECRET_LIFE_SECONDS: u64 = 24 * 3600;

/// The client the platform registered. Never printed: it holds a secret.
pub struct Registration {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub client_auth: ClientAuth,
    /// When the secret expires, in seconds since the epoch, if it does.
    pub secret_expires_at: Option<u64>,
}

#[derive(Serialize)]
struct Request<'a> {
    client_name: &'a str,
    redirect_uris: [&'a str; 1],
    grant_types: [&'a str; 2],
    response_types: [&'a str; 1],
    token_endpoint_auth_method: ClientAuth,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<String>,
}

#[derive(Deserialize)]
struct Answer {
    client_id: Option<String>,
    client_secret: Option<String>,
    token_endpoint_auth_method: Option<serde_json::Value>,
    client_secret_expires_at: Option<u64>,
    redirect_uris: Option<Vec<String>>,
}

/// Registers the platform at `server` for `redirect_uri` and `scopes`, the
/// client proving itself with a secret only if `can_keep_secret`.
pub async fn register_client(
    server: &AuthorizationServer,
    redirect_uri: &str,
    scopes: &[String],
    can_keep_secret: bool,
    policy: NetworkPolicy,
) -> Result<Registration, String> {
    let Some(endpoint) = server.registration_endpoint.as_deref() else {
        return Err(NO_REGISTRATION_ENDPOINT.into());
    };
    if let Some(problem) = endpoint_problem(endpoint, "Point d’enregistrement", policy) {
        return Err(problem);
    }
    let asked = asked_auth(server, can_keep_secret)?;
    let request = Request {
        client_name: CLIENT_NAME,
        redirect_uris: [redirect_uri],
        grant_types: ["authorization_code", "refresh_token"],
        response_types: ["code"],
        token_endpoint_auth_method: asked,
        scope: (!scopes.is_empty()).then(|| scopes.join(" ")),
    };
    let document = serde_json::to_string(&request)
        .map_err(|_| "Demande d’enregistrement impossible à écrire".to_string())?;
    let (status, body) = Fetcher::new(policy, TIMEOUT)?
        .json(endpoint, document)
        .await?;
    match status {
        200 | 201 => read_answer(&body, asked, redirect_uri, now()),
        400 => Err(match error_code(&body) {
            Some(code) => format!("Enregistrement refusé par le serveur d’autorisation ({code})"),
            None => "Enregistrement refusé par le serveur d’autorisation".into(),
        }),
        401 | 403 => Err(REGISTRATION_TOKEN_NEEDED.into()),
        status => Err(format!(
            "« {} » a répondu {status} à la demande d’enregistrement",
            quoted(endpoint)
        )),
    }
}

/// How the registered client is to prove itself: with a secret the
/// platform can keep if the server takes one, else by its identifier.
fn asked_auth(server: &AuthorizationServer, can_keep_secret: bool) -> Result<ClientAuth, String> {
    if can_keep_secret && let Ok(auth) = server.client_auth(true) {
        return Ok(auth);
    }
    server.client_auth(false).map_err(|_| {
        if can_keep_secret {
            "Ce serveur d’autorisation n’offre aucune façon de prouver un client que la \
             plateforme sache tenir"
                .to_string()
        } else {
            "Ce serveur d’autorisation n’enregistre que des clients à secret, que la plateforme \
             ne peut garder sans sa clé de chiffrement"
                .to_string()
        }
    })
}

/// The registration the answer `body` gives, if the platform can keep it.
fn read_answer(
    body: &[u8],
    asked: ClientAuth,
    redirect_uri: &str,
    now: u64,
) -> Result<Registration, String> {
    let unusable = |what: &str| format!("La réponse d’enregistrement {what}");
    let answer: Answer = serde_json::from_slice(body).map_err(|_| unusable("est illisible"))?;
    let client_id = answer
        .client_id
        .ok_or_else(|| unusable("ne donne pas d’identifiant de client"))?;
    if client_id_problem(&client_id).is_some() {
        return Err(unusable(
            "donne un identifiant de client que la plateforme ne peut envoyer",
        ));
    }
    if let Some(secret) = &answer.client_secret
        && client_secret_problem(secret).is_some()
    {
        return Err(unusable(
            "donne un secret que la plateforme ne peut envoyer",
        ));
    }
    // As registered, if said; else as asked (RFC 7591 §3.2.1).
    let client_auth = match answer.token_endpoint_auth_method {
        Some(method) => serde_json::from_value::<ClientAuth>(method).map_err(|_| {
            unusable("nomme une façon de prouver le client que la plateforme ne connaît pas")
        })?,
        None => asked,
    };
    if (client_auth == ClientAuth::None) != answer.client_secret.is_none() {
        return Err(unusable(
            "ne s’accorde pas : le secret et la façon de prouver le client se contredisent",
        ));
    }
    if answer
        .redirect_uris
        .is_some_and(|kept| !kept.iter().any(|uri| uri == redirect_uri))
    {
        return Err(unusable(
            "ne garde pas l’adresse de retour de la plateforme",
        ));
    }
    // 0: the secret does not expire.
    let secret_expires_at = answer
        .client_secret_expires_at
        .filter(|at| *at != 0 && answer.client_secret.is_some());
    if secret_expires_at.is_some_and(|at| at < now.saturating_add(MIN_SECRET_LIFE_SECONDS)) {
        return Err(
            "Le secret accordé par le serveur d’autorisation expire avant un jour : enregistrez \
             le client chez lui et saisissez son identifiant"
                .into(),
        );
    }
    Ok(Registration {
        client_id,
        client_secret: answer.client_secret,
        client_auth,
        secret_expires_at,
    })
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BACK: &str = "https://app.example.com/api/admin/connectors/oauth/callback";
    const NOW: u64 = 1_800_000_000;

    fn read(body: &str, asked: ClientAuth) -> Result<Registration, String> {
        read_answer(body.as_bytes(), asked, BACK, NOW)
    }

    #[test]
    fn a_registered_client_is_kept_as_the_server_says() {
        let confidential = read(
            r#"{"client_id":"c1","client_secret":"s3cret","token_endpoint_auth_method":"client_secret_basic","client_secret_expires_at":0}"#,
            ClientAuth::ClientSecretPost,
        )
        .expect("kept");
        assert_eq!(confidential.client_id, "c1");
        assert_eq!(confidential.client_auth, ClientAuth::ClientSecretBasic);
        assert_eq!(confidential.secret_expires_at, None);
        // Not said: as asked.
        let public = read(r#"{"client_id":"c2"}"#, ClientAuth::None).expect("kept");
        assert_eq!(public.client_auth, ClientAuth::None);
        let lasting = format!(
            r#"{{"client_id":"c3","client_secret":"s","client_secret_expires_at":{}}}"#,
            NOW + 2 * MIN_SECRET_LIFE_SECONDS
        );
        let lasting = read(&lasting, ClientAuth::ClientSecretPost).expect("kept");
        assert_eq!(
            lasting.secret_expires_at,
            Some(NOW + 2 * MIN_SECRET_LIFE_SECONDS)
        );
    }

    #[test]
    fn an_answer_the_platform_cannot_keep_is_refused_without_its_secret() {
        let soon = format!(
            r#"{{"client_id":"c","client_secret":"s3cret","client_secret_expires_at":{}}}"#,
            NOW + 60
        );
        for (body, asked) in [
            ("<html>", ClientAuth::None),
            (
                r#"{"client_secret":"s3cret"}"#,
                ClientAuth::ClientSecretPost,
            ),
            (r#"{"client_id":"clïent"}"#, ClientAuth::None),
            (
                r#"{"client_id":"c","client_secret":"s3cret\n"}"#,
                ClientAuth::ClientSecretPost,
            ),
            (
                r#"{"client_id":"c","client_secret":"s3cret","token_endpoint_auth_method":"private_key_jwt"}"#,
                ClientAuth::ClientSecretPost,
            ),
            (
                r#"{"client_id":"c","client_secret":"s3cret"}"#,
                ClientAuth::None,
            ),
            (
                r#"{"client_id":"c","token_endpoint_auth_method":"client_secret_post"}"#,
                ClientAuth::None,
            ),
            (
                r#"{"client_id":"c","redirect_uris":["https://evil.example/back"]}"#,
                ClientAuth::None,
            ),
            (&soon, ClientAuth::ClientSecretPost),
        ] {
            let refused = read(body, asked).err().unwrap_or_else(|| panic!("{body}"));
            assert!(!refused.contains("s3cret"), "{refused}");
        }
    }
}
