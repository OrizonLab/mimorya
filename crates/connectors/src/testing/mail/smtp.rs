//! An SMTP server for the tests (ADR-205), with a certificate of a test
//! authority: over implicit TLS, over STARTTLS, or in clear offering no
//! STARTTLS but `AUTH` all the same (what a client must never answer). It
//! takes `EHLO`, `STARTTLS`, `AUTH PLAIN` and `AUTH LOGIN` for its accounts,
//! `NOOP`, `RSET` and `QUIT`, and notes each command with the layer it came
//! over (`clair` or `tls`), never a credential.

use std::net::SocketAddr;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

use super::lines::next_line;
use super::{Accounts, Heard, accepts};
use crate::testing::TestAuthority;

/// How the server is secured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtpMode {
    /// TLS from the first byte.
    ImplicitTls,
    /// In clear until STARTTLS, which it offers.
    StartTls,
    /// In clear, offering `AUTH` but no STARTTLS.
    ClearOnly,
}

/// A running SMTP server and what it heard.
pub struct FakeSmtp {
    pub address: SocketAddr,
    pub heard: Heard,
}

impl FakeSmtp {
    pub async fn start(authority: &TestAuthority, mode: SmtpMode, accounts: Accounts) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("local port");
        let address = listener.local_addr().expect("local address");
        let heard = Heard::default();
        let (acceptor, noted) = (authority.acceptor(), heard.clone());
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let (acceptor, noted, accounts) =
                    (acceptor.clone(), noted.clone(), accounts.clone());
                tokio::spawn(async move {
                    let server = Server {
                        mode,
                        accounts: &accounts,
                        heard: &noted,
                    };
                    if mode == SmtpMode::ImplicitTls {
                        if let Ok(tls) = acceptor.accept(socket).await {
                            server.converse(tls, true, true).await;
                        }
                    } else if let End::Upgrade(socket) = server.converse(socket, false, true).await
                        && let Ok(tls) = acceptor.accept(socket).await
                    {
                        server.converse(tls, true, false).await;
                    }
                });
            }
        });
        Self { address, heard }
    }
}

/// How a conversation ended.
enum End<S> {
    Closed,
    /// STARTTLS: the connection to secure.
    Upgrade(S),
}

struct Server<'a> {
    mode: SmtpMode,
    accounts: &'a Accounts,
    heard: &'a Heard,
}

impl Server<'_> {
    async fn converse<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        stream: S,
        secure: bool,
        greet: bool,
    ) -> End<S> {
        let mut stream = BufReader::new(stream);
        let layer = if secure { "tls" } else { "clair" };
        if greet
            && say(&mut stream, "220 smtp.test ESMTP serveur de test")
                .await
                .is_err()
        {
            return End::Closed;
        }
        while let Some(line) = next_line(&mut stream).await {
            let (verb, argument) = line.split_once(' ').unwrap_or((line.as_str(), ""));
            let verb = verb.to_ascii_uppercase();
            let mechanism = argument.split_whitespace().next().unwrap_or_default();
            match verb.as_str() {
                "AUTH" => self.heard.note(format!("{layer} AUTH {mechanism}")),
                _ => self.heard.note(format!("{layer} {verb}")),
            }
            let answer = match verb.as_str() {
                "EHLO" | "HELO" => self.capabilities(secure),
                "STARTTLS" if !secure && self.mode == SmtpMode::StartTls => {
                    if say(&mut stream, "220 2.0.0 prêt pour TLS").await.is_err() {
                        return End::Closed;
                    }
                    return End::Upgrade(stream.into_inner());
                }
                "AUTH" => match self.authenticated(&mut stream, argument).await {
                    Some(true) => "235 2.7.0 authentifié".into(),
                    Some(false) => "535 5.7.8 identifiants invalides".into(),
                    None => return End::Closed,
                },
                "NOOP" | "RSET" => "250 2.0.0 ok".into(),
                "QUIT" => {
                    let _ = say(&mut stream, "221 2.0.0 au revoir").await;
                    return End::Closed;
                }
                _ => "502 5.5.2 commande non prise en charge".into(),
            };
            if say(&mut stream, &answer).await.is_err() {
                return End::Closed;
            }
        }
        End::Closed
    }

    /// What `EHLO` lists: STARTTLS while in clear if offered, `AUTH` once
    /// secured, or in clear by a server that offers nothing better.
    fn capabilities(&self, secure: bool) -> String {
        let mut lines = vec!["250-smtp.test".to_string()];
        if !secure && self.mode == SmtpMode::StartTls {
            lines.push("250-STARTTLS".into());
        }
        if secure || self.mode == SmtpMode::ClearOnly {
            lines.push("250-AUTH PLAIN LOGIN".into());
        }
        lines.push("250 8BITMIME".into());
        lines.join("\r\n")
    }

    /// Whether the account given to `AUTH` is one of its own; `None` if the
    /// client left meanwhile.
    async fn authenticated<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        stream: &mut BufReader<S>,
        argument: &str,
    ) -> Option<bool> {
        let mut words = argument.split_whitespace();
        let mechanism = words.next().unwrap_or_default().to_ascii_uppercase();
        let (username, password) = match mechanism.as_str() {
            "PLAIN" => {
                let payload = match words.next() {
                    Some(payload) => payload.to_string(),
                    None => {
                        say(stream, "334 ").await.ok()?;
                        next_line(stream).await?
                    }
                };
                let decoded = decoded(&payload);
                let mut parts = decoded.split('\0').skip(1);
                (parts.next()?.to_string(), parts.next()?.to_string())
            }
            "LOGIN" => {
                say(stream, "334 VXNlcm5hbWU6").await.ok()?;
                let username = decoded(&next_line(stream).await?);
                say(stream, "334 UGFzc3dvcmQ6").await.ok()?;
                (username, decoded(&next_line(stream).await?))
            }
            _ => return Some(false),
        };
        Some(accepts(self.accounts, &username, &password))
    }
}

fn decoded(payload: &str) -> String {
    let bytes = STANDARD.decode(payload.trim()).unwrap_or_default();
    String::from_utf8_lossy(&bytes).to_string()
}

async fn say<W: AsyncWrite + Unpin>(stream: &mut W, line: &str) -> std::io::Result<()> {
    stream.write_all(format!("{line}\r\n").as_bytes()).await?;
    stream.flush().await
}
