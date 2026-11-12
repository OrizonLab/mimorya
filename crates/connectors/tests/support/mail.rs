//! What the tests of signing in to a company's mail servers share
//! (ADR-205): a network policy trusting a test authority, the account the
//! simulated servers accept, where they listen, and what a person reads of
//! a refusal, the password masked.

use std::net::SocketAddr;
use std::time::Duration;

use mimorya_connectors::outbound::{Cause, NetworkPolicy, explained};
use mimorya_connectors::standard_mail::{ImapServer, MailLogin, SmtpSecurity, SmtpServer};
use mimorya_connectors::testing::{Accounts, Heard, TestAuthority};

pub const MAIL_LOCAL: NetworkPolicy = NetworkPolicy {
    allow_private_network: true,
    allow_stdio: false,
    proxy: None,
    test_servers: &[],
    test_roots: &[],
};
pub const USER: &str = "paul.renaud@neo-maintenance.example";
/// ASCII, with what IMAP must quote: guillemets and a backslash.
pub const PASSWORD: &str = r#"mot de passe "entre" \ guillemets"#;
/// Beyond ASCII.
pub const ACCENTED: &str = "mot de passe accentué à Noël";
/// Another password, refused, quoted back by some servers: an apostrophe,
/// guillemets and a backslash, spelt differently by each escaping.
pub const WRONG: &str = r#"l'autre "secret" \ de paul"#;

pub fn trusting(authority: &TestAuthority) -> NetworkPolicy {
    NetworkPolicy {
        test_roots: authority.roots(),
        ..MAIL_LOCAL
    }
}

pub fn accounts() -> Accounts {
    vec![
        (USER.into(), PASSWORD.into()),
        (USER.into(), ACCENTED.into()),
    ]
}

pub fn login(password: &str) -> MailLogin {
    MailLogin::new(USER, password).unwrap()
}

pub fn imap_at(address: SocketAddr) -> ImapServer {
    ImapServer {
        host: address.ip().to_string(),
        port: address.port(),
    }
}

pub fn smtp_at(address: SocketAddr, security: SmtpSecurity) -> SmtpServer {
    SmtpServer {
        host: address.ip().to_string(),
        port: address.port(),
        security,
    }
}

/// The refusal as the person reads it: masked by their account's redactor.
pub fn masked(cause: Cause, login: &MailLogin) -> String {
    explained(cause, &login.redactor()).to_string()
}

/// Neither the password nor its quoted form (`\"`, `\\`), nor either as
/// `{:?}` or `escape_debug` writes it.
pub fn hides(text: &str, password: &str) {
    let quoted = password.replace('\\', "\\\\").replace('"', "\\\"");
    let debugged = |form: &str| format!("{form:?}").trim_matches('"').to_string();
    for spelling in [
        password.to_string(),
        debugged(password),
        password.escape_debug().to_string(),
        debugged(&quoted),
        quoted.escape_debug().to_string(),
        quoted,
    ] {
        assert!(!text.contains(&spelling), "{spelling} in {text}");
    }
    // The secret's core never shows, however it was escaped.
    assert!(!text.contains("secret"), "{text}");
}

pub fn no_credential_in(heard: &[String]) {
    assert!(heard.iter().all(|line| !line.contains("AUTH")), "{heard:?}");
}

/// Waits until the server heard something.
pub async fn until_heard(heard: &Heard) {
    while heard.lines().is_empty() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
