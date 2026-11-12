//! The account a person signs in to their company's mail servers with
//! (ADR-205): an identifier and a password, often an application password.
//! Neither holds a line break nor a control character, which would end a
//! command early; the password is never written, neither by `Debug` nor in
//! a refusal: taken out of it however a server or a library spelt it
//! (`scrub`), and masked again by its redactor.

use std::fmt;

use crate::outbound::Redactor;

/// An identifier's longest form: an address (RFC 5321).
pub const MAX_USERNAME_CHARS: usize = 320;
/// A password's longest form.
pub const MAX_PASSWORD_CHARS: usize = 1024;

/// An identifier and its password.
#[derive(Clone)]
pub struct MailLogin {
    username: String,
    password: String,
}

impl MailLogin {
    /// The account; why it cannot be one (in French), if it cannot.
    pub fn new(username: &str, password: &str) -> Result<Self, String> {
        let username = username.trim();
        if username.is_empty() || username.chars().count() > MAX_USERNAME_CHARS {
            return Err(format!(
                "Identifiant de 1 à {MAX_USERNAME_CHARS} caractères attendu"
            ));
        }
        if password.is_empty() || password.chars().count() > MAX_PASSWORD_CHARS {
            return Err(format!(
                "Mot de passe de 1 à {MAX_PASSWORD_CHARS} caractères attendu"
            ));
        }
        if username
            .chars()
            .chain(password.chars())
            .any(char::is_control)
        {
            return Err("Identifiant et mot de passe sans caractère de contrôle attendus".into());
        }
        Ok(Self {
            username: username.to_string(),
            password: password.to_string(),
        })
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub(super) fn password(&self) -> &str {
        &self.password
    }

    /// `text` without the password, however a server quoted it back or a
    /// library wrote it: as is, quoted as IMAP quotes a string, and each of
    /// these escaped as `Debug` escapes it.
    pub(super) fn scrub(&self, text: &str) -> String {
        let quoted = self.password.replace('\\', "\\\\").replace('"', "\\\"");
        let mut spellings = [
            self.password.escape_debug().to_string(),
            quoted.escape_debug().to_string(),
            quoted,
            self.password.clone(),
        ];
        spellings.sort_by_key(|spelling| std::cmp::Reverse(spelling.len()));
        spellings.iter().fold(text.to_string(), |text, spelling| {
            text.replace(spelling.as_str(), "***")
        })
    }

    /// What masks the password in whatever a server says back.
    pub fn redactor(&self) -> Redactor {
        Redactor::new(Some(&self.password))
    }
}

impl fmt::Debug for MailLogin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MailLogin")
            .field("username", &self.username)
            .field("password", &"***")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_account_is_an_identifier_and_a_password() {
        let login = MailLogin::new(" paul@exemple.fr ", "mot de passe \"à\" \\ part").unwrap();
        assert_eq!(login.username(), "paul@exemple.fr");
        assert_eq!(login.password(), "mot de passe \"à\" \\ part");
        for (username, password) in [
            ("", "secret"),
            ("paul", ""),
            ("paul\r\nLOGOUT", "secret"),
            ("paul", "secret\r\nA2 LOGOUT"),
            ("paul", "sec\0ret"),
        ] {
            assert!(MailLogin::new(username, password).is_err(), "{username:?}");
        }
        let long = "a".repeat(MAX_PASSWORD_CHARS + 1);
        assert!(MailLogin::new("paul", &long).is_err());
    }

    #[test]
    fn its_password_is_never_written() {
        let login = MailLogin::new("paul@exemple.fr", "secret-de-paul").unwrap();
        let debug = format!("{login:?}");
        assert!(
            debug.contains("paul@exemple.fr") && !debug.contains("secret"),
            "{debug}"
        );
        let said = login.redactor().text("refusé pour secret-de-paul");
        assert!(!said.contains("secret-de-paul"), "{said}");
    }

    #[test]
    fn scrubs_the_password_however_it_was_spelt() {
        let password = r#"autre "secret" \ de paul"#;
        let login = MailLogin::new("paul", password).unwrap();
        let quoted = r#"autre \"secret\" \\ de paul"#;
        let said = format!("{password} ; {quoted} ; {:?} ; {:?}", password, quoted);
        let scrubbed = login.scrub(&said);
        assert!(!scrubbed.contains("secret"), "{scrubbed}");
        assert_eq!(scrubbed.matches("***").count(), 4, "{scrubbed}");
    }
}
