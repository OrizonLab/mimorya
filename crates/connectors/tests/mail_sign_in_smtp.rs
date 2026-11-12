//! Signing in to a company's SMTP server as a person (ADR-205): over
//! implicit TLS, or STARTTLS said by the platform itself, by `AUTH PLAIN` or
//! `LOGIN`, whichever the server offers; another password refused, what the
//! server quotes of it masked; no shared mechanism told as such. No
//! credential leaves in clear: a server offering no STARTTLS hears `EHLO`
//! and `QUIT` alone, a refused certificate (another authority, another
//! name) or answers slipped in clear after the STARTTLS go-ahead end the
//! connection first. A server that falls silent or greets without end is
//! told so. The operator's proxy carries IMAP and SMTP alike.

mod support;

use mimorya_connectors::outbound::{NOT_SECURE, NetworkPolicy, OutboundProxy, TOO_LONG, TOO_SLOW};
use mimorya_connectors::standard_mail::{
    CREDENTIALS_REFUSED, NO_SHARED_MECHANISM, NO_STARTTLS, SmtpSecurity, check_imap, check_smtp,
};
use mimorya_connectors::testing::{
    FakeImap, FakeSmtp, SmtpMode, SmtpSetup, TestAuthority, serve_proxy,
};
use support::mail::{
    PASSWORD, WRONG, accounts, hides, imap_at, login, masked, no_credential_in, smtp_at, trusting,
    until_heard,
};

fn setup(mode: SmtpMode) -> SmtpSetup {
    SmtpSetup::new(mode)
}

#[tokio::test]
async fn a_person_signs_in_over_implicit_tls_or_starttls() {
    let authority = TestAuthority::new();
    let server = FakeSmtp::start(&authority, SmtpMode::ImplicitTls, accounts()).await;
    let smtp = smtp_at(server.address, SmtpSecurity::Tls);
    check_smtp(&smtp, &login(PASSWORD), trusting(&authority))
        .await
        .unwrap();
    assert_eq!(
        server.heard.lines(),
        ["tls EHLO", "tls AUTH PLAIN", "tls QUIT"]
    );
    let server = FakeSmtp::start(&authority, SmtpMode::StartTls, accounts()).await;
    let smtp = smtp_at(server.address, SmtpSecurity::StartTls);
    check_smtp(&smtp, &login(PASSWORD), trusting(&authority))
        .await
        .unwrap();
    assert_eq!(
        server.heard.lines(),
        [
            "clair EHLO",
            "clair STARTTLS",
            "tls EHLO",
            "tls AUTH PLAIN",
            "tls QUIT"
        ]
    );
}

#[tokio::test]
async fn auth_login_serves_where_plain_is_not_offered_and_none_is_told() {
    let authority = TestAuthority::new();
    let login_only = SmtpSetup {
        mechanisms: &["LOGIN"],
        ..setup(SmtpMode::StartTls)
    };
    let server = FakeSmtp::with(&authority, login_only, accounts()).await;
    let smtp = smtp_at(server.address, SmtpSecurity::StartTls);
    check_smtp(&smtp, &login(PASSWORD), trusting(&authority))
        .await
        .unwrap();
    assert!(server.heard.lines().contains(&"tls AUTH LOGIN".into()));
    let none = SmtpSetup {
        mechanisms: &[],
        ..setup(SmtpMode::StartTls)
    };
    let server = FakeSmtp::with(&authority, none, accounts()).await;
    let smtp = smtp_at(server.address, SmtpSecurity::StartTls);
    let cause = check_smtp(&smtp, &login(PASSWORD), trusting(&authority))
        .await
        .unwrap_err();
    assert_eq!(cause.summary, NO_SHARED_MECHANISM, "{cause:?}");
    no_credential_in(&server.heard.lines());
}

