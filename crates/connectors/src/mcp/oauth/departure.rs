//! Where an administrator's browser is sent to authorize an MCP server
//! (ADR-171): the authorization endpoint kept in the declaration, asked for
//! a code for the declared client, at the platform's return address, with
//! PKCE S256 always, for the resource the tokens will be for (RFC 8707) and
//! the scopes declared; `state` ties the return to the attempt. The
//! endpoint's own parameters are kept (RFC 6749 §3.1), never one of these.

use url::Url;

use super::declaration::OAuthDeclaration;

/// What the platform adds to the endpoint.
const OWN_PARAMETERS: [&str; 8] = [
    "response_type",
    "client_id",
    "redirect_uri",
    "state",
    "code_challenge",
    "code_challenge_method",
    "resource",
    "scope",
];

/// What the attempt brings to the departure.
pub struct Departure<'a> {
    pub redirect_uri: &'a str,
    pub state: &'a str,
    /// The S256 challenge of the attempt's PKCE verifier.
    pub code_challenge: &'a str,
}

/// The address the browser is sent to, or why there is none.
pub fn authorization_url(
    declaration: &OAuthDeclaration,
    departure: Departure<'_>,
) -> Result<String, String> {
    let mut url = Url::parse(&declaration.authorization_endpoint)
        .map_err(|_| "Point d’autorisation illisible : déclarez OAuth de nouveau".to_string())?;
    if let Some((taken, _)) = url
        .query_pairs()
        .find(|(name, _)| OWN_PARAMETERS.contains(&name.as_ref()))
    {
        return Err(format!(
            "Le point d’autorisation porte déjà le paramètre « {} », que la plateforme envoie",
            taken.chars().take(40).collect::<String>()
        ));
    }
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", &declaration.client_id)
            .append_pair("redirect_uri", departure.redirect_uri)
            .append_pair("state", departure.state)
            .append_pair("code_challenge", departure.code_challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("resource", &declaration.resource);
        if !declaration.scopes.is_empty() {
            query.append_pair("scope", &declaration.scopes.join(" "));
        }
    }
    Ok(url.into())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::mcp::oauth::ClientAuth;

    fn declaration(endpoint: &str, scopes: &[&str]) -> OAuthDeclaration {
        OAuthDeclaration {
            revocation_endpoint: None,
            registered: None,
            issuer: "https://auth.example.com".into(),
            resource: "https://mcp.example.com/mcp".into(),
            authorization_endpoint: endpoint.into(),
            token_endpoint: "https://auth.example.com/token".into(),
            names_issuer_on_return: false,
            client_id: "client mimorya".into(),
            client_auth: ClientAuth::ClientSecretPost,
            scopes: scopes.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn departure() -> Departure<'static> {
        Departure {
            redirect_uri: "https://app.example.com/api/admin/connectors/oauth/callback",
            state: "etat",
            code_challenge: "defi",
        }
    }

    #[test]
    fn the_browser_asks_for_a_code_with_pkce_for_the_resource() {
        let declared = declaration(
            "https://auth.example.com/authorize?p=b2c_1",
            &["files:read", "offline_access"],
        );
        let url = authorization_url(&declared, departure()).expect("url");
        let parsed = Url::parse(&url).expect("parsed");
        let query: HashMap<String, String> = parsed.query_pairs().into_owned().collect();
        assert_eq!(query["p"], "b2c_1");
        assert_eq!(query["response_type"], "code");
        assert_eq!(query["client_id"], "client mimorya");
        assert_eq!(query["code_challenge_method"], "S256");
        assert_eq!(query["code_challenge"], "defi");
        assert_eq!(query["state"], "etat");
        assert_eq!(query["resource"], "https://mcp.example.com/mcp");
        assert_eq!(query["scope"], "files:read offline_access");
        // No scope declared: none asked.
        let none = authorization_url(
            &declaration("https://a.example/authorize", &[]),
            departure(),
        );
        assert!(!none.expect("url").contains("scope="));
    }

    #[test]
    fn an_endpoint_that_names_a_parameter_of_its_own_is_refused() {
        for endpoint in [
            "https://auth.example.com/authorize?client_id=autre",
            "https://auth.example.com/authorize?redirect_uri=https://evil.example",
            "https://auth.example.com/authorize?code_challenge_method=plain",
        ] {
            let refused = authorization_url(&declaration(endpoint, &[]), departure());
            assert!(refused.expect_err(endpoint).contains("déjà"), "{endpoint}");
        }
    }
}
