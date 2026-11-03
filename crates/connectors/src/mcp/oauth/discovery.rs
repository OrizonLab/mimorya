//! Finding where an MCP server is authorized (ADR-171), as the MCP
//! specification does, once, when an administrator declares OAuth: the
//! server is asked to start without a token; its refusal's challenge names
//! its Protected Resource Metadata, followed only on its own origin, else
//! the well-known locations are tried; the metadata names the resource
//! tokens are for and the authorization servers, among which the
//! administrator's issuer, if named, must be; without metadata, that issuer
//! or the server's origin is asked, if the server did refuse; the
//! authorization server's metadata is read last. Each request is bounded
//! (`fetch`), and the whole as well; what fails is said in French, what a
//! server wrote quoted short.

use std::time::Duration;

use url::Url;

use super::challenge::bearer_challenge;
use super::fetch::Fetcher;
use super::protected_resource::{issuer_problem, read_protected_resource};
use super::quoted::quoted;
use super::resource::{canonical_resource, protected_resource_locations, same_origin};
use super::scope::suggested_scopes;
use super::server_metadata::{AuthorizationServer, metadata_locations, read_server_metadata};
use crate::outbound::{NetworkPolicy, url_problem};

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

/// How long the discovery may take: each request, and all of them, the
/// well-known locations being tried one after another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscoveryLimits {
    pub per_request: Duration,
    pub overall: Duration,
}

impl DiscoveryLimits {
    pub const DEFAULT: Self = Self {
        per_request: Duration::from_secs(10),
        overall: Duration::from_secs(30),
    };
}

/// Where the MCP server at `url` is authorized: through `issuer`, if the
/// administrator names one.
pub async fn discover(
    url: &str,
    issuer: Option<&str>,
    policy: NetworkPolicy,
) -> Result<Discovery, String> {
    discover_within(url, issuer, policy, DiscoveryLimits::DEFAULT).await
}

/// `discover`, within these limits.
pub async fn discover_within(
    url: &str,
    issuer: Option<&str>,
    policy: NetworkPolicy,
    limits: DiscoveryLimits,
) -> Result<Discovery, String> {
    let found = discovered(url, issuer, policy, limits.per_request);
    tokio::time::timeout(limits.overall, found)
        .await
        .map_err(|_| {
            let seconds = limits.overall.as_secs_f32().to_string().replace('.', ",");
            format!(
                "Découverte interrompue : le serveur et son serveur d’autorisation n’ont pas \
                 tout dit en {seconds} s"
            )
        })?
}

async fn discovered(
    url: &str,
    issuer: Option<&str>,
    policy: NetworkPolicy,
    per_request: Duration,
) -> Result<Discovery, String> {
    if let Some(problem) = url_problem(url, policy) {
        return Err(problem);
    }
    if let Some(problem) = issuer.and_then(|issuer| issuer_problem(issuer, policy)) {
        return Err(problem);
    }
    let fetcher = Fetcher::new(policy, per_request)?;
    let probe = fetcher.probe(url).await?;
    let challenge = (probe.status == 401)
        .then(|| probe.challenge.as_deref().and_then(bearer_challenge))
        .flatten();
    // The scopes the refusal asks for: the server's own word, even where
    // the metadata it names, elsewhere, is not followed.
    let challenged = challenge
        .as_ref()
        .and_then(|c| c.scope.as_deref())
        .and_then(suggested_scopes)
        .filter(|scopes| !scopes.is_empty());
    // A challenge's metadata on the server's own origin, at an address the
    // platform reaches, else where RFC 9728 publishes it.
    let named = challenge
        .and_then(|c| c.resource_metadata)
        .filter(|metadata| same_origin(url, metadata) && url_problem(metadata, policy).is_none());
    let locations = match named {
        Some(metadata) => vec![metadata],
        None => protected_resource_locations(url),
    };
    let mut protected = None;
    for location in locations {
        if let Some(body) = fetcher.document(&location).await? {
            protected = Some(read_protected_resource(&body, &location, url, policy)?);
            break;
        }
    }
    let (resource, issuer, suggested) = match protected {
        Some(protected) => {
            let issuer = chosen(&protected.authorization_servers, issuer)?;
            let suggested = challenged.unwrap_or(protected.scopes_supported);
            (protected.resource, issuer, suggested)
        }
        None if probe.status != 401 => {
            return Err(format!(
                "« {} » a répondu {} sans demander d’autorisation : ce serveur n’annonce pas \
                 OAuth",
                quoted(url),
                probe.status
            ));
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
        if let Some(body) = fetcher.document(&location).await? {
            let server = read_server_metadata(&body, &issuer, policy)?;
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
