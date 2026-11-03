//! An MCP server's Protected Resource Metadata (RFC 9728): which
//! authorization servers issue its tokens, and the scopes it supports. The
//! document comes from the server: it is taken only if it names this very
//! server as its resource (RFC 9728 §3.3, a document copied from another
//! resource is refused), between one and ten authorization servers, each
//! an address the network rules allow (ADR-025). Its scopes are only
//! suggestions: kept if each could be asked for, else none.

use serde::Deserialize;

use url::Url;

use super::quoted::quoted;
use super::resource::same_resource;
use super::scope::scopes_problem;
use mimorya_core::plain_text::is_hidden;

use crate::outbound::{MAX_URL_CHARS, NetworkPolicy, url_problem};

pub const MAX_AUTHORIZATION_SERVERS: usize = 10;

/// What the platform keeps of the metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedResource {
    pub authorization_servers: Vec<String>,
    pub scopes_supported: Vec<String>,
}

/// The fields read; any other is passed over (the format is extensible).
#[derive(Deserialize)]
struct Document {
    resource: Option<String>,
    authorization_servers: Option<Vec<String>>,
    scopes_supported: Option<Vec<String>>,
}

/// The metadata in `body`, if it is that of `resource` and names
/// authorization servers the platform may reach.
pub fn read_protected_resource(
    body: &[u8],
    resource: &str,
    policy: NetworkPolicy,
) -> Result<ProtectedResource, String> {
    let document: Document = serde_json::from_slice(body)
        .map_err(|_| "Métadonnées de la ressource protégée illisibles".to_string())?;
    if !document
        .resource
        .as_deref()
        .is_some_and(|named| same_resource(named, resource))
    {
        return Err(
            "Les métadonnées de la ressource protégée nomment une autre ressource que ce serveur"
                .into(),
        );
    }
    let servers = document.authorization_servers.unwrap_or_default();
    if servers.is_empty() {
        return Err(
            "Les métadonnées de la ressource protégée ne nomment aucun serveur \
                    d’autorisation"
                .into(),
        );
    }
    if servers.len() > MAX_AUTHORIZATION_SERVERS {
        return Err(format!(
            "Les métadonnées de la ressource protégée nomment plus de \
             {MAX_AUTHORIZATION_SERVERS} serveurs d’autorisation"
        ));
    }
    for server in &servers {
        if let Some(problem) = issuer_problem(server, policy) {
            return Err(problem);
        }
    }
    let scopes = document.scopes_supported.unwrap_or_default();
    Ok(ProtectedResource {
        authorization_servers: servers,
        scopes_supported: if scopes_problem(&scopes).is_none() {
            scopes
        } else {
            Vec::new()
        },
    })
}

/// Why an address a server wrote cannot be kept as written, if it cannot:
/// longer than any address the platform reaches (its parameters counted),
/// or holding a blank or a hidden character, which the address parser would
/// drop or encode and the administrator would not see.
fn written_problem(address: &str) -> Option<String> {
    if address.chars().count() > MAX_URL_CHARS {
        return Some(format!(
            "Adresse trop longue ({MAX_URL_CHARS} caractères au plus)"
        ));
    }
    if address.chars().any(|c| c.is_whitespace() || is_hidden(c)) {
        return Some("Adresse avec un blanc ou un caractère caché".to_string());
    }
    None
}

/// Why `issuer` cannot name an authorization server the platform reaches,
/// if it cannot: without parameters, fragment nor credentials (RFC 8414
/// §2), at an address the network rules allow. The issuer is quoted short.
pub fn issuer_problem(issuer: &str, policy: NetworkPolicy) -> Option<String> {
    let problem = match Url::parse(issuer) {
        _ if let Some(problem) = written_problem(issuer) => problem,
        Ok(parsed)
            if parsed.query().is_some()
                || parsed.fragment().is_some()
                || !parsed.username().is_empty()
                || parsed.password().is_some() =>
        {
            "un émetteur n’a ni paramètres, ni fragment, ni identifiants (RFC 8414)".to_string()
        }
        _ => url_problem(issuer, policy)?,
    };
    Some(format!(
        "Serveur d’autorisation « {} » : {problem}",
        quoted(issuer)
    ))
}

