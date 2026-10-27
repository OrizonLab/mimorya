use uuid::Uuid;

use super::*;
use crate::identity_providers::TokenClaims;

const DIRECTORY: &str = "72f988bf-86f1-41af-91ab-2d7cd011db47";
const CLIENT: &str = "0b1c2d3e-4f50-6172-8394-a5b6c7d8e9f0";

#[test]
fn an_application_is_its_directory_and_client_ids() {
    let application = microsoft365_application(DIRECTORY, CLIENT, "secret~valeur.1").unwrap();
    assert_eq!(application.directory.to_string(), DIRECTORY);
    assert_eq!(application.client_id.to_string(), CLIENT);
    assert_eq!(
        application.issuer,
        format!("https://login.microsoftonline.com/{DIRECTORY}/v2.0")
    );
    assert!(
        microsoft365_application("contoso.onmicrosoft.com", CLIENT, "s")
            .unwrap_err()
            .contains("ID de locataire")
    );
    assert!(
        microsoft365_application(DIRECTORY, "mimorya", "s")
            .unwrap_err()
            .contains("ID client")
    );
    assert!(
        microsoft365_application(DIRECTORY, CLIENT, "deux mots")
            .unwrap_err()
            .contains("secret client")
    );
    assert!(microsoft365_application(DIRECTORY, CLIENT, "").is_err());
}

fn claims<'a>(username: Option<&'a str>, email: Option<&'a str>) -> TokenClaims<'a> {
    TokenClaims {
        subject: "sujet",
        oid: Some("objet-1"),
        preferred_username: username,
        email,
        ..TokenClaims::default()
    }
}

#[test]
fn a_link_is_the_persons_own_account_by_its_sign_in_name() {
    let identity = linked_identity(
        &claims(Some("Claire.Martin@Neo.example"), None),
        "claire.martin@neo.example",
    )
    .unwrap();
    assert_eq!(identity.subject, "objet-1");
    assert_eq!(identity.address, "claire.martin@neo.example");
}

#[test]
fn another_account_is_never_linked() {
    let other = claims(Some("paul.durand@neo.example"), None);
    assert_eq!(
        linked_identity(&other, "claire.martin@neo.example"),
        Err(LinkRefusal::NotTheirs)
    );
}

#[test]
fn an_address_counts_only_when_the_directory_verified_its_domain() {
    let mut unverified = claims(Some("cmartin"), Some("claire.martin@neo.example"));
    assert_eq!(
        linked_identity(&unverified, "claire.martin@neo.example"),
        Err(LinkRefusal::Unidentified)
    );
    unverified.email_domain_verified = Some(true);
    assert_eq!(
        linked_identity(&unverified, "claire.martin@neo.example")
            .unwrap()
            .address,
        "claire.martin@neo.example"
    );
}

#[test]
fn the_sign_in_name_wins_over_another_verified_address() {
    // As a sign-in reads them: the sign-in name names the account.
    let mut token = claims(
        Some("paul.durand@neo.example"),
        Some("claire.martin@neo.example"),
    );
    token.email_domain_verified = Some(true);
    assert_eq!(
        linked_identity(&token, "claire.martin@neo.example"),
        Err(LinkRefusal::NotTheirs)
    );
}

#[test]
fn a_token_without_object_id_identifies_nobody() {
    let mut token = claims(Some("claire.martin@neo.example"), None);
    token.oid = None;
    assert_eq!(
        linked_identity(&token, "claire.martin@neo.example"),
        Err(LinkRefusal::Unidentified)
    );
}

#[test]
fn granted_scopes_are_kept_once_each_and_bounded() {
    assert_eq!(
        granted_scopes(Some("Mail.Read  openid Mail.Read offline_access")),
        "Mail.Read openid offline_access"
    );
    assert_eq!(granted_scopes(None), "");
    let many: Vec<String> = (0..400).map(|n| format!("scope.{n}")).collect();
    let kept = granted_scopes(Some(&many.join(" ")));
    assert!(kept.len() <= MAX_SCOPES_CHARS);
    assert!(kept.starts_with("scope.0 scope.1 "));
}

#[test]
fn a_sealed_token_names_its_connector_and_person() {
    let (connector, user) = (Uuid::nil(), Uuid::max());
    assert_eq!(
        sealed_as(connector, user),
        format!("account_link.{connector}.{user}")
    );
}

#[test]
fn a_link_is_renewed_with_what_the_person_granted() {
    assert_eq!(
        renewal_scopes("openid profile email offline_access mail.read User.Read"),
        ["email", "profile", "offline_access", "Mail.Read"]
    );
    // A permission the platform asks now, not granted then, is not asked.
    assert!(!renewal_scopes("offline_access Mail.Read").contains(&"Calendars.Read"));
    assert_eq!(renewal_scopes(" "), MICROSOFT365_SCOPES);
    // Graph's permissions by their address; offline access always.
    assert_eq!(
        renewal_scopes(
            "https://graph.microsoft.com/Mail.Read https://graph.microsoft.com/User.Read"
        ),
        ["offline_access", "Mail.Read"]
    );
}
