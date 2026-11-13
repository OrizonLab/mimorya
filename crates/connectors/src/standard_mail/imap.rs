//! Signing in to a company's IMAP server as a person (ADR-205): a connection
//! the network policy allows, what the server sends bounded, secured by TLS
//! before anything is said (implicit TLS: IMAP over STARTTLS is not
//! offered), the server's greeting read, then `LOGIN`, or `AUTHENTICATE
//! PLAIN` for an account beyond ASCII where the greeting offers it (a
//! quoted string is ASCII in IMAP4rev1), all within one delay; no literal
//! announced past what the session may read is made room for. A refused
//! account is told as such; what the server says is the caller's to mask
//! (`MailLogin::redactor`).

use std::borrow::Cow;

use async_imap::Authenticator;
use async_imap::error::Error as ImapError;
use async_imap::imap_proto::{Capability, Response, ResponseCode, Status};
use tokio::time::timeout;

use super::literals::{guarded, io_failure};
use super::{
    CREDENTIALS_REFUSED, FORM_REFUSED, ImapServer, LEAVE_TIMEOUT, MailLogin, NOT_A_MAIL_SERVER,
    SIGN_IN_READ_LIMIT, SIGN_IN_TIMEOUT, TRY_LATER,
};
use crate::outbound::{
    Cause, Connection, NetworkPolicy, ReadBudget, TOO_LONG, TOO_SLOW, UNREACHABLE, bounded,
    connect, secured,
};

/// A person's session on their IMAP server.
pub type ImapSession = async_imap::Session<Connection>;

/// A session of `login` on `server`, within the sign-in delay, and what it
/// may still read (a step reading more is granted it).
pub async fn imap_session(
    server: &ImapServer,
    login: &MailLogin,
    policy: NetworkPolicy,
) -> Result<(ImapSession, ReadBudget), Cause> {
    let signed_in = match timeout(SIGN_IN_TIMEOUT, sign_in(server, login, policy)).await {
        Ok(session) => session,
        Err(_) => Err(Cause {
            summary: TOO_SLOW,
            detail: "connexion au serveur IMAP".into(),
        }),
    };
    // What the server said back never carries the password out.
    signed_in.map_err(|cause| Cause {
        detail: login.scrub(&cause.detail),
        ..cause
    })
}

/// Whether `login` signs in to `server`: the session is left at once.
pub async fn check_imap(
    server: &ImapServer,
    login: &MailLogin,
    policy: NetworkPolicy,
) -> Result<(), Cause> {
    let (mut session, _) = imap_session(server, login, policy).await?;
    // Accepted already: a logout the server answers badly changes nothing.
    let _ = timeout(LEAVE_TIMEOUT, session.logout()).await;
    Ok(())
}

async fn sign_in(
    server: &ImapServer,
    login: &MailLogin,
    policy: NetworkPolicy,
) -> Result<(ImapSession, ReadBudget), Cause> {
    let plain = connect(&server.host, server.port, policy).await?;
    let (plain, budget) = bounded(plain, SIGN_IN_READ_LIMIT);
    let told = |cause: Cause| match budget.spent() {
        true => Cause {
            summary: TOO_LONG,
            detail: cause.detail,
        },
        false => cause,
    };
    let secured = secured(plain, &server.host, policy).await.map_err(told)?;
    // Nothing announced past the budget is made room for.
    let mut client = async_imap::Client::new(guarded(secured, budget.clone()));
    let offers_plain = greeted(&mut client).await.map_err(told)?;
    let ascii = login.username().is_ascii() && login.password().is_ascii();
    let session = if !ascii && offers_plain {
        let plain = Plain {
            username: login.username(),
            password: login.password(),
        };
        client.authenticate("PLAIN", plain).await
    } else {
        client.login(login.username(), login.password()).await
    };
    let session = session.map_err(|(error, _)| told(refused(error)))?;
    Ok((session, budget))
}

/// The server's greeting: `OK`, or the connection is not kept; whether it
/// offers `AUTH=PLAIN`.
async fn greeted(client: &mut async_imap::Client<Connection>) -> Result<bool, Cause> {
    let response = client.read_response().await.map_err(|error| {
        let (summary, detail) = io_failure(&error);
        Cause { summary, detail }
    })?;
    let Some(response) = response else {
        return Err(Cause {
            summary: UNREACHABLE,
            detail: "connexion fermée par le serveur".into(),
        });
    };
    match response.parsed() {
        Response::Data {
            status: Status::Ok,
            outcome,
        } => Ok(match &outcome.code {
            Some(ResponseCode::Capabilities(offered)) => offered
                .iter()
                .any(|c| matches!(c, Capability::Auth(m) if m.eq_ignore_ascii_case("PLAIN"))),
            _ => false,
        }),
        Response::Data {
            status: Status::Bye,
            ..
        } => Err(Cause {
            summary: UNREACHABLE,
            detail: "le serveur refuse la connexion (BYE)".into(),
        }),
        // Signed in before any account is given: not the person's session.
        _ => Err(Cause {
            summary: NOT_A_MAIL_SERVER,
            detail: "accueil inattendu".into(),
        }),
    }
}

/// `AUTHENTICATE PLAIN` (RFC 4616): the account in UTF-8, base64-encoded
/// by the library.
struct Plain<'a> {
    username: &'a str,
    password: &'a str,
}

impl Authenticator for Plain<'_> {
    type Response = String;

    fn process(&mut self, _challenge: &[u8]) -> String {
        format!("\0{}\0{}", self.username, self.password)
    }
}

/// What a server refusing for now says (RFC 5530): its directory down, a
/// fault of its own, the account in use, a limit reached. The account is
/// not taken for refused.
const FOR_NOW: [&str; 4] = ["[UNAVAILABLE]", "[SERVERBUG]", "[INUSE]", "[LIMIT]"];

/// A refused sign-in, told by what failed.
fn refused(error: ImapError) -> Cause {
    let (summary, detail): (&'static str, Cow<'_, str>) = match &error {
        ImapError::No(said) if for_now(said) => (TRY_LATER, said.into()),
        ImapError::No(said) => (CREDENTIALS_REFUSED, said.into()),
        ImapError::Bad(said) => (FORM_REFUSED, said.into()),
        ImapError::Io(error) => {
            let (summary, detail) = io_failure(error);
            (summary, detail.into())
        }
        ImapError::ConnectionLost => (UNREACHABLE, "connexion perdue".into()),
        other => (NOT_A_MAIL_SERVER, other.to_string().into()),
    };
    Cause {
        summary,
        detail: detail.into_owned(),
    }
}

/// Whether a refusal says the server refuses for now only.
fn for_now(said: &str) -> bool {
    let said = said.to_ascii_uppercase();
    FOR_NOW.iter().any(|code| said.contains(code))
}
