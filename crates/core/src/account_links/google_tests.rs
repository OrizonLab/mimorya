use super::*;
use crate::identity_providers::TokenClaims;

const GOOGLE: LinkKind = LinkKind::GoogleWorkspace;
const CLIENT: &str = "123456789012-abcdefghij0123456789.apps.googleusercontent.com";

#[test]
fn an_application_is_its_oauth_client() {
    let application =
        google_workspace_application(&format!(" {CLIENT} "), "GOCSPX-valeur_1").unwrap();
    assert_eq!(application.client_id, CLIENT);
    assert_eq!(application.issuer, "https://accounts.google.com");
    for refused in [
        "123456789012",
        ".apps.googleusercontent.com",
        "1234 5678.apps.googleusercontent.com",
        "1234/5678.apps.googleusercontent.com",
        "1234.apps.googleusercontent.com.evil.example",
    ] {
        assert!(
            google_workspace_application(refused, "GOCSPX-valeur_1")
                .unwrap_err()
                .contains("apps.googleusercontent.com"),
            "{refused}"
        );
    }
    let long = format!("{}{}", "a".repeat(250), ".apps.googleusercontent.com");
    assert!(google_workspace_application(&long, "GOCSPX-valeur_1").is_err());
    assert!(
        google_workspace_application(CLIENT, "deux mots")
            .unwrap_err()
            .contains("secret client")
    );
    assert!(google_workspace_application(CLIENT, "").is_err());
}

fn claims<'a>(email: Option<&'a str>, hosted: Option<&'a str>) -> TokenClaims<'a> {
    TokenClaims {
        subject: "110169484474386276334",
        email,
        email_verified: Some(true),
        hosted_domain: hosted,
        ..TokenClaims::default()
    }
}

#[test]
fn a_link_is_the_persons_own_workspace_account() {
    let identity = GOOGLE
        .linked_identity(
            &claims(Some("Claire.Martin@Neo.example"), Some("neo.example")),
            "claire.martin@neo.example",
        )
        .unwrap();
    assert_eq!(identity.subject, "110169484474386276334");
    assert_eq!(identity.address, "claire.martin@neo.example");
}

#[test]
fn another_account_is_never_linked() {
    let other = claims(Some("paul.durand@neo.example"), Some("neo.example"));
    assert_eq!(
        GOOGLE.linked_identity(&other, "claire.martin@neo.example"),
        Err(LinkRefusal::NotTheirs)
    );
}

#[test]
fn an_address_counts_only_when_google_verified_it_in_the_workspace_domain() {
    let account = "claire.martin@neo.example";
    let unverified = TokenClaims {
        email_verified: Some(false),
        ..claims(Some(account), Some("neo.example"))
    };
    let unsaid = TokenClaims {
        email_verified: None,
        ..claims(Some(account), Some("neo.example"))
    };
    // A consumer Google account bearing the person's address.
    let consumer = claims(Some(account), None);
    let elsewhere = claims(Some(account), Some("autre.example"));
    let without_address = claims(None, Some("neo.example"));
    for token in [unverified, unsaid, consumer, elsewhere, without_address] {
        assert_eq!(
            GOOGLE.linked_identity(&token, account),
            Err(LinkRefusal::Unidentified)
        );
    }
}

#[test]
fn a_token_without_subject_identifies_nobody() {
    let account = "claire.martin@neo.example";
    let token = TokenClaims {
        subject: "",
        ..claims(Some(account), Some("neo.example"))
    };
    assert_eq!(
        GOOGLE.linked_identity(&token, account),
        Err(LinkRefusal::Unidentified)
    );
    // Microsoft's object ID proves nothing at Google.
    let token = TokenClaims {
        oid: Some("objet-1"),
        preferred_username: Some(account),
        email_verified: None,
        ..claims(Some(account), None)
    };
    assert_eq!(
        GOOGLE.linked_identity(&token, account),
        Err(LinkRefusal::Unidentified)
    );
}

#[test]
fn a_link_asks_gmail_and_calendar_and_writing_when_let() {
    assert_eq!(
        GOOGLE.link_scopes(false),
        ["email", "profile", GMAIL_READONLY, CALENDAR_READONLY]
    );
    assert_eq!(
        GOOGLE.link_scopes(true),
        [
            "email",
            "profile",
            GMAIL_READONLY,
            CALENDAR_READONLY,
            GMAIL_SEND,
            CALENDAR_EVENTS
        ]
    );
    assert_eq!(
        GOOGLE.tool_scopes(false),
        [GMAIL_READONLY, CALENDAR_READONLY]
    );
    assert_eq!(
        GOOGLE.tool_scopes(true),
        [
            GMAIL_READONLY,
            CALENDAR_READONLY,
            GMAIL_SEND,
            CALENDAR_EVENTS
        ]
    );
}

#[test]
fn a_google_link_is_renewed_with_what_was_granted() {
    // Google renews what the person granted when no permission is named.
    assert!(
        GOOGLE
            .renewal_scopes("openid https://www.googleapis.com/auth/gmail.readonly")
            .is_empty()
    );
    assert!(GOOGLE.renewal_scopes("").is_empty());
}

#[test]
fn a_google_tool_is_told_which_permissions_were_not_granted() {
    let granted = "openid https://www.googleapis.com/auth/userinfo.email \
                   https://www.googleapis.com/auth/gmail.readonly \
                   https://www.googleapis.com/auth/gmail.send";
    assert!(
        GOOGLE
            .missing_scopes(granted, &[GMAIL_READONLY, GMAIL_SEND])
            .is_empty()
    );
    assert_eq!(
        GOOGLE.missing_scopes(granted, &[CALENDAR_READONLY, GMAIL_SEND]),
        [CALENDAR_READONLY]
    );
    assert_eq!(
        GOOGLE.serving_scopes(granted, true),
        [GMAIL_READONLY, GMAIL_SEND]
    );
    // Not said: reading, as the tools read.
    assert_eq!(
        GOOGLE.serving_scopes("", true),
        [GMAIL_READONLY, CALENDAR_READONLY]
    );
    // Microsoft's permission by the same last segment is not Google's.
    assert_eq!(
        GOOGLE.missing_scopes("Mail.Send", &[GMAIL_SEND]),
        [GMAIL_SEND]
    );
}

#[test]
fn google_is_asked_offline_access_consent_and_the_persons_domain() {
    assert_eq!(
        GOOGLE.authorization_params(" Claire.Martin@Neo.Example "),
        [
            ("access_type", "offline".to_string()),
            ("prompt", "consent".to_string()),
            ("hd", "neo.example".to_string())
        ]
    );
    assert!(
        LinkKind::Microsoft365
            .authorization_params("claire.martin@neo.example")
            .is_empty()
    );
}

#[test]
fn a_kind_is_found_by_its_connector_type() {
    assert_eq!(LinkKind::of("google_workspace"), Some(GOOGLE));
    assert_eq!(LinkKind::of("microsoft365"), Some(LinkKind::Microsoft365));
    assert_eq!(LinkKind::of("mcp"), None);
    for kind in LinkKind::ALL {
        assert_eq!(LinkKind::of(kind.connector_type()), Some(kind));
    }
    assert_eq!(GOOGLE.provider(), "Google");
    assert_eq!(GOOGLE.product(), "Google Workspace");
}
