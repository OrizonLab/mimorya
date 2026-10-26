//! The prefix naming the tools of a connector a company adds: an MCP
//! server's are `mcp_<prefix>_…` (ADR-025), a REST API's `rest_<prefix>_…`
//! (ADR-112).

/// Why this prefix cannot name a connector's tools (in French), if it
/// cannot: a lowercase letter then letters or digits, 2 to 16.
pub fn prefix_problem(prefix: &str) -> Option<String> {
    let mut chars = prefix.chars();
    let well_formed = chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && (2..=16).contains(&prefix.len());
    (!well_formed).then(|| {
        "Préfixe invalide : une minuscule puis des minuscules ou des chiffres (2 à 16)".into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_prefixes() {
        assert_eq!(prefix_problem("docs2"), None);
        for refused in ["d", "Docs", "2docs", "docs_x", "abcdefghijklmnopq"] {
            assert!(prefix_problem(refused).is_some(), "{refused}");
        }
    }
}
