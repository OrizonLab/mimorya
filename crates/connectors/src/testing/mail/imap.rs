//! An IMAP server for the tests (ADR-205): implicit TLS with a certificate
//! of a test authority, the accounts it accepts, `CAPABILITY`, `LOGIN`
//! (quoted strings as RFC 3501 writes them), `NOOP` and `LOGOUT`. It notes
//! each command, the password masked; it may refuse at its greeting or fall
//! silent once it has greeted.

use std::net::SocketAddr;

use tokio::io::{AsyncWriteExt, BufReader};
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
}

/// A running IMAP server and what it heard.
pub struct FakeImap {
    pub address: SocketAddr,
    pub heard: Heard,
}

impl FakeImap {
    /// A server of `authority` accepting `accounts`.
    pub async fn start(authority: &TestAuthority, accounts: Accounts) -> Self {
        Self::behaving(authority, accounts, ImapBehaviour::Answers).await
    }

    pub async fn behaving(
        authority: &TestAuthority,
        accounts: Accounts,
        behaviour: ImapBehaviour,
    ) -> Self {
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
                        converse(stream, &accounts, behaviour, &noted).await;
                    }
                });
            }
        });
        Self { address, heard }
    }
}

async fn converse<S>(stream: S, accounts: &Accounts, behaviour: ImapBehaviour, heard: &Heard)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);
    let greeting = match behaviour {
        ImapBehaviour::SaysBye => "* BYE serveur de test fermé\r\n",
        _ => "* OK [CAPABILITY IMAP4rev1] serveur de test prêt\r\n",
    };
    if writer.write_all(greeting.as_bytes()).await.is_err() || behaviour == ImapBehaviour::SaysBye {
        return;
    }
    while let Some(line) = next_line(&mut reader).await {
        let (tag, rest) = line.split_once(' ').unwrap_or((line.as_str(), ""));
        let (command, arguments) = rest.split_once(' ').unwrap_or((rest, ""));
        let command = command.to_ascii_uppercase();
        if command == "LOGIN" {
            heard.note(format!(
                "LOGIN {}",
                strings(arguments).first().cloned().unwrap_or_default()
            ));
        } else {
            heard.note(command.clone());
        }
        if behaviour == ImapBehaviour::FallsSilent {
            continue;
        }
        let answer = match command.as_str() {
            "CAPABILITY" => format!("* CAPABILITY IMAP4rev1\r\n{tag} OK CAPABILITY terminé\r\n"),
            "NOOP" => format!("{tag} OK NOOP terminé\r\n"),
            "LOGIN" => match strings(arguments).as_slice() {
                [user, password] if accepts(accounts, user, password) => {
                    format!("{tag} OK [CAPABILITY IMAP4rev1] connecté\r\n")
                }
                _ => format!("{tag} NO [AUTHENTICATIONFAILED] identifiants invalides\r\n"),
            },
            "LOGOUT" => {
                let bye = format!("* BYE au revoir\r\n{tag} OK LOGOUT terminé\r\n");
                let _ = writer.write_all(bye.as_bytes()).await;
                return;
            }
            _ => format!("{tag} BAD commande inconnue\r\n"),
        };
        if writer.write_all(answer.as_bytes()).await.is_err() {
            return;
        }
    }
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
    use super::strings;

    #[test]
    fn reads_atoms_and_quoted_strings() {
        assert_eq!(strings("paul secret"), ["paul", "secret"]);
        assert_eq!(
            strings(r#""paul@exemple.fr" "a \"b\" \\ c""#),
            ["paul@exemple.fr", r#"a "b" \ c"#]
        );
    }
}
