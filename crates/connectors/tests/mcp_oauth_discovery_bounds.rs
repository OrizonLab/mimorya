//! The bounds of the discovery of an MCP server's authorization (ADR-171),
//! against a server that tries them: a document too long, an answer that
//! never comes or never ends, for one request or all of them, an answer
//! broken off at a long address written to mislead, said short; a refusal
//! whose body never ends, not read; a server that says to come back later,
//! to the first request or to a document, or a name that leads to no
//! public server, said so.

mod support;

use std::time::Duration;

use mimorya_connectors::mcp::oauth::{DiscoveryLimits, discover, discover_within};
use mimorya_connectors::outbound::{NOT_PUBLIC, NetworkPolicy, TOO_SLOW, UNREACHABLE};
use mimorya_connectors::testing::{
    FakeAuthorizationServer, LURE, OAuthQuirk, serve_oauth_mcp_http,
};
use support::mcp::PRIVATE;

async fn refusal(quirk: OAuthQuirk, limits: DiscoveryLimits) -> String {
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, quirk).await;
    discover_within(&server.url, None, PRIVATE, limits)
        .await
        .expect_err("refused")
}

/// A short delay for each request, a long one for them all.
const EACH_SHORT: DiscoveryLimits = DiscoveryLimits {
    per_request: Duration::from_millis(300),
    overall: Duration::from_secs(30),
};

#[tokio::test]
async fn a_document_longer_than_any_is_refused() {
    let refused = refusal(OAuthQuirk::HugeMetadata, DiscoveryLimits::DEFAULT).await;
    assert!(
        refused.contains("trop longue (256 Kio au plus)"),
        "{refused}"
    );
}

#[tokio::test]
async fn an_answer_that_never_comes_is_waited_for_within_bounds() {
    let refused = refusal(OAuthQuirk::SilentMetadata, EACH_SHORT).await;
    assert!(refused.contains(TOO_SLOW), "{refused}");
    let all = DiscoveryLimits {
        per_request: EACH_SHORT.overall,
        overall: EACH_SHORT.per_request,
    };
    let refused = refusal(OAuthQuirk::SilentMetadata, all).await;
    assert!(refused.contains("Découverte interrompue"), "{refused}");
    assert!(refused.contains("0,3 s"), "{refused}");
}

#[tokio::test]
async fn a_document_that_never_ends_is_bounded_by_its_own_delay() {
    // Its headers come, its body never ends: the request's delay, not
    // the whole discovery's, stops it.
    let refused = refusal(OAuthQuirk::DripMetadata, EACH_SHORT).await;
    assert!(refused.contains(TOO_SLOW), "{refused}");
}

#[tokio::test]
async fn a_refusal_whose_body_never_ends_is_not_read() {
    let authority = FakeAuthorizationServer::start().await;
    let server = serve_oauth_mcp_http(&authority, OAuthQuirk::EndlessRefusal).await;
    let found = discover_within(&server.url, None, PRIVATE, EACH_SHORT).await;
    assert_eq!(found.expect("found").issuer, authority.issuer);
}

#[tokio::test]
async fn an_answer_broken_off_is_said_short_whatever_the_address() {
    let refused = refusal(OAuthQuirk::BrokenMetadata, DiscoveryLimits::DEFAULT).await;
    assert!(refused.contains(UNREACHABLE), "{refused}");
    // The address quoted short, never whole in the client's own words.
    assert!(refused.chars().count() < 400, "{refused}");
    assert!(!refused.contains(LURE), "{refused}");
    assert!(!refused.contains("for url"), "{refused}");
}

#[tokio::test]
async fn a_server_that_says_to_come_back_later_is_not_taken_at_its_word() {
    // A 503 is no absence of metadata: nothing else is asked instead.
    let refused = refusal(OAuthQuirk::UnavailableMetadata, DiscoveryLimits::DEFAULT).await;
    assert!(
        refused.contains("a répondu 503 : réessayez plus tard"),
        "{refused}"
    );
    // Neither is it a server that does not ask for OAuth.
    for status in [429, 503] {
        let refused = refusal(OAuthQuirk::ProbeAnswered(status), DiscoveryLimits::DEFAULT).await;
        assert!(
            refused.contains(&format!("a répondu {status} : réessayez plus tard")),
            "{refused}"
        );
    }
}

#[tokio::test]
async fn a_name_that_leads_to_no_public_server_is_said_so() {
    let refused = discover("https://localhost/mcp", None, NetworkPolicy::default())
        .await
        .expect_err("refused");
    assert!(refused.contains(NOT_PUBLIC), "{refused}");
    assert!(
        refused.starts_with("« https://localhost/mcp » : "),
        "{refused}"
    );
}
