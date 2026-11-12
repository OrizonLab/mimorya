//! An IMAP server for the tests (ADR-205): implicit TLS with a certificate
//! of a test authority, the accounts it accepts, `CAPABILITY`, `LOGIN`
//! (quoted strings as RFC 3501 writes them), `AUTHENTICATE PLAIN` where it
//! offers it, `NOOP` and `LOGOUT`. It notes each command, never a password.
//! It may refuse at its greeting, fall silent once it has greeted, greet
//! without end, or quote the password it refused (what the platform must
//! mask).

use std::net::SocketAddr;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

use super::lines::next_line;
use super::{Accounts, Heard, accepts};
use crate::testing::TestAuthority;

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
}

/// What the server offers and does.
#[derive(Debug, Clone, Copy)]
pub struct ImapSetup {
    pub behaviour: ImapBehaviour,
    /// Whether its greeting offers `AUTH=PLAIN`.
    pub offers_plain: bool,
    /// Whether a refusal quotes the password it was given.
    pub quotes_refused_password: bool,
}

impl ImapSetup {
    pub fn new(behaviour: ImapBehaviour) -> Self {
        Self {
            behaviour,
            offers_plain: true,
            quotes_refused_password: false,
        }
    }
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
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("local port");
        let address = listener.local_addr().expect("local address");
        let heard = Heard::default();
        let (acceptor, noted) = (authority.acceptor(), heard.clone());
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let (acceptor, noted, accounts) =
                    (acceptor.clone(), noted.clone(), accounts.clone());
                tokio::spawn(async move {
                    if let Ok(stream) = acceptor.accept(socket).await {
                        converse(stream, &accounts, setup, &noted).await;
                    }
                });
            }
        });
        Self { address, heard }
    }
}

async fn converse<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    accounts: &Accounts,
    setup: ImapSetup,
    heard: &Heard,
) {
    let mut stream = BufReader::new(stream);
    let greeting = match setup.behaviour {
        ImapBehaviour::SaysBye => "* BYE serveur de test fermé\r\n".to_string(),
        _ if setup.offers_plain => "* OK [CAPABILITY IMAP4rev1 AUTH=PLAIN] prêt\r\n".into(),
        _ => "* OK [CAPABILITY IMAP4rev1] prêt\r\n".into(),
    };
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
    while let Some(line) = next_line(&mut stream).await {
        let (tag, rest) = line.split_once(' ').unwrap_or((line.as_str(), ""));
        let (command, arguments) = rest.split_once(' ').unwrap_or((rest, ""));
        let command = command.to_ascii_uppercase();
        match command.as_str() {
            "LOGIN" => heard.note(format!(
                "LOGIN {}",
                strings(arguments).first().cloned().unwrap_or_default()
            )),
            "AUTHENTICATE" => heard.note(format!("AUTHENTICATE {}", arguments.to_uppercase())),
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
        let answer = match (command.as_str(), account) {
            ("CAPABILITY", _) => format!("* CAPABILITY IMAP4rev1\r\n{tag} OK terminé\r\n"),
            ("NOOP", _) => format!("{tag} OK NOOP terminé\r\n"),
            ("LOGIN" | "AUTHENTICATE", Some((user, password))) => {
                if accepts(accounts, &user, &password) {
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
fn strings(arguments: &str) -> Vec<String> {
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
