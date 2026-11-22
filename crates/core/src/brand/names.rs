//! The names people read: the product's (the tab, the sign-in, the sidebar,
//! the e-mails, the authenticator apps) and the assistant's (the
//! conversations, the model's instructions), the same by default.

/// The name the platform carries when the installation names nothing.
pub const DEFAULT_PRODUCT_NAME: &str = "Mimorya";

/// The longest name, in characters: it fits the sidebar and a subject line.
pub const MAX_NAME_CHARS: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrandNames {
    pub product: String,
    pub assistant: String,
}

impl Default for BrandNames {
    fn default() -> Self {
        Self {
            product: DEFAULT_PRODUCT_NAME.to_string(),
            assistant: DEFAULT_PRODUCT_NAME.to_string(),
        }
    }
}

/// A name as written, or why it is refused: it goes into e-mail headers, so
/// no control character (a line break would add one); into an
/// authenticator app's label, where a colon separates the issuer from the
/// account (`otpauth://totp/Issuer:account`), so none for the product.
pub fn name(written: &str, product: bool) -> Result<String, String> {
    let name = written.trim();
    if name.is_empty() {
        return Err("nom vide".to_string());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(format!("{MAX_NAME_CHARS} caractères au plus"));
    }
    if name.chars().any(char::is_control) {
        return Err("caractère de contrôle (retour à la ligne, tabulation…) refusé".to_string());
    }
    if product && name.contains(':') {
        return Err(
            "deux-points refusés : le nom sert d’émetteur dans les applications \
             d’authentification, où ils séparent l’émetteur du compte"
                .to_string(),
        );
    }
    Ok(name.to_string())
}

/// The start of a file name drawn from the product's name: its letters and
/// digits without accents, lower case, anything else a hyphen; the
/// default's when nothing is left (`Ω` alone).
pub fn file_prefix(product: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    use unicode_normalization::char::is_combining_mark;
    let mut prefix = String::new();
    for character in product.nfd() {
        if character.is_ascii_alphanumeric() {
            prefix.push(character.to_ascii_lowercase());
        } else if is_combining_mark(character) {
            // The accent of a letter kept: « é » is « e ».
        } else if !prefix.is_empty() && !prefix.ends_with('-') {
            prefix.push('-');
        }
    }
    let prefix = prefix.trim_end_matches('-');
    if prefix.is_empty() {
        DEFAULT_PRODUCT_NAME.to_ascii_lowercase()
    } else {
        prefix.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_taken_trimmed() {
        assert_eq!(name("  Néo Assistant ", true).unwrap(), "Néo Assistant");
        assert_eq!(
            name(&"é".repeat(MAX_NAME_CHARS), true)
                .unwrap()
                .chars()
                .count(),
            40
        );
    }

    #[test]
    fn a_name_that_would_break_a_header_or_a_label_is_refused() {
        assert_eq!(name("   ", true).unwrap_err(), "nom vide");
        assert!(name(&"a".repeat(MAX_NAME_CHARS + 1), false).is_err());
        assert!(
            name("Néo\r\nBcc: x@y.example", false)
                .unwrap_err()
                .starts_with("caractère de contrôle")
        );
        assert!(name("Néo\tAssistant", false).is_err());
        assert!(
            name("Néo : assistant", true)
                .unwrap_err()
                .starts_with("deux-points refusés")
        );
        // The assistant's name is no issuer.
        assert_eq!(
            name("Ada : l’assistante", false).unwrap(),
            "Ada : l’assistante"
        );
    }

    #[test]
    fn a_file_prefix_keeps_letters_and_digits() {
        assert_eq!(file_prefix("Mimorya"), "mimorya");
        assert_eq!(file_prefix("Néo Assistant 2"), "neo-assistant-2");
        assert_eq!(file_prefix("  L’Atelier — IA  "), "l-atelier-ia");
        assert_eq!(file_prefix("Ω"), "mimorya");
    }
}
