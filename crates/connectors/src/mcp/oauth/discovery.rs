//! Finding where an MCP server is authorized (ADR-171), as the MCP
//! specification does, once, when an administrator declares OAuth: the
//! server is asked to start without a token; its refusal's challenge names
//! its Protected Resource Metadata, followed only on its own origin, else
//! the well-known locations are tried; the metadata names the resource
//! tokens are for and the authorization servers, among which the
//! administrator's issuer, if named, must be; without metadata, that issuer
//! or the server's origin is asked; the authorization server's metadata is
//! read last. Every request goes through the hardened client (public
//! addresses only unless allowed, no redirect, the operator's proxy),
//! bounded in time and size; what fails is said in French, what a server
//! wrote quoted short.

use std::time::Duration;

use reqwest_mcp::header::{ACCEPT, CONTENT_TYPE, WWW_AUTHENTICATE};
use reqwest_mcp::{Client, RequestBuilder};
use url::Url;

use super::challenge::bearer_challenge;
use super::protected_resource::{issuer_problem, read_protected_resource};
use super::quoted::quoted;
use super::resource::{canonical_resource, protected_resource_locations, same_origin};
use super::scope::suggested_scopes;
use super::server_metadata::{AuthorizationServer, metadata_locations, read_server_metadata};
use crate::outbound::{NetworkPolicy, http_client, url_problem};

const TIMEOUT: Duration = Duration::from_secs(10);
/// Metadata documents are a few kilobytes.
const MAX_DOCUMENT_BYTES: usize = 256 * 1024;
/// What starts an MCP session: the server answers it, or says it wants a
/// token.
const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"mimorya","version":"1"}}}"#;

/// What was found of an MCP server's authorization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovery {
    /// The resource tokens are asked for (RFC 8707).
    pub resource: String,
    pub issuer: String,
    pub server: AuthorizationServer,
    /// What the server suggests asking for: its refusal's scopes, else its
    /// metadata's.
    pub suggested_scopes: Vec<String>,
}

/// An answer, bounded.
struct Answer {
    status: u16,
    challenge: Option<String>,
    body: Vec<u8>,
}

/// Where the MCP server at `url` is authorized: through `issuer`, if the
/// administrator names one.
pub async fn discover(
    url: &str,
    issuer: Option<&str>,
    policy: NetworkPolicy,
) -> Result<Discovery, String> {
    if let Some(problem) = url_problem(url, policy) {
        return Err(problem);
    }
    if let Some(problem) = issuer.and_then(|issuer| issuer_problem(issuer, policy)) {
        return Err(problem);
    }
    let client = http_client(policy).map_err(|error| error.to_string())?;
    let probe = client
        .post(url)
        .header(ACCEPT, "application/json, text/event-stream")
        .header(CONTENT_TYPE, "application/json")
        .body(INITIALIZE);
    let refusal = fetch(probe, policy, url).await?;
    let challenge = (refusal.status == 401)
        .then(|| refusal.challenge.as_deref().and_then(bearer_challenge))
        .flatten();
    // A challenge's metadata on the server's own origin, else where RFC 9728
    // publishes it.
    let named = challenge
        .as_ref()
        .and_then(|c| c.resource_metadata.clone())
        .filter(|metadata| same_origin(url, metadata));
    let locations = match named {
        Some(metadata) => vec![metadata],
        None => protected_resource_locations(url),
    };
    let mut protected = None;
    for location in locations {
        let answer = fetch(document(&client, &location), policy, &location).await?;
        if answer.status == 200 {
            protected = Some(read_protected_resource(&answer.body, url, policy)?);
            break;
        }
    }
    let challenged = challenge
        .and_then(|c| c.scope)
        .and_then(|scope| suggested_scopes(&scope))
        .filter(|scopes| !scopes.is_empty());
    let (resource, issuer, suggested) = match protected {
        Some(protected) => {
            let issuer = chosen(&protected.authorization_servers, issuer)?;
            let suggested = challenged.unwrap_or(protected.scopes_supported);
            (protected.resource, issuer, suggested)
        }
        // Without metadata: the administrator's issuer, else the server's
        // origin (the MCP specification of 2025-03-26).
        None => {
            let resource = canonical_resource(url).ok_or("Adresse invalide")?;
            let issuer = match issuer {
                Some(issuer) => issuer.to_string(),
                None => Url::parse(url)
                    .map_err(|_| "Adresse invalide".to_string())?
                    .origin()
                    .ascii_serialization(),
            };
            (resource, issuer, challenged.unwrap_or_default())
        }
    };
    for location in metadata_locations(&issuer) {
        let answer = fetch(document(&client, &location), policy, &location).await?;
        if answer.status == 200 {
            let server = read_server_metadata(&answer.body, &issuer, policy)?;
            return Ok(Discovery {
                resource,
                issuer,
                server,
                suggested_scopes: suggested,
            });
        }
    }
    Err(format!(
        "Métadonnées du serveur d’autorisation « {} » introuvables : nommez son émetteur",
        quoted(&issuer)
    ))
}

