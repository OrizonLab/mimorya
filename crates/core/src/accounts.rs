//! Limits of an account's identity, the same wherever an account is created
//! or renamed: by an administrator of its company or by the platform's
//! operator (ADR-033).

/// Longest e-mail address (RFC 5321 path limit).
pub const EMAIL_MAX_LEN: u64 = 254;
/// Shortest display name, in characters.
pub const DISPLAY_NAME_MIN_CHARS: u64 = 2;
/// Longest display name, in characters.
pub const DISPLAY_NAME_MAX_CHARS: u64 = 120;

/// Whether `name` (trimmed by the caller) has an acceptable length.
pub fn display_name_fits(name: &str) -> bool {
    let chars = name.chars().count() as u64;
    (DISPLAY_NAME_MIN_CHARS..=DISPLAY_NAME_MAX_CHARS).contains(&chars)
}

#[cfg(test)]
mod tests {
    use super::display_name_fits;

    #[test]
    fn counts_characters_not_bytes() {
        assert!(display_name_fits("Lé"));
        assert!(display_name_fits(&"é".repeat(120)));
        assert!(!display_name_fits("L"));
        assert!(!display_name_fits(&"é".repeat(121)));
    }
}
