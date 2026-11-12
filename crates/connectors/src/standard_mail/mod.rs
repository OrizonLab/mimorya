//! A company's own mail servers, reached with the protocols every host
//! speaks (ADR-205): its declaration (`declaration`) and connector type
//! (`connector`), where its IMAP and SMTP servers are (`servers`), the
//! account each person signs in with (`login`), and signing in, IMAP over
//! implicit TLS (`imap`), SMTP over implicit TLS or STARTTLS (`smtp`, its
//! words in clear in `starttls`), both on a connection the network policy
//! allows, secured by the platform's own TLS before any credential, what
//! the server sends bounded.

mod connector;
mod declaration;
mod imap;
mod login;
mod servers;
mod smtp;
mod starttls;
mod wire;

pub use connector::MailServerConnector;
pub use declaration::{
    MAIL_NAMESPACE, MAIL_SERVER, MAIL_TOOLS, MAIL_WRITE_TOOLS, MailServerConfig,
};
pub use imap::{ImapSession, check_imap, imap_session};
pub use login::{MAX_PASSWORD_CHARS, MAX_USERNAME_CHARS, MailLogin};
pub use servers::{ImapServer, SmtpSecurity, SmtpServer};
pub use smtp::{check_smtp, smtp_connection};

use std::time::Duration;

/// What a server that refuses the account says (in French).
pub const CREDENTIALS_REFUSED: &str = "Identifiant ou mot de passe refusé par le serveur";
/// What a server that refuses the account's form says.
pub const FORM_REFUSED: &str =
    "Le serveur refuse la forme de l’identifiant ou du mot de passe (caractères accentués ?)";
/// What a server that does not answer as a mail server should is told.
pub const NOT_A_MAIL_SERVER: &str = "Le serveur ne répond pas comme un serveur de messagerie";
/// What an SMTP server offering no STARTTLS is told.
pub const NO_STARTTLS: &str =
    "Le serveur SMTP n’offre pas STARTTLS : aucun identifiant ne lui est envoyé en clair";
/// What an SMTP server offering neither PLAIN nor LOGIN is told.
pub const NO_SHARED_MECHANISM: &str = "Le serveur SMTP n’offre ni AUTH PLAIN ni AUTH LOGIN";
/// What a server refusing for now is told.
pub const TRY_LATER: &str = "Le serveur refuse pour l’instant : réessayez plus tard";
/// How long signing in may take, from the connection to the account's
/// acceptance, whatever each step's own delay.
pub const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(30);
/// How long leaving a server, the answer known, may take.
const LEAVE_TIMEOUT: Duration = Duration::from_secs(5);
/// What a server may send while a person signs in: greeting, capabilities,
/// TLS handshake and answers.
const SIGN_IN_READ_LIMIT: usize = 64 * 1024;
/// The name the platform gives itself (`EHLO`): no host of its own told.
const HELLO_NAME: &str = "[127.0.0.1]";
