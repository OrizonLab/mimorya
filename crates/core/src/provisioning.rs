//! Rules of a company created by the platform's operator (ADR-033): its
//! name, and its short name (`slug`), unique across the platform.

/// Why `name` (trimmed by the caller) cannot name a company, if it cannot.
pub fn company_name_problem(name: &str) -> Option<&'static str> {
    if (2..=120).contains(&name.chars().count()) {
        None
    } else {
        Some("Le nom de l’entreprise compte de 2 à 120 caractères")
    }
}

/// Why `slug` cannot name a company, if it cannot: 2 to 63 lower-case ASCII
/// letters and digits, with single hyphens between them.
pub fn slug_problem(slug: &str) -> Option<&'static str> {
    if !(2..=63).contains(&slug.len()) {
        return Some("Le sigle compte de 2 à 63 caractères");
    }
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-';
    if !slug.chars().all(allowed) {
        return Some(
            "Le sigle ne contient que des minuscules sans accent, des chiffres et des tirets",
        );
    }
    if slug.starts_with('-') || slug.ends_with('-') || slug.contains("--") {
        return Some("Le sigle ne commence ni ne finit par un tiret, et n’en double aucun");
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