/// Why `url`, an endpoint of an authorization server (`name`), cannot be
/// reached, if it cannot: parameters are allowed (RFC 6749 §3.1), neither
/// a fragment nor credentials; the address otherwise as the network rules
/// allow. The address is quoted short.
pub fn endpoint_problem(url: &str, name: &str, policy: NetworkPolicy) -> Option<String> {
    let problem = match Url::parse(url) {
        _ if let Some(problem) = written_problem(url) => problem,
        Ok(parsed)
            if parsed.fragment().is_some()
                || !parsed.username().is_empty()
                || parsed.password().is_some() =>
        {
            "ni fragment ni identifiants attendus".to_string()
        }
        Ok(mut parsed) => {
            parsed.set_query(None);
            url_problem(parsed.as_str(), policy)?
        }
        Err(_) => url_problem(url, policy)?,
    };
    Some(format!("{name} « {} » : {problem}", quoted(url)))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const RESOURCE: &str = "https://mcp.example.com/mcp";

    fn read(document: serde_json::Value) -> Result<ProtectedResource, String> {
        read_protected_resource(
            document.to_string().as_bytes(),
            RESOURCE,
            NetworkPolicy::default(),
        )
    }

    #[test]
    fn the_metadata_of_this_server_names_its_authorization_servers() {
        let read = read(json!({
            "resource": "https://MCP.example.com:443/mcp",
            "authorization_servers": ["https://auth.example.com", "https://auth.example.com/t1"],
            "scopes_supported": ["files:read"],
            "bearer_methods_supported": ["header"],
        }));
        assert_eq!(
            read,
            Ok(ProtectedResource {
                authorization_servers: vec![
                    "https://auth.example.com".into(),
                    "https://auth.example.com/t1".into()
                ],
                scopes_supported: vec!["files:read".into()],
            })
        );
    }

    #[test]
    fn metadata_of_another_resource_or_without_servers_is_refused() {
        let servers = json!(["https://auth.example.com"]);
        for document in [
            json!({ "resource": "https://mcp.example.com/other", "authorization_servers": servers }),
            json!({ "resource": "https://evil.example.com/mcp", "authorization_servers": servers }),
            json!({ "authorization_servers": servers }),
            json!({ "resource": RESOURCE }),
            json!({ "resource": RESOURCE, "authorization_servers": [] }),
        ] {
            assert!(read(document.clone()).is_err(), "{document}");
        }
        assert!(read_protected_resource(b"<html>", RESOURCE, NetworkPolicy::default()).is_err());
        let many: Vec<String> = (0..=MAX_AUTHORIZATION_SERVERS)
            .map(|i| format!("https://auth{i}.example.com"))
            .collect();
        assert!(read(json!({ "resource": RESOURCE, "authorization_servers": many })).is_err());
    }

    #[test]
    fn an_authorization_server_the_network_rules_forbid_is_refused() {
        for issuer in [
            "http://auth.example.com",
            "https://10.0.0.1",
            "https://auth.example.com/?tenant=1",
            "https://auth.example.com/#f",
            "auth.example.com",
        ] {
            let document = json!({ "resource": RESOURCE, "authorization_servers": [issuer] });
            assert!(read(document).is_err(), "{issuer}");
        }
        // A private deployment reaches its own.
        let private = NetworkPolicy {
            allow_private_network: true,
            ..NetworkPolicy::default()
        };
        let body =
            json!({ "resource": RESOURCE, "authorization_servers": ["http://10.0.0.1:8080"] });
        assert!(read_protected_resource(body.to_string().as_bytes(), RESOURCE, private).is_ok());
    }

    #[test]
    fn scopes_that_could_not_be_asked_for_are_not_kept() {
        let read = read(json!({
            "resource": RESOURCE,
            "authorization_servers": ["https://auth.example.com"],
            "scopes_supported": ["files:read", "bad scope"],
        }));
        assert_eq!(read.map(|r| r.scopes_supported), Ok(vec![]));
    }

    #[test]
    fn a_server_s_address_is_quoted_short_and_on_one_line() {
        let long = format!("https://{}.example.com", "a".repeat(100_000));
        let refused = read(json!({ "resource": RESOURCE, "authorization_servers": [long] }));
        assert!(refused.expect_err("refused").chars().count() < 400);
        let lure = "Contactez support@evil.example au +33 1 23 45 67 89\nURGENT";
        let refused = read(json!({ "resource": RESOURCE, "authorization_servers": [lure] }));
        let refused = refused.expect_err("refused");
        assert!(
            !refused.contains('\n') && refused.contains("\\u{a}"),
            "{refused}"
        );
    }

    #[test]
    fn an_issuer_has_no_parameters_but_an_endpoint_may() {
        let policy = NetworkPolicy::default();
        let refused = issuer_problem("https://auth.example.com/?tenant=1", policy);
        assert!(refused.expect("refused").contains("RFC 8414"));
        let endpoint = "https://auth.example.com/authorize?p=b2c_1_signin";
        assert_eq!(
            endpoint_problem(endpoint, "point d’autorisation", policy),
            None
        );
        for refused in [
            "https://auth.example.com/authorize#f",
            "https://u:p@auth.example.com/authorize",
            "http://auth.example.com/authorize?p=x",
            "https://10.0.0.1/authorize?p=x",
        ] {
            let problem = endpoint_problem(refused, "point d’autorisation", policy);
            let problem = problem.expect("refused");
            assert!(problem.starts_with("point d’autorisation « "), "{problem}");
            assert!(!problem.contains("utilisez le jeton"), "{problem}");
        }
    }

    #[test]
    fn an_address_is_bounded_with_its_parameters_and_hides_nothing() {
        let policy = NetworkPolicy::default();
        let long = format!(
            "https://auth.example.com/authorize?p={}",
            "a".repeat(100_000)
        );
        let refused = endpoint_problem(&long, "point d’autorisation", policy);
        assert!(refused.expect("refused").contains("trop longue"));
        for hiding in [
            "https://auth.example.com/\u{202e}moc.elgoog",
            "https://auth.example.com/t1\n",
            "https://auth.example.com/a b",
            " https://auth.example.com",
            "https://auth.example.com/\u{200b}",
        ] {
            let refused = issuer_problem(hiding, policy).expect("issuer refused");
            assert!(refused.contains("caché"), "{refused}");
            let refused = endpoint_problem(hiding, "point de jeton", policy).expect("refused");
            assert!(refused.contains("caché"), "{refused}");
        }
        let document = json!({
            "resource": RESOURCE,
            "authorization_servers": ["https://auth.example.com/\u{202e}moc.elgoog\nURGENT"],
        });
        assert!(read(document).is_err());
    }
}