#[tokio::test]
async fn another_password_is_refused_and_masked_where_the_server_quotes_it() {
    let authority = TestAuthority::new();
    let quoting = SmtpSetup {
        quotes_refused_password: true,
        ..setup(SmtpMode::StartTls)
    };
    let server = FakeSmtp::with(&authority, quoting, accounts()).await;
    let wrong = login(WRONG);
    let smtp = smtp_at(server.address, SmtpSecurity::StartTls);
    let cause = check_smtp(&smtp, &wrong, trusting(&authority))
        .await
        .unwrap_err();
    assert_eq!(cause.summary, CREDENTIALS_REFUSED, "{cause:?}");
    // The server said it back: taken out at once, masked again for the
    // person.
    hides(&cause.detail, WRONG);
    assert!(cause.detail.contains("***"), "{cause:?}");
    hides(&masked(cause, &wrong), WRONG);
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
    // It offered AUTH in clear: the client said nothing but goodbye.
    assert_eq!(server.heard.lines(), ["clair EHLO", "clair QUIT"]);
}

#[tokio::test]
async fn no_credential_leaves_toward_a_certificate_refused() {
    let authority = TestAuthority::new();
    let other = TestAuthority::new();
    let elsewhere = TestAuthority::naming(&["smtp.example.com"]);
    for (server_authority, trusted, mode, security) in [
        (
            &authority,
            &other,
            SmtpMode::StartTls,
            SmtpSecurity::StartTls,
        ),
        (&authority, &other, SmtpMode::ImplicitTls, SmtpSecurity::Tls),
        (
            &elsewhere,
            &elsewhere,
            SmtpMode::StartTls,
            SmtpSecurity::StartTls,
        ),
        (
            &elsewhere,
            &elsewhere,
            SmtpMode::ImplicitTls,
            SmtpSecurity::Tls,
        ),
        // Implicit TLS declared, a server speaking in clear.
        (
            &authority,
            &authority,
            SmtpMode::ClearOnly,
            SmtpSecurity::Tls,
        ),
    ] {
        let server = FakeSmtp::start(server_authority, mode, accounts()).await;
        let smtp = smtp_at(server.address, security);
        let cause = check_smtp(&smtp, &login(PASSWORD), trusting(trusted))
            .await
            .unwrap_err();
        assert_eq!(cause.summary, NOT_SECURE, "{mode:?}: {cause:?}");
        no_credential_in(&server.heard.lines());
    }
}

#[tokio::test]
async fn answers_slipped_in_clear_after_starttls_are_never_taken() {
    let authority = TestAuthority::new();
    let slipping = SmtpSetup {
        slips_answers_in_clear: true,
        ..setup(SmtpMode::StartTls)
    };
    let server = FakeSmtp::with(&authority, slipping, accounts()).await;
    let smtp = smtp_at(server.address, SmtpSecurity::StartTls);
    // A wrong password: `235` slipped in clear would have accepted it.
    let cause = check_smtp(&smtp, &login(WRONG), trusting(&authority))
        .await
        .unwrap_err();
    assert_eq!(cause.summary, NOT_SECURE, "{cause:?}");
    no_credential_in(&server.heard.lines());
}

#[tokio::test]
async fn a_server_falling_silent_or_greeting_without_end_is_told_so() {
    let authority = TestAuthority::new();
    let silent = SmtpSetup {
        falls_silent: true,
        ..setup(SmtpMode::StartTls)
    };
    let server = FakeSmtp::with(&authority, silent, accounts()).await;
    let (smtp, policy) = (
        smtp_at(server.address, SmtpSecurity::StartTls),
        trusting(&authority),
    );
    let attempt = tokio::spawn(async move { check_smtp(&smtp, &login(PASSWORD), policy).await });
    until_heard(&server.heard).await;
    tokio::time::pause();
    let cause = attempt.await.unwrap().unwrap_err();
    assert_eq!(cause.summary, TOO_SLOW, "{cause:?}");
    tokio::time::resume();
    for (mode, security) in [
        (SmtpMode::StartTls, SmtpSecurity::StartTls),
        (SmtpMode::ImplicitTls, SmtpSecurity::Tls),
    ] {
        let endless = SmtpSetup {
            greets_endlessly: true,
            ..setup(mode)
        };
        let server = FakeSmtp::with(&authority, endless, accounts()).await;
        let smtp = smtp_at(server.address, security);
        let cause = check_smtp(&smtp, &login(PASSWORD), trusting(&authority))
            .await
            .unwrap_err();
        assert_eq!(cause.summary, TOO_LONG, "{mode:?}: {cause:?}");
        assert!(server.heard.lines().is_empty());
    }
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
