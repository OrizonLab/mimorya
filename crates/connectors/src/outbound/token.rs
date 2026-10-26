//! What the access token of an MCP server or a REST API may be: sent as
//! `Authorization: Bearer …` (or in the header a REST API names), it
//! follows the `b64token` of RFC 6750, which leaves out what JSON always
//! escapes (`"`, `\`) and keeps the spellings to mask few (`redact`). Checked
//! when an administrator saves a token, and again at each start for a token
//! saved before the rule.

pub const MIN_TOKEN_CHARS: usize = 8;
pub const MAX_TOKEN_CHARS: usize = 2000;

/// Why this token cannot be used (in French), if it cannot.
pub fn token_problem(token: &str) -> Option<String> {
    let length = token.chars().count();
    if !(MIN_TOKEN_CHARS..=MAX_TOKEN_CHARS).contains(&length) {
        return Some(format!(
            "Jeton de {MIN_TOKEN_CHARS} à {MAX_TOKEN_CHARS} caractères"
        ));
    }
    let body = token.trim_end_matches('=');
    let well_formed = !body.is_empty()
        && body
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-._~+/".contains(c));
    (!well_formed).then(|| {
        "Jeton invalide : lettres sans accent, chiffres et - . _ ~ + / seulement, puis \
         éventuellement des ="
            .into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_the_b64token_of_rfc_6750() {
        assert_eq!(token_problem("jeton-0123456789"), None);
        assert_eq!(token_problem("ab/cd+ef_gh.ij~kl=="), None);
        for refused in [
            "court",
            "avec espace 123",
            "accentu\u{e9}-123",
            "guillemet\"0123",
            "barre\\0123456",
            "egal=au-milieu",
            "==========",
            &"x".repeat(MAX_TOKEN_CHARS + 1),
        ] {
            assert!(token_problem(refused).is_some(), "{refused}");
        }
    }
}
