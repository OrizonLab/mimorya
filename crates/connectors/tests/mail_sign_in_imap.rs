//! Signing in to a company's IMAP server as a person (ADR-205): over TLS
//! from the first byte, the person's account accepted, by `LOGIN` (its
//! quotes and backslash quoted) or, beyond ASCII, by `AUTHENTICATE PLAIN`
//! where offered; another refused, what the server quotes of it masked.
//! Nothing is said to a server whose certificate is refused (another
//! authority, another name); a server that says `BYE`, falls silent or
//! greets without end is told so, within the sign-in delay and the bytes a
//! sign-in may read. A server refusing for now (its directory down) is
//! told so, the account not taken for refused.

mod support;

use mimorya_connectors::outbound::{NOT_SECURE, TOO_LONG, TOO_SLOW, UNREACHABLE};
use mimorya_connectors::standard_mail::{
    CREDENTIALS_REFUSED, FORM_REFUSED, NOT_A_MAIL_SERVER, TRY_LATER, check_imap,
};
use mimorya_connectors::testing::{FakeImap, ImapBehaviour, ImapSetup, TestAuthority};
use support::mail::{
    ACCENTED, PASSWORD, USER, WRONG, accounts, hides, imap_at, login, masked, trusting, until_heard,
};

#[tokio::test]
async fn a_person_signs_in_with_their_account() {
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
async fn an_account_beyond_ascii_signs_in_by_authenticate_plain_where_offered() {
    let authority = TestAuthority::new();
    let server = FakeImap::start(&authority, accounts()).await;
    check_imap(
        &imap_at(server.address),
        &login(ACCENTED),
        trusting(&authority),
    )
    .await
    .unwrap();
    assert_eq!(server.heard.lines(), ["AUTHENTICATE PLAIN", "LOGOUT"]);
    // Not offered: LOGIN still, which this server takes.
    let setup = ImapSetup {
        offers_plain: false,
        ..ImapSetup::new(ImapBehaviour::Answers)
    };
    let server = FakeImap::with(&authority, accounts(), setup).await;
    check_imap(
        &imap_at(server.address),
        &login(ACCENTED),
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
async fn another_password_is_refused_and_masked_where_the_server_quotes_it() {
    let authority = TestAuthority::new();
    let setup = ImapSetup {
        quotes_refused_password: true,
        ..ImapSetup::new(ImapBehaviour::Answers)
    };
    let server = FakeImap::with(&authority, accounts(), setup).await;
    let wrong = login(WRONG);
    let cause = check_imap(&imap_at(server.address), &wrong, trusting(&authority))
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
async fn a_server_refusing_the_login_form_is_told_so() {
    let authority = TestAuthority::new();
    let setup = ImapSetup {
        offers_plain: false,
        refuses_login_form: true,
        ..ImapSetup::new(ImapBehaviour::Answers)
    };
    let server = FakeImap::with(&authority, accounts(), setup).await;
    let cause = check_imap(
        &imap_at(server.address),
        &login(ACCENTED),
        trusting(&authority),
    )
    .await
    .unwrap_err();
    assert_eq!(cause.summary, FORM_REFUSED, "{cause:?}");
}

#[tokio::test]
async fn a_server_refusing_for_now_is_not_taken_for_a_refused_account() {
    let authority = TestAuthority::new();
    let setup = ImapSetup {
        refuses_for_now: true,
        ..ImapSetup::new(ImapBehaviour::Answers)
    };
    let server = FakeImap::with(&authority, accounts(), setup).await;
    let cause = check_imap(
        &imap_at(server.address),
        &login(PASSWORD),
        trusting(&authority),
    )
    .await
    .unwrap_err();
    assert_eq!(cause.summary, TRY_LATER, "{cause:?}");
}

#[tokio::test]
async fn nothing_is_said_to_a_server_whose_certificate_is_refused() {
    let authority = TestAuthority::new();
    let server = FakeImap::start(&authority, accounts()).await;
    let other = TestAuthority::new();
    let cause = check_imap(&imap_at(server.address), &login(PASSWORD), trusting(&other))
        .await
        .unwrap_err();
    assert_eq!(cause.summary, NOT_SECURE, "{cause:?}");
    assert!(server.heard.lines().is_empty());
    // A trusted certificate naming another server.
    let elsewhere = TestAuthority::naming(&["imap.example.com"]);
    let server = FakeImap::start(&elsewhere, accounts()).await;
    let cause = check_imap(
        &imap_at(server.address),
        &login(PASSWORD),
        trusting(&elsewhere),
    )
    .await
    .unwrap_err();
    assert_eq!(cause.summary, NOT_SECURE, "{cause:?}");
    assert!(server.heard.lines().is_empty());
}

#[tokio::test]
async fn a_server_saying_bye_is_told_so() {
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
async fn a_server_falling_silent_runs_out_of_the_sign_in_delay() {
    let authority = TestAuthority::new();
    let server = FakeImap::behaving(&authority, accounts(), ImapBehaviour::FallsSilent).await;
    let (imap, policy) = (imap_at(server.address), trusting(&authority));
    let attempt = tokio::spawn(async move { check_imap(&imap, &login(PASSWORD), policy).await });
    // Asked to sign in: from there, only the delay runs out.
    until_heard(&server.heard).await;
    tokio::time::pause();
    let cause = attempt.await.unwrap().unwrap_err();
    assert_eq!(cause.summary, TOO_SLOW, "{cause:?}");
}

#[tokio::test]
async fn a_server_nesting_without_end_is_refused_before_any_account() {
    let authority = TestAuthority::new();
    let server = FakeImap::behaving(&authority, accounts(), ImapBehaviour::GreetsNested).await;
    let cause = check_imap(
        &imap_at(server.address),
        &login(PASSWORD),
        trusting(&authority),
    )
    .await
    .unwrap_err();
    // Refused, not the process ended by the parser.
    assert_eq!(cause.summary, NOT_A_MAIL_SERVER, "{cause:?}");
    assert!(server.heard.lines().is_empty());
}

#[tokio::test]
async fn a_server_greeting_without_end_is_cut_short() {
    let authority = TestAuthority::new();
    let server = FakeImap::behaving(&authority, accounts(), ImapBehaviour::GreetsEndlessly).await;
    let cause = check_imap(
        &imap_at(server.address),
        &login(PASSWORD),
        trusting(&authority),
    )
    .await
    .unwrap_err();
    assert_eq!(cause.summary, TOO_LONG, "{cause:?}");
    assert!(server.heard.lines().is_empty());
}
