//! Signing in to a company's IMAP server as a person (ADR-205): a connection
//! the network policy allows, secured by TLS before anything is said
//! (implicit TLS: IMAP over STARTTLS is not offered), the server's greeting
//! read, then `LOGIN`, all within one delay. A refused account is told as
//! such; what the server says is the caller's to mask (`MailLogin::redactor`).

use async_imap::error::Error as ImapError;
use async_imap::imap_proto::{Response, Status};
use tokio::time::timeout;

use super::{CREDENTIALS_REFUSED, ImapServer, MailLogin, NOT_A_MAIL_SERVER, SIGN_IN_TIMEOUT};
use crate::outbound::{Cause, Connection, NetworkPolicy, TOO_SLOW, UNREACHABLE, connect, secured};

/// A person's session on their IMAP server.
pub type ImapSession = async_imap::Session<Connection>;

/// A session of `login` on `server`, within the sign-in delay.
pub async fn imap_session(
    server: &ImapServer,
    login: &MailLogin,
    policy: NetworkPolicy,
) -> Result<ImapSession, Cause> {
    match timeout(SIGN_IN_TIMEOUT, sign_in(server, login, policy)).await {
        Ok(session) => session,
        Err(_) => Err(Cause {
            summary: TOO_SLOW,
            detail: "connexion au serveur IMAP".into(),
        }),
    }
}

/// Whether `login` signs in to `server`: the session is left at once.
pub async fn check_imap(
    server: &ImapServer,
    login: &MailLogin,
    policy: NetworkPolicy,
) -> Result<(), Cause> {
    let mut session = imap_session(server, login, policy).await?;
    // Accepted already: a logout the server answers badly changes nothing.
    let _ = timeout(SIGN_IN_TIMEOUT, session.logout()).await;
    Ok(())
}

async fn sign_in(
    server: &ImapServer,
    login: &MailLogin,
    policy: NetworkPolicy,
) -> Result<ImapSession, Cause> {
    let plain = connect(&server.host, server.port, policy).await?;
    let secured = secured(plain, &server.host, policy).await?;
    let mut client = async_imap::Client::new(secured);
    greeted(&mut client).await?;
    client
        .login(login.username(), login.password())
        .await
        .map_err(|(error, _)| refused(error))
}

/// The server's greeting: `OK`, or the connection is not kept.
async fn greeted(client: &mut async_imap::Client<Connection>) -> Result<(), Cause> {
    let response = client.read_response().await.map_err(|error| Cause {
        summary: UNREACHABLE,
        detail: error.to_string(),
    })?;
    let Some(response) = response else {
        return Err(Cause {
            summary: UNREACHABLE,
            detail: "connexion fermée par le serveur".into(),
        });
    };
    match response.parsed() {
        Response::Data {
            status: Status::Ok, ..
        } => Ok(()),
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

/// A refused `LOGIN`, told by what failed.
fn refused(error: ImapError) -> Cause {
    match error {
        ImapError::No(said) => Cause {
            summary: CREDENTIALS_REFUSED,
            detail: said,
        },
        ImapError::Io(error) => Cause {
            summary: UNREACHABLE,
            detail: error.to_string(),
        },
        ImapError::ConnectionLost => Cause {
            summary: UNREACHABLE,
            detail: "connexion perdue".into(),
        },
        other => Cause {
            summary: NOT_A_MAIL_SERVER,
            detail: other.to_string(),
        },
    }
}
