//! The filters an identity provider sends when it looks for an account
//! (RFC 7644 §3.4.2.2): `userName eq "…"` and `externalId eq "…"`, which
//! Entra ID and Okta send to match one, the attribute and the operator in
//! any case. Anything else is refused (`invalidFilter`), never read loosely:
//! a filter read wrongly would match the wrong account.

/// What a provider looks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserFilter {
    /// The account's address, compared without case.
    UserName(String),
    /// The provider's own identifier of the account, compared exactly.
    ExternalId(String),
}

/// Why a filter is refused.
pub const FILTER_REFUSED: &str =
    "Seuls les filtres userName eq \"…\" et externalId eq \"…\" sont acceptés";

/// The filter `filter` asks for, or why it is refused.
pub fn parse_user_filter(filter: &str) -> Result<UserFilter, &'static str> {
    let filter = filter.trim();
    let (attribute, rest) = filter
        .split_once(char::is_whitespace)
        .ok_or(FILTER_REFUSED)?;
    let (operator, value) = rest
        .trim_start()
        .split_once(char::is_whitespace)
        .ok_or(FILTER_REFUSED)?;
    if !operator.eq_ignore_ascii_case("eq") {
        return Err(FILTER_REFUSED);
    }
    // A JSON string, its escapes read as JSON reads them; nothing after it.
    let value = value.trim();
    if !value.starts_with('"') {
        return Err(FILTER_REFUSED);
    }
    let value: String = serde_json::from_str(value).map_err(|_| FILTER_REFUSED)?;
    if attribute.eq_ignore_ascii_case("userName") {
        Ok(UserFilter::UserName(value))
    } else if attribute.eq_ignore_ascii_case("externalId") {
        Ok(UserFilter::ExternalId(value))
    } else {
        Err(FILTER_REFUSED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_entra_id_and_okta_send() {
        assert_eq!(
            parse_user_filter(r#"userName eq "Lea.Martin@neo.example""#),
            Ok(UserFilter::UserName("Lea.Martin@neo.example".into()))
        );
        assert_eq!(
            parse_user_filter(r#"  username  EQ  "a@b.example"  "#),
            Ok(UserFilter::UserName("a@b.example".into()))
        );
        assert_eq!(
            parse_user_filter(r#"externalId eq "5f1c-\"x\"""#),
            Ok(UserFilter::ExternalId("5f1c-\"x\"".into()))
        );
    }

    #[test]
    fn refuses_anything_else_rather_than_read_it_loosely() {
        for filter in [
            "",
            "userName",
            r#"userName eq"#,
            r#"userName ne "a@b.example""#,
            r#"userName co "a""#,
            r#"userName sw "a""#,
            r#"userName eq a@b.example"#,
            r#"userName eq "a@b.example" or userName eq "c@d.example""#,
            r#"userName eq "a@b.example" and active eq true"#,
            r#"emails.value eq "a@b.example""#,
            r#"emails[type eq "work"].value eq "a@b.example""#,
            r#"displayName eq "Léa""#,
            r#"userName eq "unterminated"#,
        ] {
            assert_eq!(parse_user_filter(filter), Err(FILTER_REFUSED), "{filter}");
        }
    }
}
