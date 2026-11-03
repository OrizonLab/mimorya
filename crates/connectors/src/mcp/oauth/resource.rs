//! The MCP server as the resource its authorization is for (RFC 8707,
//! RFC 9728): its canonical address, sent with every request for a token so
//! that a token is only good for it; where its Protected Resource Metadata
//! may be published; and whether an address is on its own origin, the only
//! one whose word the platform takes about where to authorize.

use url::Url;

/// Where RFC 9728 publishes the metadata of a protected resource.
const WELL_KNOWN: &str = "/.well-known/oauth-protected-resource";

/// The canonical address of the MCP server at `url`: scheme and host in
/// lower case (as `Url` writes them), default port left out, no fragment,
/// no trailing slash on a bare origin. `None` if `url` is not an absolute
/// http(s) address or carries a fragment.
pub fn canonical_resource(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host().is_none()
        || parsed.fragment().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return None;
    }
    let mut canonical = parsed.to_string();
    if parsed.path() == "/" && parsed.query().is_none() {
        canonical.pop();
    }
    Some(canonical)
}

/// Whether two canonical addresses name the same resource.
pub fn same_resource(a: &str, b: &str) -> bool {
    matches!((canonical_resource(a), canonical_resource(b)), (Some(a), Some(b)) if a == b)
}

/// Whether `candidate` is on the origin of `server`: an http(s) address of
/// the same scheme, host and port (a `blob:` address holding the server's
/// is not).
pub fn same_origin(server: &str, candidate: &str) -> bool {
    match (Url::parse(server), Url::parse(candidate)) {
        (Ok(server), Ok(candidate)) => {
            matches!(candidate.scheme(), "http" | "https") && server.origin() == candidate.origin()
        }
        _ => false,
    }
}

/// The path of an address as the well-known locations insert it: as written
/// (RFC 8414 §3.1, RFC 9728 §3.1: only a slash right after the host goes),
/// then without a final slash if it had one.
pub(super) fn paths(path: &str) -> Vec<String> {
    if path == "/" || path.is_empty() {
        return vec![String::new()];
    }
    let trimmed = path.trim_end_matches('/');
    if trimmed == path {
        vec![path.to_string()]
    } else {
        vec![path.to_string(), trimmed.to_string()]
    }
}

/// Where the Protected Resource Metadata of the MCP server at `url` may be,
/// in the order RFC 9728 §3.1 and the MCP specification try them: the
/// well-known path inserted before the server's path (`paths`), then at the
/// root.
pub fn protected_resource_locations(url: &str) -> Vec<String> {
    let Ok(parsed) = Url::parse(url) else {
        return Vec::new();
    };
    let origin = parsed.origin().ascii_serialization();
    let mut locations: Vec<String> = paths(parsed.path())
        .into_iter()
        .filter(|path| !path.is_empty())
        .map(|path| format!("{origin}{WELL_KNOWN}{path}"))
        .collect();
    locations.push(format!("{origin}{WELL_KNOWN}"));
    locations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_canonical_address_is_written_one_way() {
        let canonical = |url| canonical_resource(url);
        assert_eq!(
            canonical("HTTPS://MCP.Example.com/mcp").as_deref(),
            Some("https://mcp.example.com/mcp")
        );
        assert_eq!(
            canonical("https://mcp.example.com:443/").as_deref(),
            Some("https://mcp.example.com")
        );
        assert_eq!(
            canonical("https://mcp.example.com:8443").as_deref(),
            Some("https://mcp.example.com:8443")
        );
        assert_eq!(
            canonical("https://mcp.example.com/a/b/").as_deref(),
            Some("https://mcp.example.com/a/b/")
        );
        for refused in [
            "mcp.example.com/mcp",
            "ftp://x/mcp",
            "https://x/mcp#f",
            "https://u:p@x/mcp",
        ] {
            assert_eq!(canonical(refused), None, "{refused}");
        }
        assert!(same_resource(
            "https://MCP.example.com:443/mcp",
            "https://mcp.example.com/mcp"
        ));
        assert!(!same_resource(
            "https://mcp.example.com/mcp",
            "https://mcp.example.com/other"
        ));
        assert!(!same_resource(
            "https://mcp.example.com/mcp",
            "not an address"
        ));
    }

    #[test]
    fn an_origin_is_scheme_host_and_port() {
        let server = "https://mcp.example.com/mcp";
        assert!(same_origin(server, "https://mcp.example.com/.well-known/x"));
        for other in [
            "http://mcp.example.com/x",
            "https://mcp.example.com:8443/x",
            "https://evil.example.com/x",
            "https://mcp.example.com.evil.example/x",
            "blob:https://mcp.example.com/x",
        ] {
            assert!(!same_origin(server, other), "{other}");
        }
    }

    #[test]
    fn the_metadata_is_looked_for_under_the_path_then_at_the_root() {
        assert_eq!(
            protected_resource_locations("https://mcp.example.com/tools/mcp"),
            [
                "https://mcp.example.com/.well-known/oauth-protected-resource/tools/mcp",
                "https://mcp.example.com/.well-known/oauth-protected-resource",
            ]
        );
        assert_eq!(
            protected_resource_locations("https://mcp.example.com:8443/"),
            ["https://mcp.example.com:8443/.well-known/oauth-protected-resource"]
        );
    }

    #[test]
    fn a_final_slash_is_kept_then_tried_without() {
        assert_eq!(
            protected_resource_locations("https://mcp.example.com/mcp/"),
            [
                "https://mcp.example.com/.well-known/oauth-protected-resource/mcp/",
                "https://mcp.example.com/.well-known/oauth-protected-resource/mcp",
                "https://mcp.example.com/.well-known/oauth-protected-resource",
            ]
        );
    }
}
