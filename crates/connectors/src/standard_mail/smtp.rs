//! Signing in to a company's SMTP server as a person (ADR-205): a connection
//! the network policy allows (through the operator's proxy if any), what the
//! server sends bounded; implicit TLS, or STARTTLS said by the platform
//! itself (`starttls`), the server having to offer it; then the platform's
//! own TLS, the same as for implicit TLS, before lettre speaks SMTP over it
//! and any credential leaves (`AUTH PLAIN` or `LOGIN`), all within one
//! delay. The platform gives no host name of its own (`EHLO [127.0.0.1]`).

use std::net::Ipv4Addr;

use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::transport::smtp::client::AsyncSmtpConnection;
use lettre::transport::smtp::extension::ClientId;
use tokio::time::timeout;

use super::starttls::start_tls;
use super::wire::Wire;
use super::{
    CREDENTIALS_REFUSED, LEAVE_TIMEOUT, MailLogin, NO_SHARED_MECHANISM, NOT_A_MAIL_SERVER,
    SIGN_IN_READ_LIMIT, SIGN_IN_TIMEOUT, SmtpSecurity, SmtpServer, TRY_LATER,
};
use crate::outbound::{
    Cause, NOT_SECURE, NetworkPolicy, ReadBudget, TOO_LONG, TOO_SLOW, UNREACHABLE, bounded,
    connect, secured,
};

/// A connection of `login` to `server`, signed in, within the sign-in
/// delay, and what it may still read.
pub async fn smtp_connection(
    server: &SmtpServer,
    login: &MailLogin,
    policy: NetworkPolicy,
) -> Result<(AsyncSmtpConnection, ReadBudget), Cause> {
    let signed_in = match timeout(SIGN_IN_TIMEOUT, sign_in(server, login, policy)).await {
        Ok(connection) => connection,
        Err(_) => Err(Cause {
            summary: TOO_SLOW,
            detail: "connexion au serveur SMTP".into(),
        }),
    };
    // What the server said back never carries the password out.
    signed_in.map_err(|cause| Cause {
        detail: login.scrub(&cause.detail),
        ..cause
    })
}

/// Whether `login` signs in to `server`: the connection is left at once.
pub async fn check_smtp(
    server: &SmtpServer,
    login: &MailLogin,
    policy: NetworkPolicy,
) -> Result<(), Cause> {
    let (mut connection, _) = smtp_connection(server, login, policy).await?;
    let _ = timeout(LEAVE_TIMEOUT, connection.quit()).await;
    Ok(())
}

async fn sign_in(
    server: &SmtpServer,
    login: &MailLogin,
    policy: NetworkPolicy,
) -> Result<(AsyncSmtpConnection, ReadBudget), Cause> {
    let plain = connect(&server.host, server.port, policy).await?;
    let (mut plain, budget) = bounded(plain, SIGN_IN_READ_LIMIT);
    let told = |cause: Cause| match budget.spent() {
        true => Cause {
            summary: TOO_LONG,
            detail: cause.detail,
        },
        false => cause,
    };
    let hello = ClientId::Ipv4(Ipv4Addr::LOCALHOST);
    let wire = match server.security {
        SmtpSecurity::Tls => {
            Wire::greeted_by_server(secured(plain, &server.host, policy).await.map_err(told)?)
        }
        SmtpSecurity::StartTls => {
            start_tls(&mut plain).await.map_err(told)?;
            let secured = secured(plain, &server.host, policy).await.map_err(told)?;
            Wire::resumed_after_starttls(secured)
        }
    };
    let mut connection = AsyncSmtpConnection::connect_with_transport(Box::new(wire), &hello)
        .await
        .map_err(|error| told(failed(error)))?;
    let mechanisms = [Mechanism::Plain, Mechanism::Login];
    if connection
        .server_info()
        .get_auth_mechanism(&mechanisms)
        .is_none()
    {
        // No mechanism both sides know: nothing is sent.
        let _ = timeout(LEAVE_TIMEOUT, connection.quit()).await;
        return Err(Cause {
            summary: NO_SHARED_MECHANISM,
            detail: String::new(),
        });
    }
    let credentials = Credentials::new(login.username().into(), login.password().into());
    connection
        .auth(&mechanisms, &credentials)
        .await
        .map_err(|error| told(not_signed_in(error)))?;
    Ok((connection, budget))
}

/// A refused `AUTH`, told by what failed.
fn not_signed_in(error: lettre::transport::smtp::Error) -> Cause {
    let summary = if error.is_permanent() {
        CREDENTIALS_REFUSED
    } else if error.is_transient() {
        TRY_LATER
    } else {
        return failed(error);
    };
    Cause {
        summary,
        detail: error.to_string(),
    }
}

/// A failure of the exchange itself.
fn failed(error: lettre::transport::smtp::Error) -> Cause {
    let summary = if error.is_timeout() {
        TOO_SLOW
    } else if error.is_tls() {
        NOT_SECURE
    } else if error.is_response()
        || error.is_permanent()
        || error.is_transient()
        || error.is_client()
    {
        // An answer the exchange did not expect, a challenge among them.
        NOT_A_MAIL_SERVER
    } else {
        UNREACHABLE
    };
    Cause {
        summary,
        detail: error.to_string(),
    }
}
