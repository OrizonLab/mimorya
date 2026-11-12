//! A company's own mail servers, reached with the protocols every host
//! speaks (ADR-205): where its IMAP and SMTP servers are (`servers`), the
//! account each person signs in with (`login`), and signing in, IMAP over
//! implicit TLS (`imap`), SMTP over implicit TLS or STARTTLS (`smtp`), both
//! on a connection the network policy allows, secured before any word.

mod imap;
mod login;
mod servers;
mod smtp;

pub use imap::{ImapSession, check_imap, imap_session};
pub use login::{MAX_PASSWORD_CHARS, MAX_USERNAME_CHARS, MailLogin};
pub use servers::{ImapServer, SmtpSecurity, SmtpServer};
pub use smtp::{NO_STARTTLS, check_smtp, smtp_connection};

use std::time::Duration;

/// What a server that refuses the account says (in French).
pub const CREDENTIALS_REFUSED: &str = "Identifiant ou mot de passe refusé par le serveur";
/// What a server that does not answer as a mail server should is told.
pub const NOT_A_MAIL_SERVER: &str = "Le serveur ne répond pas comme un serveur de messagerie";
/// How long signing in may take, from the connection to the account's
/// acceptance, whatever each step's own delay.
pub const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(30);
