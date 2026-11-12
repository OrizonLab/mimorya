//! Signing in to a company's mail servers as a person (ADR-205). IMAP over
//! implicit TLS and SMTP over implicit TLS or STARTTLS accept the person's
//! account and refuse another, told without the password; a password with
//! quotes, a backslash and an accent reaches the server as typed. Nothing
//! is said before TLS: a refused certificate ends the connection before any
//! account is given, and an SMTP server in clear offering no STARTTLS hears
//! no credential. A server that says `BYE`, or falls silent once it has
//! greeted, is told so within the sign-in delay. The operator's proxy
//! carries both protocols.

use std::net::SocketAddr;
use std::time::Duration;

use mimorya_connectors::outbound::{
    Cause, NOT_SECURE, NetworkPolicy, OutboundProxy, Redactor, TOO_SLOW, UNREACHABLE, explained,
};
use mimorya_connectors::standard_mail::{
    CREDENTIALS_REFUSED, ImapServer, MailLogin, NO_STARTTLS, SmtpSecurity, SmtpServer, check_imap,
    check_smtp,
};
use mimorya_connectors::testing::{
    Accounts, FakeImap, FakeSmtp, ImapBehaviour, SmtpMode, TestAuthority, serve_proxy,
};

const LOCAL: NetworkPolicy = NetworkPolicy {
    allow_private_network: true,
    allow_stdio: false,
    proxy: None,
    test_servers: &[],
    test_roots: &[],
};
const USER: &str = "paul.renaud@neo-maintenance.example";
const PASSWORD: &str = "mot de passe \"à\" \\ part";
const WRONG: &str = "autre-secret-de-paul";

fn trusting(authority: &TestAuthority) -> NetworkPolicy {
    NetworkPolicy {
        test_roots: authority.roots(),
        ..LOCAL
    }
}

fn accounts() -> Accounts {
    vec![(USER.into(), PASSWORD.into())]
}

fn login(password: &str) -> MailLogin {
    MailLogin::new(USER, password).unwrap()
}

fn imap_at(address: SocketAddr) -> ImapServer {
    ImapServer {
        host: address.ip().to_string(),
        port: address.port(),
    }
}

fn smtp_at(address: SocketAddr, security: SmtpSecurity) -> SmtpServer {
    SmtpServer {
        host: address.ip().to_string(),
        port: address.port(),
        security,
    }
}

/// The refusal as the person reads it, the password masked.
fn told(cause: Cause, password: &str) -> String {
    explained(cause, &Redactor::new(Some(password))).to_string()
}

fn no_credential_in(heard: &[String]) {
    assert!(heard.iter().all(|line| !line.contains("AUTH")), "{heard:?}");
}

#[tokio::test]
async fn a_person_signs_in_to_imap_with_their_account() {
    let authority = TestAuthority::new();
    let server = FakeImap::start(&authority, accounts()).await;
    check_imap(
        &imap_at(server.address),
        &login(PASSWORD),
        trusting(&authority),
    )
    .await
    .unwrap();
    assert_eq!(
        server.heard.lines(),
        [format!("LOGIN {USER}"), "LOGOUT".into()]
    );
}

#[tokio::test]
async fn imap_refuses_another_password_and_says_so_without_it() {
    let authority = TestAuthority::new();
    let server = FakeImap::start(&authority, accounts()).await;
    let cause = check_imap(
        &imap_at(server.address),
        &login(WRONG),
        trusting(&authority),
    )
    .await
    .unwrap_err();
    assert_eq!(cause.summary, CREDENTIALS_REFUSED, "{cause:?}");
    let said = told(cause, WRONG);
    assert!(!said.contains(WRONG), "{said}");
}

#[tokio::test]
async fn nothing_is_said_to_an_imap_server_whose_certificate_is_refused() {
    let authority = TestAuthority::new();
    let server = FakeImap::start(&authority, accounts()).await;
    let other = TestAuthority::new();
    let cause = check_imap(&imap_at(server.address), &login(PASSWORD), trusting(&other))
        .await
        .unwrap_err();
    assert_eq!(cause.summary, NOT_SECURE, "{cause:?}");
    assert!(
        server.heard.lines().is_empty(),
        "{:?}",
        server.heard.lines()
    );
}

#[tokio::test]
async fn an_imap_server_saying_bye_is_told_so() {
    let authority = TestAuthority::new();
    let server = FakeImap::behaving(&authority, accounts(), ImapBehaviour::SaysBye).await;
    let cause = check_imap(
        &imap_at(server.address),
        &login(PASSWORD),
        trusting(&authority),
    )
    .await
    .unwrap_err();
    assert_eq!(cause.summary, UNREACHABLE, "{cause:?}");
    assert!(cause.detail.contains("BYE"), "{cause:?}");
    assert!(server.heard.lines().is_empty());
}

#[tokio::test]
async fn an_imap_server_falling_silent_runs_out_of_the_sign_in_delay() {
    let authority = TestAuthority::new();
    let server = FakeImap::behaving(&authority, accounts(), ImapBehaviour::FallsSilent).await;
    let (imap, policy) = (imap_at(server.address), trusting(&authority));
    let attempt = tokio::spawn(async move { check_imap(&imap, &login(PASSWORD), policy).await });
    // Asked to sign in: from there, only the delay runs out.
    while server.heard.lines().is_empty() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::pause();
    let cause = attempt.await.unwrap().unwrap_err();
    assert_eq!(cause.summary, TOO_SLOW, "{cause:?}");
}

