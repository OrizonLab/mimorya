//! What an MCP server says when it refuses a request without a valid token
//! (RFC 6750 §3, RFC 9728 §5.1): the `Bearer` challenge of its
//! `WWW-Authenticate` header, read for where its Protected Resource
//! Metadata is (`resource_metadata`), the scopes it wants (`scope`) and why
//! it refused (`error`). The header comes from the server: it is read
//! within bounds, as RFC 9110 writes challenges, and not at all when it is
//! malformed or names a parameter twice.

pub const MAX_HEADER_CHARS: usize = 4096;
const MAX_VALUE_CHARS: usize = 2000;

/// The parameters of a `Bearer` challenge that matter here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BearerChallenge {
    pub resource_metadata: Option<String>,
    pub scope: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Item {
    Token(String),
    Quoted(String),
    Equals,
    Comma,
}

/// The `Bearer` challenge of a `WWW-Authenticate` header, if it has exactly
/// one and the header is well formed.
pub fn bearer_challenge(header: &str) -> Option<BearerChallenge> {
    if header.chars().count() > MAX_HEADER_CHARS {
        return None;
    }
    let items = lex(header)?;
    let mut found = None;
    let mut at = 0;
    while at < items.len() {
        if items[at] == Item::Comma {
            at += 1;
            continue;
        }
        let Item::Token(scheme) = &items[at] else {
            return None;
        };
        at += 1;
        let params = params(&items, &mut at)?;
        if scheme.eq_ignore_ascii_case("bearer") {
            if found.is_some() {
                return None;
            }
            found = Some(challenge_of(params)?);
        }
    }
    found
}

/// The auth-params of the challenge starting at `at`, up to the next
/// challenge; a token68 is passed over.
fn params(items: &[Item], at: &mut usize) -> Option<Vec<(String, String)>> {
    let mut params: Vec<(String, String)> = Vec::new();
    while *at < items.len() {
        match (&items[*at], items.get(*at + 1), items.get(*at + 2)) {
            (Item::Comma, _, _) => *at += 1,
            (
                Item::Token(name),
                Some(Item::Equals),
                Some(Item::Token(value) | Item::Quoted(value)),
            ) => {
                let name = name.to_ascii_lowercase();
                if params.iter().any(|(seen, _)| *seen == name) {
                    return None;
                }
                params.push((name, value.clone()));
                *at += 3;
                if !matches!(items.get(*at), None | Some(Item::Comma)) {
                    return None;
                }
            }
            // A token68 (`Basic dXNlcg==`): its padding is not a parameter.
            (Item::Token(_), Some(Item::Equals), _) => {
                *at += 1;
                while items.get(*at) == Some(&Item::Equals) {
                    *at += 1;
                }
            }
            // The next challenge's scheme.
            (Item::Token(_), _, _) => return Some(params),
            _ => return None,
        }
    }
    Some(params)
}

fn challenge_of(params: Vec<(String, String)>) -> Option<BearerChallenge> {
    let mut challenge = BearerChallenge::default();
    for (name, value) in params {
        if value.chars().count() > MAX_VALUE_CHARS {
            return None;
        }
        match name.as_str() {
            "resource_metadata" => challenge.resource_metadata = Some(value),
            "scope" => challenge.scope = Some(value),
            "error" => challenge.error = Some(value),
            _ => {}
        }
    }
    Some(challenge)
}

/// The header as tokens, quoted strings (unescaped), `=` and `,`.
fn lex(header: &str) -> Option<Vec<Item>> {
    let mut items = Vec::new();
    let mut chars = header.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => {}
            '=' => items.push(Item::Equals),
            ',' => items.push(Item::Comma),
            '"' => {
                let mut value = String::new();
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => value.push(chars.next()?),
                        c if c.is_control() && c != '\t' => return None,
                        c => value.push(c),
                    }
                }
                items.push(Item::Quoted(value));
            }
            c if is_token_char(c) => {
                let mut token = c.to_string();
                while let Some(&next) = chars.peek() {
                    if !is_token_char(next) {
                        break;
                    }
                    token.push(next);
                    chars.next();
                }
                items.push(Item::Token(token));
            }
            _ => return None,
        }
    }
    Some(items)
}

/// RFC 9110 tchar, and `/` for a token68.
fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~/".contains(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bearer_challenge_says_where_its_metadata_is() {
        let header = r#"Bearer realm="mcp", error="invalid_token", resource_metadata="https://mcp.example.com/.well-known/oauth-protected-resource/mcp", scope="files:read files:write""#;
        assert_eq!(
            bearer_challenge(header),
            Some(BearerChallenge {
                resource_metadata: Some(
                    "https://mcp.example.com/.well-known/oauth-protected-resource/mcp".into()
                ),
                scope: Some("files:read files:write".into()),
                error: Some("invalid_token".into()),
            })
        );
    }

    #[test]
    fn other_challenges_are_passed_over() {
        let header =
            r#"Basic realm="x", Bearer resource_metadata="https://a.example/m", Negotiate YWJj=="#;
        assert_eq!(
            bearer_challenge(header)
                .and_then(|c| c.resource_metadata)
                .as_deref(),
            Some("https://a.example/m")
        );
        assert_eq!(bearer_challenge("Basic dXNlcjpwYXNz=="), None);
        assert_eq!(bearer_challenge("bearer"), Some(BearerChallenge::default()));
        // A tab within a quoted string (RFC 9110 qdtext).
        assert_eq!(
            bearer_challenge("Bearer error=\"a\tb\"")
                .and_then(|c| c.error)
                .as_deref(),
            Some("a\tb")
        );
        assert_eq!(
            bearer_challenge(r#"Bearer error="invalid_\"token""#)
                .and_then(|c| c.error)
                .as_deref(),
            Some("invalid_\"token")
        );
    }

    #[test]
    fn a_malformed_or_ambiguous_header_is_not_read() {
        for refused in [
            r#"Bearer resource_metadata="https://a.example/m"#,
            r#"Bearer scope="a", scope="b""#,
            r#"Bearer error="x" scope="y""#,
            r#"Bearer realm="x", Bearer realm="y""#,
            "Bearer realm=\"x\u{1}\"",
            "Bearer realm=(x)",
        ] {
            assert_eq!(bearer_challenge(refused), None, "{refused}");
        }
        let long = format!(r#"Bearer scope="{}""#, "a".repeat(MAX_VALUE_CHARS + 1));
        assert_eq!(bearer_challenge(&long), None);
        let huge = format!("Bearer {}", "a=b, ".repeat(MAX_HEADER_CHARS));
        assert_eq!(bearer_challenge(&huge), None);
    }
}
