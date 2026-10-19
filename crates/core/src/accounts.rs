//! Limits of an account's identity, the same wherever an account is created
//! or renamed: by an administrator of its company or by the platform's
//! operator (ADR-033).

/// Longest e-mail address (RFC 5321 path limit).
pub const EMAIL_MAX_LEN: u64 = 254;
/// Shortest display name, in characters.
pub const DISPLAY_NAME_MIN_CHARS: u64 = 2;
/// Longest display name, in characters.
pub const DISPLAY_NAME_MAX_CHARS: u64 = 120;

/// Why `name` (trimmed by the caller) cannot be the display name of `whom`
/// (« de l’administrateur »…), if it cannot.
pub fn display_name_problem(name: &str, whom: &str) -> Option<String> {
    let chars = name.chars().count() as u64;
    (!(DISPLAY_NAME_MIN_CHARS..=DISPLAY_NAME_MAX_CHARS).contains(&chars)).then(|| {
        format!(
            "Le nom {whom} compte de {DISPLAY_NAME_MIN_CHARS} à {DISPLAY_NAME_MAX_CHARS} caractères"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::display_name_problem;

    #[test]
    fn counts_characters_not_bytes() {
        let whom = "de l’administrateur";
        assert_eq!(display_name_problem("Lé", whom), None);
        assert_eq!(display_name_problem(&"é".repeat(120), whom), None);
        assert_eq!(
            display_name_problem("L", whom).as_deref(),
            Some("Le nom de l’administrateur compte de 2 à 120 caractères")
        );
        assert!(display_name_problem(&"é".repeat(121), whom).is_some());
    }
}
