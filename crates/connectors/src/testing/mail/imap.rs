//! An IMAP server for the tests (ADR-205): implicit TLS with a certificate
//! of a test authority, the accounts it accepts, `CAPABILITY`, `LOGIN`
//! (quoted strings as RFC 3501 writes them), `AUTHENTICATE PLAIN` where it
//! offers it, `NOOP` and `LOGOUT`, and, once signed in, the folders the test
//! gives (`mailbox`). It notes each command, never a password. It may refuse
//! at its greeting, fall silent once it has greeted, greet without end or
//! with a structure nested without end,
//! quote the password it refused (what the platform must mask), or hold its
//! answer to an account until the test lets it through.

use std::net::SocketAddr;
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

use super::lines::{next_command, next_line};
use super::mailbox::{FetchBehaviour, Mailboxes, nested_without_end};
use super::{Accounts, Heard, accepts};
use crate::testing::{Hold, TestAuthority};

/// How the server behaves once a client is connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImapBehaviour {
    /// Greets and answers.
    Answers,
    /// Greets with `BYE`: it takes no client.
    SaysBye,
    /// Greets, then never answers again.
    FallsSilent,
    /// Greets without ever ending its line.
    GreetsEndlessly,
    /// Greets with a structure nested without end, before any account.
    GreetsNested,
}

/// What the server offers and does.
#[derive(Debug, Clone, Copy)]
pub struct ImapSetup {
    pub behaviour: ImapBehaviour,
    /// Whether its greeting offers `AUTH=PLAIN`.
    pub offers_plain: bool,
    /// Whether a refusal quotes the password it was given.
    pub quotes_refused_password: bool,
    /// Whether it answers `BAD` to `LOGIN` (as a strict IMAP4rev1 server
    /// answers a quoted string beyond ASCII).
    pub refuses_login_form: bool,
    /// Whether it refuses every account for now (`NO [UNAVAILABLE]`, its
    /// directory down).
    pub refuses_for_now: bool,
}

impl ImapSetup {
    pub fn new(behaviour: ImapBehaviour) -> Self {
        Self {
            behaviour,
            offers_plain: true,
            quotes_refused_password: false,
            refuses_login_form: false,
            refuses_for_now: false,
        }
    }
}

/// What a server holds besides its accounts: the folders they see, and
/// what holds its answer to an account.
#[derive(Clone, Default)]
struct Extras {
    mailboxes: Mailboxes,
    hold: Option<Arc<Hold>>,
}

/// A running IMAP server and what it heard.
pub struct FakeImap {
    pub address: SocketAddr,
    pub heard: Heard,
}

impl FakeImap {
    /// A server of `authority` accepting `accounts`.
    pub async fn start(authority: &TestAuthority, accounts: Accounts) -> Self {
        Self::with(authority, accounts, ImapSetup::new(ImapBehaviour::Answers)).await
    }

    pub async fn behaving(
        authority: &TestAuthority,
        accounts: Accounts,
        behaviour: ImapBehaviour,
    ) -> Self {
        Self::with(authority, accounts, ImapSetup::new(behaviour)).await
    }

    pub async fn with(authority: &TestAuthority, accounts: Accounts, setup: ImapSetup) -> Self {
        Self::serve(authority, accounts, setup, Extras::default()).await
    }

    /// A server answering an account only once `hold` lets it through.
    pub async fn held(authority: &TestAuthority, accounts: Accounts, hold: Arc<Hold>) -> Self {
        let setup = ImapSetup::new(ImapBehaviour::Answers);
        let extras = Extras {
            hold: Some(hold),
            ..Extras::default()
        };
        Self::serve(authority, accounts, setup, extras).await
    }

    /// A server whose accounts each see `mailboxes`.
    pub async fn holding(
        authority: &TestAuthority,
        accounts: Accounts,
        setup: ImapSetup,
        mailboxes: Mailboxes,
    ) -> Self {
        let extras = Extras {
            mailboxes,
            hold: None,
        };
        Self::serve(authority, accounts, setup, extras).await
    }

    async fn serve(
        authority: &TestAuthority,
        accounts: Accounts,
        setup: ImapSetup,
        extras: Extras,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("local port");
        let address = listener.local_addr().expect("local address");
        let heard = Heard::default();
        let (acceptor, noted) = (authority.acceptor(), heard.clone());
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let (acceptor, noted, accounts, extras) = (
                    acceptor.clone(),
                    noted.clone(),
                    accounts.clone(),
                    extras.clone(),
                );
                tokio::spawn(async move {
                    if let Ok(stream) = acceptor.accept(socket).await {
                        converse(stream, (&accounts, &extras), setup, &noted).await;
                    }
                });
            }
        });
        Self { address, heard }
    }
}

