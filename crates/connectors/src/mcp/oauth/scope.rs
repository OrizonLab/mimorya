//! The scopes an MCP server's authorization is asked for (RFC 6749 §3.3):
//! each a visible ASCII token without space, quote or backslash, a few of
//! them, each short. What the platform sends, and what a server's metadata
//! or challenge suggests, is checked alike: a scope that could not be sent
//! as such is never kept.

pub const MAX_SCOPES: usize = 20;
pub const MAX_SCOPE_CHARS: usize = 200;

/// Why `scope` cannot be asked for, if it cannot.
pub fn scope_problem(scope: &str) -> Option<String> {
    if scope.is_empty() || scope.chars().count() > MAX_SCOPE_CHARS {
        return Some(format!(
            "Portée de 1 à {MAX_SCOPE_CHARS} caractères attendue"
        ));
    }
    // %x21 / %x23-5B / %x5D-7E: no space, no quote, no backslash.
    if !scope
        .bytes()
        .all(|b| b == 0x21 || (0x23..=0x5b).contains(&b) || (0x5d..=0x7e).contains(&b))
    {
        return Some(format!(
            "Portée « {} » : caractères visibles sans espace, guillemet ni barre oblique \
             inversée attendus",
            scope.escape_default()
        ));
    }
    None
}

/// Why `scopes` cannot be asked for together, if they cannot.
pub fn scopes_problem(scopes: &[String]) -> Option<String> {
    if scopes.len() > MAX_SCOPES {
        return Some(format!("{MAX_SCOPES} portées au plus"));
    }
    for (index, scope) in scopes.iter().enumerate() {
        if let Some(problem) = scope_problem(scope) {
            return Some(problem);
        }
        if scopes[..index].contains(scope) {
            return Some(format!("Portée « {scope} » donnée deux fois"));
        }
    }
    None
}

/// The scopes of a space-separated list a server suggests (a challenge's
/// `scope`), or none if one of them could not be asked for.
pub fn suggested_scopes(list: &str) -> Option<Vec<String>> {
    let scopes: Vec<String> = list
        .split(' ')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    scopes_problem(&scopes).is_none().then_some(scopes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scope_is_a_visible_token() {
        assert_eq!(scope_problem("files:read"), None);
        assert_eq!(scope_problem("https://api.example/read"), None);
        for refused in [
            "",
            "two words",
            "quo\"te",
            "back\\slash",
            "tab\t",
            "é",
            "zero\u{200b}width",
        ] {
            assert!(scope_problem(refused).is_some(), "{refused:?}");
        }
        assert!(scope_problem(&"a".repeat(MAX_SCOPE_CHARS)).is_none());
        assert!(scope_problem(&"a".repeat(MAX_SCOPE_CHARS + 1)).is_some());
    }

    #[test]
    fn scopes_are_few_and_each_once() {
        let many: Vec<String> = (0..=MAX_SCOPES).map(|i| format!("s{i}")).collect();
        assert!(scopes_problem(&many).is_some());
        assert!(scopes_problem(&many[..MAX_SCOPES]).is_none());
        let twice = vec!["read".to_string(), "read".to_string()];
        assert_eq!(
            scopes_problem(&twice).as_deref(),
            Some("Portée « read » donnée deux fois")
        );
    }

    #[test]
    fn a_suggested_list_is_kept_whole_or_not_at_all() {
        assert_eq!(
            suggested_scopes("files:read  files:write"),
            Some(vec!["files:read".into(), "files:write".into()])
        );
        assert_eq!(suggested_scopes(""), Some(vec![]));
        assert_eq!(suggested_scopes("read wr\"ite"), None);
    }
}
