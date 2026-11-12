//! Signing in to a company's SMTP server as a person (ADR-205): implicit
//! TLS, or STARTTLS, which the server must offer, before any credential
//! leaves; lettre speaks SMTP over the connection the platform opened (the
//! network policy's, through the operator's proxy if any), all within one
//! delay. A server in clear that offers no STARTTLS hears nothing more than
//! `EHLO` and `QUIT`.

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::transport::smtp::client::{
    AsyncSmtpConnection, AsyncTokioStream, Certificate, TlsParameters,
};
use lettre::transport::smtp::extension::ClientId;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::time::timeout;

use super::{
    CREDENTIALS_REFUSED, MailLogin, NOT_A_MAIL_SERVER, SIGN_IN_TIMEOUT, SmtpSecurity, SmtpServer,
};
use crate::outbound::{
    Cause, Connection, NOT_SECURE, NetworkPolicy, TOO_SLOW, UNREACHABLE, connect, secured,
};

/// What a server offering no STARTTLS is told.
pub const NO_STARTTLS: &str =
    "Le serveur SMTP n’offre pas STARTTLS : rien ne lui est envoyé en clair";

/// A connection of `login` to `server`, signed in, within the sign-in delay.
pub async fn smtp_connection(
    server: &SmtpServer,
    login: &MailLogin,
    policy: NetworkPolicy,
) -> Result<AsyncSmtpConnection, Cause> {
    match timeout(SIGN_IN_TIMEOUT, sign_in(server, login, policy)).await {
        Ok(connection) => connection,
        Err(_) => Err(Cause {
            summary: TOO_SLOW,
            detail: "connexion au serveur SMTP".into(),
        }),
    }
}

/// Whether `login` signs in to `server`: the connection is left at once.
pub async fn check_smtp(
    server: &SmtpServer,
    login: &MailLogin,
    policy: NetworkPolicy,
) -> Result<(), Cause> {
    let mut connection = smtp_connection(server, login, policy).await?;
    let _ = timeout(SIGN_IN_TIMEOUT, connection.quit()).await;
    Ok(())
}

async fn sign_in(
    server: &SmtpServer,
    login: &MailLogin,
    policy: NetworkPolicy,
) -> Result<AsyncSmtpConnection, Cause> {
    let hello = ClientId::default();
    let plain = connect(&server.host, server.port, policy).await?;
    let mut connection = match server.security {
        SmtpSecurity::Tls => {
            let secured = secured(plain, &server.host, policy).await?;
            AsyncSmtpConnection::connect_with_transport(Box::new(Wire(secured)), &hello)
                .await
                .map_err(failed)?
        }
        SmtpSecurity::StartTls => {
            let mut connection =
                AsyncSmtpConnection::connect_with_transport(Box::new(Wire(plain)), &hello)
                    .await
                    .map_err(failed)?;
            if !connection.can_starttls() {
                let _ = connection.quit().await;
                return Err(Cause {
                    summary: NO_STARTTLS,
                    detail: server.host.clone(),
                });
            }
            let tls = tls_parameters(&server.host, policy)?;
            connection
                .starttls(tls, &hello)
                .await
                .map_err(|error| Cause {
                    summary: NOT_SECURE,
                    detail: error.to_string(),
                })?;
            connection
        }
    };
    let credentials = Credentials::new(login.username().into(), login.password().into());
    connection
        .auth(&[Mechanism::Plain, Mechanism::Login], &credentials)
        .await
        .map_err(|error| match error.is_permanent() {
            true => Cause {
                summary: CREDENTIALS_REFUSED,
                detail: error.to_string(),
            },
            false => failed(error),
        })?;
    Ok(connection)
}

/// How STARTTLS checks the server: the webpki roots, beside them the
/// tests' alone, its certificate naming the host.
fn tls_parameters(host: &str, policy: NetworkPolicy) -> Result<TlsParameters, Cause> {
    let refused = |detail: String| Cause {
        summary: NOT_SECURE,
        detail,
    };
    let mut builder = TlsParameters::builder(host.to_string());
    for root in policy.test_roots {
        let root = Certificate::from_der(root.to_vec()).map_err(|e| refused(e.to_string()))?;
        builder = builder.add_root_certificate(root);
    }
    builder.build_rustls().map_err(|e| refused(e.to_string()))
}

/// A failure of the exchange itself.
fn failed(error: lettre::transport::smtp::Error) -> Cause {
    let summary = if error.is_timeout() {
        TOO_SLOW
    } else if error.is_tls() {
        NOT_SECURE
    } else if error.is_response() || error.is_permanent() || error.is_transient() {
        NOT_A_MAIL_SERVER
    } else {
        UNREACHABLE
    };
    Cause {
        summary,
        detail: error.to_string(),
    }
}

/// The platform's connection as lettre takes one.
#[derive(Debug)]
struct Wire(Connection);

impl AsyncRead for Wire {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.0).poll_read(cx, buf)
    }
}

impl AsyncWrite for Wire {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut *self.0).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.0).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.0).poll_shutdown(cx)
    }
}

impl AsyncTokioStream for Wire {
    /// Through a tunnel, the server's address is the proxy's to know.
    fn peer_addr(&self) -> io::Result<SocketAddr> {
        Err(io::Error::other(
            "adresse tenue par la connexion de la plateforme",
        ))
    }
}
