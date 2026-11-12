//! An SMTP server for the tests (ADR-205), with a certificate of a test
//! authority: over implicit TLS, over STARTTLS, or in clear offering no
//! STARTTLS but `AUTH` all the same (what a client must never answer). It
//! takes `EHLO`, `STARTTLS`, `AUTH PLAIN` and `AUTH LOGIN` (those it
//! offers) for its accounts, `NOOP`, `RSET` and `QUIT`, and notes each
//! command with the layer it came over (`clair` or `tls`), never a
//! credential. It may also slip answers in clear after its STARTTLS
//! go-ahead, fall silent once it has greeted, greet without end, or quote
//! the password it refused.

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

/// What the server offers and does.
#[derive(Debug, Clone, Copy)]
pub struct SmtpSetup {
    pub mode: SmtpMode,
    /// The `AUTH` mechanisms it offers, once secured.
    pub mechanisms: &'static [&'static str],
    /// Whether it slips `250 AUTH PLAIN` then `235` in clear after its
    /// STARTTLS go-ahead, as someone on the path would.
    pub slips_answers_in_clear: bool,
    /// Whether it never answers once it has greeted.
    pub falls_silent: bool,
    /// Whether its greeting never ends.
    pub greets_endlessly: bool,
    /// Whether a refusal quotes the password it was given.
    pub quotes_refused_password: bool,
    /// Whether it refuses `AUTH` for now (`454`).
    pub refuses_for_now: bool,
}

impl SmtpSetup {
    pub fn new(mode: SmtpMode) -> Self {
        Self {
            mode,
            mechanisms: &["PLAIN", "LOGIN"],
            slips_answers_in_clear: false,
            falls_silent: false,
            greets_endlessly: false,
            quotes_refused_password: false,
            refuses_for_now: false,
        }
    }
}

/// A running SMTP server and what it heard.
pub struct FakeSmtp {
    pub address: SocketAddr,
    pub heard: Heard,
}

impl FakeSmtp {
    pub async fn start(authority: &TestAuthority, mode: SmtpMode, accounts: Accounts) -> Self {
        Self::with(authority, SmtpSetup::new(mode), accounts).await
    }

    pub async fn with(authority: &TestAuthority, setup: SmtpSetup, accounts: Accounts) -> Self {
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
                        setup,
                        accounts: &accounts,
                        heard: &noted,
                    };
                    if setup.mode == SmtpMode::ImplicitTls {
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
    setup: SmtpSetup,
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
        if greet && self.setup.greets_endlessly {
            let _ = stream.write_all(b"220 ").await;
            while stream.write_all(&[b'a'; 4096]).await.is_ok() {}
            return End::Closed;
        }
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
            if self.setup.falls_silent {
                continue;
            }
            let answer = match verb.as_str() {
                "EHLO" | "HELO" => self.capabilities(secure),
                "STARTTLS" if !secure && self.setup.mode == SmtpMode::StartTls => {
                    let mut go = "220 2.0.0 prêt pour TLS".to_string();
                    if self.setup.slips_answers_in_clear {
                        go.push_str("\r\n250 AUTH PLAIN\r\n235 2.7.0 authentifié");
                    }
                    if say(&mut stream, &go).await.is_err() {
                        return End::Closed;
                    }
                    return End::Upgrade(stream.into_inner());
                }
                "AUTH" if self.setup.refuses_for_now => "454 4.7.0 réessayez plus tard".into(),
                "AUTH" => match self.authenticated(&mut stream, argument).await {
                    Some(Ok(())) => "235 2.7.0 authentifié".into(),
                    Some(Err(password)) if self.setup.quotes_refused_password => {
                        format!("535 5.7.8 {password} refusé")
                    }
                    Some(Err(_)) => "535 5.7.8 identifiants invalides".into(),
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
        if !secure && self.setup.mode == SmtpMode::StartTls {
            lines.push("250-STARTTLS".into());
        }
        let offered = self.setup.mechanisms;
        if (secure || self.setup.mode == SmtpMode::ClearOnly) && !offered.is_empty() {
            lines.push(format!("250-AUTH {}", offered.join(" ")));
        }
        lines.push("250 8BITMIME".into());
        lines.join("\r\n")
    }

    /// Whether the account given to `AUTH` is one of its own, else the
    /// password refused; `None` if the client left meanwhile.
    async fn authenticated<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        stream: &mut BufReader<S>,
        argument: &str,
    ) -> Option<Result<(), String>> {
        let mut words = argument.split_whitespace();
        let mechanism = words.next().unwrap_or_default().to_ascii_uppercase();
        if !self.setup.mechanisms.contains(&mechanism.as_str()) {
            return Some(Err(String::new()));
        }
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
            _ => {
                say(stream, "334 VXNlcm5hbWU6").await.ok()?;
                let username = decoded(&next_line(stream).await?);
                say(stream, "334 UGFzc3dvcmQ6").await.ok()?;
                (username, decoded(&next_line(stream).await?))
            }
        };
        Some(match accepts(self.accounts, &username, &password) {
            true => Ok(()),
            false => Err(password),
        })
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