async fn converse<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    (accounts, extras): (&Accounts, &Extras),
    setup: ImapSetup,
    heard: &Heard,
) {
    let mut stream = BufReader::new(stream);
    let greeting = match setup.behaviour {
        ImapBehaviour::SaysBye => "* BYE serveur de test fermé\r\n".to_string(),
        _ if setup.offers_plain => "* OK [CAPABILITY IMAP4rev1 AUTH=PLAIN] prêt\r\n".into(),
        _ => "* OK [CAPABILITY IMAP4rev1] prêt\r\n".into(),
    };
    if setup.behaviour == ImapBehaviour::GreetsNested {
        let _ = stream.write_all(nested_without_end().as_bytes()).await;
        // Kept open until the client leaves.
        while next_line(&mut stream).await.is_some() {}
        return;
    }
    if setup.behaviour == ImapBehaviour::GreetsEndlessly {
        let _ = stream.write_all(b"* OK ").await;
        while stream.write_all(&[b'a'; 4096]).await.is_ok() {}
        return;
    }
    if stream.write_all(greeting.as_bytes()).await.is_err()
        || setup.behaviour == ImapBehaviour::SaysBye
    {
        return;
    }
    let (mut signed_in, mut examined) = (false, None);
    while let Some(line) = next_command(&mut stream).await {
        let (tag, rest) = line.split_once(' ').unwrap_or((line.as_str(), ""));
        let (command, arguments) = rest.split_once(' ').unwrap_or((rest, ""));
        let command = command.to_ascii_uppercase();
        match command.as_str() {
            "LOGIN" => heard.note(format!(
                "LOGIN {}",
                strings(arguments).first().cloned().unwrap_or_default()
            )),
            "AUTHENTICATE" => heard.note(format!("AUTHENTICATE {}", arguments.to_uppercase())),
            "UID" | "EXAMINE" | "SELECT" | "FETCH" => heard.note(format!("{command} {arguments}")),
            _ => heard.note(command.clone()),
        }
        if setup.behaviour == ImapBehaviour::FallsSilent {
            continue;
        }
        let account = match command.as_str() {
            "LOGIN" => match strings(arguments).as_slice() {
                [user, password] => Some((user.clone(), password.clone())),
                _ => None,
            },
            "AUTHENTICATE" if setup.offers_plain && arguments.eq_ignore_ascii_case("PLAIN") => {
                if stream.write_all(b"+ \r\n").await.is_err() {
                    return;
                }
                let Some(payload) = next_line(&mut stream).await else {
                    return;
                };
                plain(&payload)
            }
            _ => None,
        };
        if let (Some(hold), Some(_)) = (&extras.hold, &account) {
            hold.wait().await;
        }
        let mailboxes = &extras.mailboxes;
        let fetching = command == "FETCH" || arguments.to_ascii_uppercase().starts_with("FETCH");
        if signed_in && fetching && mailboxes.fetch == FetchBehaviour::Endless {
            let _ = stream
                .write_all(b"* 1 FETCH (UID 1 BODY[] {000000000000000000000400000000}\r\n")
                .await;
            while stream.write_all(&[b'a'; 4096]).await.is_ok() {}
            return;
        }
        let folders = signed_in
            .then(|| mailboxes.answer(tag, &command, arguments, &mut examined))
            .flatten();
        let answer = match (command.as_str(), account) {
            _ if folders.is_some() => folders.unwrap_or_default(),
            ("LOGIN", _) if setup.refuses_login_form => format!("{tag} BAD chaîne invalide\r\n"),
            ("LOGIN" | "AUTHENTICATE", Some(_)) if setup.refuses_for_now => {
                format!("{tag} NO [UNAVAILABLE] annuaire indisponible\r\n")
            }
            ("CAPABILITY", _) if mailboxes.literal_plus => {
                format!("* CAPABILITY IMAP4rev1 LITERAL+\r\n{tag} OK terminé\r\n")
            }
            ("CAPABILITY", _) => format!("* CAPABILITY IMAP4rev1\r\n{tag} OK terminé\r\n"),
            ("NOOP", _) => format!("{tag} OK NOOP terminé\r\n"),
            ("LOGIN" | "AUTHENTICATE", Some((user, password))) => {
                if accepts(accounts, &user, &password) {
                    signed_in = true;
                    format!("{tag} OK connecté\r\n")
                } else if setup.quotes_refused_password {
                    let quoted = password.replace('\\', "\\\\").replace('"', "\\\"");
                    format!("{tag} NO [AUTHENTICATIONFAILED] {password} refusé (\"{quoted}\")\r\n")
                } else {
                    format!("{tag} NO [AUTHENTICATIONFAILED] identifiants invalides\r\n")
                }
            }
            ("LOGOUT", _) => {
                let bye = format!("* BYE au revoir\r\n{tag} OK LOGOUT terminé\r\n");
                let _ = stream.write_all(bye.as_bytes()).await;
                return;
            }
            _ => format!("{tag} BAD commande inconnue\r\n"),
        };
        if stream.write_all(answer.as_bytes()).await.is_err() {
            return;
        }
    }
}

/// The account `AUTHENTICATE PLAIN` gives (RFC 4616).
fn plain(payload: &str) -> Option<(String, String)> {
    let bytes = STANDARD.decode(payload.trim()).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let mut parts = text.split('\0').skip(1);
    Some((parts.next()?.to_string(), parts.next()?.to_string()))
}

/// The strings of a command's arguments: atoms, or quoted strings whose `\`
/// escapes the next character.
pub(super) fn strings(arguments: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut chars = arguments.chars().peekable();
    while let Some(&next) = chars.peek() {
        if next == ' ' {
            chars.next();
            continue;
        }
        let mut word = String::new();
        if next == '"' {
            chars.next();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => word.extend(chars.next()),
                    '"' => break,
                    other => word.push(other),
                }
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c == ' ' {
                    break;
                }
                word.push(c);
                chars.next();
            }
        }
        found.push(word);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_atoms_and_quoted_strings() {
        assert_eq!(strings("paul secret"), ["paul", "secret"]);
        assert_eq!(
            strings(r#""paul@exemple.fr" "a \"b\" \\ c""#),
            ["paul@exemple.fr", r#"a "b" \ c"#]
        );
    }

    #[test]
    fn reads_the_account_authenticate_plain_gives() {
        let payload = STANDARD.encode("\0paul\0mot de passe à");
        assert_eq!(
            plain(&payload),
            Some(("paul".into(), "mot de passe à".into()))
        );
    }
}