/// The issuer among those the metadata names: the administrator's, which
/// must be one of them, else the only one.
fn chosen(servers: &[String], issuer: Option<&str>) -> Result<String, String> {
    match issuer {
        Some(issuer) if servers.iter().any(|s| s == issuer) => Ok(issuer.to_string()),
        Some(issuer) => Err(format!(
            "Ce serveur ne nomme pas « {} » parmi ses serveurs d’autorisation",
            quoted(issuer)
        )),
        None if servers.len() == 1 => Ok(servers[0].clone()),
        None => Err(format!(
            "Ce serveur nomme plusieurs serveurs d’autorisation : choisissez l’émetteur parmi {}",
            servers
                .iter()
                .map(|s| format!("« {} »", quoted(s)))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn document(client: &Client, location: &str) -> RequestBuilder {
    client.get(location).header(ACCEPT, "application/json")
}

/// The answer to `request`, sent to `address`, within its bounds.
async fn fetch(
    request: RequestBuilder,
    policy: NetworkPolicy,
    address: &str,
) -> Result<Answer, String> {
    if let Some(problem) = url_problem(address, policy) {
        return Err(format!("« {} » : {problem}", quoted(address)));
    }
    let unreachable =
        |error: reqwest_mcp::Error| format!("« {} » injoignable ({error})", quoted(address));
    let exchange = async {
        let mut response = request.send().await.map_err(unreachable)?;
        let status = response.status().as_u16();
        let lines: Vec<&str> = response
            .headers()
            .get_all(WWW_AUTHENTICATE)
            .iter()
            .filter_map(|line| line.to_str().ok())
            .collect();
        let challenge = (!lines.is_empty()).then(|| lines.join(", "));
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(unreachable)? {
            if body.len() + chunk.len() > MAX_DOCUMENT_BYTES {
                return Err(format!("Réponse de « {} » trop longue", quoted(address)));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(Answer {
            status,
            challenge,
            body,
        })
    };
    tokio::time::timeout(TIMEOUT, exchange)
        .await
        .map_err(|_| format!("« {} » n’a pas répondu à temps", quoted(address)))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_issuer_is_the_administrator_s_among_those_named_or_the_only_one() {
        let one = vec!["https://a.example".to_string()];
        let two = vec![
            "https://a.example".to_string(),
            "https://b.example".to_string(),
        ];
        assert_eq!(chosen(&one, None).as_deref(), Ok("https://a.example"));
        assert_eq!(
            chosen(&two, Some("https://b.example")).as_deref(),
            Ok("https://b.example")
        );
        assert!(
            chosen(&two, None)
                .expect_err("ambiguous")
                .contains("« https://b.example »")
        );
        assert!(chosen(&one, Some("https://c.example")).is_err());
    }
}