#[tokio::test]
async fn a_person_signs_in_to_smtp_over_implicit_tls_or_starttls() {
    let authority = TestAuthority::new();
    for (mode, security) in [
        (SmtpMode::ImplicitTls, SmtpSecurity::Tls),
        (SmtpMode::StartTls, SmtpSecurity::StartTls),
    ] {
        let server = FakeSmtp::start(&authority, mode, accounts()).await;
        let smtp = smtp_at(server.address, security);
        check_smtp(&smtp, &login(PASSWORD), trusting(&authority))
            .await
            .unwrap();
        let heard = server.heard.lines();
        assert!(heard.contains(&"tls AUTH PLAIN".to_string()), "{heard:?}");
        assert!(
            heard.iter().all(|line| !line.starts_with("clair AUTH")),
            "{heard:?}"
        );
        if mode == SmtpMode::StartTls {
            assert!(
                heard.starts_with(&["clair EHLO".into(), "clair STARTTLS".into()]),
                "{heard:?}"
            );
        }
    }
}

#[tokio::test]
async fn smtp_refuses_another_password_and_says_so_without_it() {
    let authority = TestAuthority::new();
    let server = FakeSmtp::start(&authority, SmtpMode::StartTls, accounts()).await;
    let smtp = smtp_at(server.address, SmtpSecurity::StartTls);
    let cause = check_smtp(&smtp, &login(WRONG), trusting(&authority))
        .await
        .unwrap_err();
    assert_eq!(cause.summary, CREDENTIALS_REFUSED, "{cause:?}");
    let said = told(cause, WRONG);
    assert!(!said.contains(WRONG), "{said}");
}

#[tokio::test]
async fn a_server_in_clear_offering_no_starttls_hears_no_credential() {
    let authority = TestAuthority::new();
    let server = FakeSmtp::start(&authority, SmtpMode::ClearOnly, accounts()).await;
    let smtp = smtp_at(server.address, SmtpSecurity::StartTls);
    let cause = check_smtp(&smtp, &login(PASSWORD), trusting(&authority))
        .await
        .unwrap_err();
    assert_eq!(cause.summary, NO_STARTTLS, "{cause:?}");
    // It offered AUTH in clear: the client still said nothing but goodbye.
    assert_eq!(server.heard.lines(), ["clair EHLO", "clair QUIT"]);
}

#[tokio::test]
async fn no_credential_leaves_toward_a_certificate_refused_or_absent() {
    let authority = TestAuthority::new();
    let other = TestAuthority::new();
    // STARTTLS to a certificate of another authority.
    let server = FakeSmtp::start(&authority, SmtpMode::StartTls, accounts()).await;
    let smtp = smtp_at(server.address, SmtpSecurity::StartTls);
    let cause = check_smtp(&smtp, &login(PASSWORD), trusting(&other))
        .await
        .unwrap_err();
    assert_eq!(cause.summary, NOT_SECURE, "{cause:?}");
    no_credential_in(&server.heard.lines());
    // Implicit TLS, to the same.
    let server = FakeSmtp::start(&authority, SmtpMode::ImplicitTls, accounts()).await;
    let smtp = smtp_at(server.address, SmtpSecurity::Tls);
    let cause = check_smtp(&smtp, &login(PASSWORD), trusting(&other))
        .await
        .unwrap_err();
    assert_eq!(cause.summary, NOT_SECURE, "{cause:?}");
    assert!(server.heard.lines().is_empty());
    // Implicit TLS declared, a server speaking in clear.
    let server = FakeSmtp::start(&authority, SmtpMode::ClearOnly, accounts()).await;
    let smtp = smtp_at(server.address, SmtpSecurity::Tls);
    let cause = check_smtp(&smtp, &login(PASSWORD), trusting(&authority))
        .await
        .unwrap_err();
    assert_eq!(cause.summary, NOT_SECURE, "{cause:?}");
    no_credential_in(&server.heard.lines());
}

#[tokio::test]
async fn the_operators_proxy_carries_imap_and_smtp() {
    let authority = TestAuthority::new();
    let imap = FakeImap::start(&authority, accounts()).await;
    let smtp = FakeSmtp::start(&authority, SmtpMode::StartTls, accounts()).await;
    let (proxy, proxied) = serve_proxy(None).await;
    let policy = NetworkPolicy {
        proxy: Some(OutboundProxy::new(&proxy, None).unwrap().leak()),
        ..trusting(&authority)
    };
    check_imap(&imap_at(imap.address), &login(PASSWORD), policy)
        .await
        .unwrap();
    check_smtp(
        &smtp_at(smtp.address, SmtpSecurity::StartTls),
        &login(PASSWORD),
        policy,
    )
    .await
    .unwrap();
    assert_eq!(
        proxied.targets(),
        [imap.address.to_string(), smtp.address.to_string()]
    );
}
