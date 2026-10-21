//! The name of an account created at its first sign-in (ADR-067), from what
//! the provider's token says: its `name`, else its given and family names,
//! else the address's local part, else the address. One visible line, as an
//! administrator would write it: canonical, without hidden characters,
//! spaces folded, within the limits of a display name.

use super::TokenClaims;
use crate::accounts::{DISPLAY_NAME_MAX_CHARS, DISPLAY_NAME_MIN_CHARS};
use crate::plain_text::{canonical, is_hidden};

pub fn federated_display_name(claims: &TokenClaims<'_>, email: &str) -> String {
    let full = [claims.given_name, claims.family_name]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    let local = email.rsplit_once('@').map_or(email, |(local, _)| local);
    [claims.name.unwrap_or_default(), &full, local, email]
        .into_iter()
        .map(visible_line)
        .find(|name| name.chars().count() as u64 >= DISPLAY_NAME_MIN_CHARS)
        .unwrap_or_else(|| email.into())
}

/// Spaces of any kind folded into one, other hidden characters taken out,
/// cut to the longest display name.
fn visible_line(text: &str) -> String {
    let visible: String = canonical(text)
        .chars()
        .filter_map(|c| match c {
            c if c.is_whitespace() => Some(' '),
            c if is_hidden(c) => None,
            c => Some(c),
        })
        .collect();
    let folded = visible.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = folded
        .chars()
        .take(DISPLAY_NAME_MAX_CHARS as usize)
        .collect();
    cut.trim_end().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::display_name_problem;

    fn claims<'a>(
        name: Option<&'a str>,
        given: Option<&'a str>,
        family: Option<&'a str>,
    ) -> TokenClaims<'a> {
        TokenClaims {
            name,
            given_name: given,
            family_name: family,
            ..TokenClaims::default()
        }
    }

    const EMAIL: &str = "zoe.martin@entreprise.fr";

    #[test]
    fn the_name_first_then_given_and_family_names_then_the_address() {
        let named = claims(Some("Zoé Martin"), Some("Zoé"), Some("Dupont"));
        assert_eq!(federated_display_name(&named, EMAIL), "Zoé Martin");
        let given = claims(None, Some("Zoé"), Some("Martin"));
        assert_eq!(federated_display_name(&given, EMAIL), "Zoé Martin");
        let family = claims(Some("  "), None, Some("Martin"));
        assert_eq!(federated_display_name(&family, EMAIL), "Martin");
        assert_eq!(
            federated_display_name(&claims(None, None, None), EMAIL),
            "zoe.martin"
        );
        // A local part too short to be a name: the whole address.
        assert_eq!(
            federated_display_name(&claims(Some("Z"), None, None), "z@ab.fr"),
            "z@ab.fr"
        );
    }

    #[test]
    fn one_visible_line_within_the_limits() {
        let hidden = claims(
            Some("Zo\u{0065}\u{0301}\u{200B}\tMartin\n\u{202E}"),
            None,
            None,
        );
        // Canonical (é composed), the zero-width space and override out,
        // the tab and line break one space, trimmed.
        assert_eq!(federated_display_name(&hidden, EMAIL), "Zoé Martin");
        let long = "é".repeat(200);
        let cut = federated_display_name(&claims(Some(&long), None, None), EMAIL);
        assert_eq!(cut.chars().count(), 120);
        // Cut before a space: no trailing one.
        let spaced = format!("{} b", "a".repeat(119));
        let cut = federated_display_name(&claims(Some(&spaced), None, None), EMAIL);
        assert_eq!(cut, "a".repeat(119));
        for name in [
            federated_display_name(&hidden, EMAIL),
            federated_display_name(&claims(Some(&long), None, None), EMAIL),
            federated_display_name(&claims(Some("\u{200B}"), None, None), "x@y.fr"),
        ] {
            assert_eq!(display_name_problem(&name, "du compte"), None, "{name}");
        }
    }
}
