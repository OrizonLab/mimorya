//! Rules of a company created by the platform's operator (ADR-033): its
//! name, and its short name (`slug`), unique across the platform.

/// Shortest company name, in characters.
pub const COMPANY_NAME_MIN_CHARS: usize = 2;
/// Longest company name, in characters.
pub const COMPANY_NAME_MAX_CHARS: usize = 120;

/// Why `name` (trimmed by the caller) cannot name a company, if it cannot.
pub fn company_name_problem(name: &str) -> Option<String> {
    let chars = name.chars().count();
    (!(COMPANY_NAME_MIN_CHARS..=COMPANY_NAME_MAX_CHARS).contains(&chars)).then(|| {
        format!(
            "Le nom de l’entreprise compte de {COMPANY_NAME_MIN_CHARS} à {COMPANY_NAME_MAX_CHARS} caractères"
        )
    })
}

/// Shortest slug.
pub const SLUG_MIN_LEN: usize = 2;
/// Longest slug (a DNS label's length).
pub const SLUG_MAX_LEN: usize = 63;

/// Why `slug` cannot name a company, if it cannot: lower-case ASCII letters
/// and digits, with single hyphens between them.
pub fn slug_problem(slug: &str) -> Option<String> {
    if !(SLUG_MIN_LEN..=SLUG_MAX_LEN).contains(&slug.len()) {
        return Some(format!(
            "Le sigle compte de {SLUG_MIN_LEN} à {SLUG_MAX_LEN} caractères"
        ));
    }
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-';
    if !slug.chars().all(allowed) {
        return Some(
            "Le sigle ne contient que des minuscules sans accent, des chiffres et des tirets"
                .into(),
        );
    }
    if slug.starts_with('-') || slug.ends_with('-') || slug.contains("--") {
        return Some("Le sigle ne commence ni ne finit par un tiret, et n’en double aucun".into());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{company_name_problem, slug_problem};

    #[test]
    fn names_a_company_with_2_to_120_characters() {
        assert_eq!(company_name_problem("Lé"), None);
        assert_eq!(company_name_problem(&"é".repeat(120)), None);
        assert!(company_name_problem("L").is_some());
        assert!(company_name_problem(&"é".repeat(121)).is_some());
    }

    #[test]
    fn accepts_lower_case_words_joined_by_hyphens() {
        for slug in [
            "ab",
            "acme",
            "neo-maintenance",
            "groupe-2026",
            &"a".repeat(63),
        ] {
            assert_eq!(slug_problem(slug), None, "{slug}");
        }
    }

    #[test]
    fn refuses_anything_else() {
        let refused = [
            "",
            "a",
            &"a".repeat(64),
            "Acme",
            "néo",
            "acme corp",
            "acme_corp",
            "acme/",
            "-acme",
            "acme-",
            "ac--me",
        ];
        for slug in refused {
            assert!(slug_problem(slug).is_some(), "{slug}");
        }
    }
}
