use super::*;

const DIRECTORY: &str = "72f988bf-86f1-41af-91ab-2d7cd011db47";

fn domains() -> Vec<String> {
    vec!["neo-maintenance.example".into()]
}

#[test]
fn each_kind_designates_its_issuer() {
    assert_eq!(
        declared_issuer(ProviderKind::Microsoft, Some(DIRECTORY), None).unwrap(),
        format!("https://login.microsoftonline.com/{DIRECTORY}/v2.0")
    );
    assert_eq!(
        declared_issuer(ProviderKind::Google, None, None).unwrap(),
        GOOGLE_ISSUER
    );
    assert_eq!(
        declared_issuer(ProviderKind::Oidc, None, Some("https://id.example.com")).unwrap(),
        "https://id.example.com"
    );
    for (kind, directory, issuer) in [
        (ProviderKind::Microsoft, None, None),
        // A domain name is not the directory's GUID (its issuer differs).
        (
            ProviderKind::Microsoft,
            Some("contoso.onmicrosoft.com"),
            None,
        ),
        (
            ProviderKind::Microsoft,
            Some(DIRECTORY),
            Some("https://id.example.com"),
        ),
        (ProviderKind::Google, Some(DIRECTORY), None),
        (ProviderKind::Google, None, Some("https://id.example.com")),
        (ProviderKind::Oidc, None, None),
        (
            ProviderKind::Oidc,
            Some(DIRECTORY),
            Some("https://id.example.com"),
        ),
        // Their own kinds check what a generic provider does not.
        (ProviderKind::Oidc, None, Some(GOOGLE_ISSUER)),
        (
            ProviderKind::Oidc,
            None,
            Some("https://Accounts.Google.com/"),
        ),
        (
            ProviderKind::Oidc,
            None,
            Some("https://login.microsoftonline.com/72f988bf-86f1-41af-91ab-2d7cd011db47/v2.0"),
        ),
        (
            ProviderKind::Oidc,
            None,
            Some("https://sts.windows.net/72f988bf-86f1-41af-91ab-2d7cd011db47/"),
        ),
        (
            ProviderKind::Oidc,
            None,
            Some("https://login.microsoftonline.us/72f988bf-86f1-41af-91ab-2d7cd011db47/v2.0"),
        ),
    ] {
        assert!(
            declared_issuer(kind, directory, issuer).is_err(),
            "{kind:?} {directory:?} {issuer:?}"
        );
    }
    assert_eq!(ProviderKind::parse("saml"), Some(ProviderKind::Saml));
    assert_eq!(ProviderKind::parse("ldap"), None);
    for kind in ["microsoft", "google", "oidc"] {
        assert_eq!(ProviderKind::parse(kind).unwrap().as_str(), kind);
    }
}

#[test]
fn a_generic_provider_needs_a_name() {
    assert_eq!(
        declared_name(ProviderKind::Microsoft, None).unwrap(),
        "Microsoft 365"
    );
    assert_eq!(
        declared_name(ProviderKind::Google, Some("Comptes Néo")).unwrap(),
        "Comptes Néo"
    );
    assert!(declared_name(ProviderKind::Oidc, None).is_err());
    assert!(declared_name(ProviderKind::Oidc, Some("K")).is_err());
    assert!(declared_name(ProviderKind::Oidc, Some("Key\ncloak")).is_err());
    assert!(declared_name(ProviderKind::Oidc, Some(&"é".repeat(81))).is_err());
    assert!(declared_name(ProviderKind::Oidc, Some(&"é".repeat(80))).is_ok());
}

#[test]
fn checks_the_client() {
    assert_eq!(client_id_problem("0a1b2c3d-app"), None);
    assert!(client_id_problem("").is_some());
    assert!(client_id_problem("a b").is_some());
    assert!(client_id_problem(&"a".repeat(256)).is_some());
    assert_eq!(client_secret_problem("Xy7~Q.abc_123"), None);
    assert!(client_secret_problem("").is_some());
    assert!(client_secret_problem("secret\n").is_some());
    assert!(client_secret_problem(&"a".repeat(1025)).is_some());
}

#[test]
fn domains_are_stored_once_in_lower_case() {
    let given = vec![
        " Neo-Maintenance.example".to_string(),
        "neo-maintenance.example".into(),
        "xn--nomaintenance-bkb.fr".into(),
    ];
    assert_eq!(
        normalise_domains(&given).unwrap(),
        ["neo-maintenance.example", "xn--nomaintenance-bkb.fr"]
    );
    for refused in [
        vec![],
        vec!["localhost".to_string()],
        vec!["@neo.example".into()],
        vec!["néo.fr".into()],
        vec!["-neo.example".into()],
        vec!["neo..example".into()],
        vec!["10.0.0.1".into()],
        vec!["neo example.fr".into()],
        (0..21).map(|i| format!("d{i}.example")).collect(),
    ] {
        assert!(normalise_domains(&refused).is_err(), "{refused:?}");
    }
    assert_eq!(
        email_domain("Nadia@Neo-Maintenance.example").as_deref(),
        Some("neo-maintenance.example")
    );
    assert_eq!(email_domain("nadia@"), None);
}

#[test]
fn microsoft_proves_its_directory_s_accounts_by_their_object_id() {
    let claims = TokenClaims {
        subject: "pairwise-sub",
        oid: Some("00000000-0000-0000-0000-0000000000aa"),
        preferred_username: Some("Nadia.Benali@neo-maintenance.example"),
        ..TokenClaims::default()
    };
    assert_eq!(
        proven_identity(ProviderKind::Microsoft, &claims, &domains()).unwrap(),
        ProvenIdentity {
            subject: "00000000-0000-0000-0000-0000000000aa".into(),
            email: Some("nadia.benali@neo-maintenance.example".into()),
        }
    );
    // The sign-in name, whose domain the directory verified, before `email`.
    let with_email = TokenClaims {
        email: Some("autre@neo-maintenance.example"),
        email_domain_verified: Some(true),
        ..claims.clone()
    };
    let email = |claims: &TokenClaims<'_>| {
        proven_identity(ProviderKind::Microsoft, claims, &domains())
            .unwrap()
            .email
    };
    assert_eq!(
        email(&with_email).as_deref(),
        Some("nadia.benali@neo-maintenance.example")
    );
    // Without a sign-in name that is an address, `email` only if Microsoft
    // vouches for its domain.
    let no_address = TokenClaims {
        preferred_username: Some("NBENALI"),
        ..with_email.clone()
    };
    assert_eq!(
        email(&no_address).as_deref(),
        Some("autre@neo-maintenance.example")
    );
    for email_domain_verified in [None, Some(false)] {
        let unverified = TokenClaims {
            email_domain_verified,
            ..no_address.clone()
        };
        assert_eq!(email(&unverified), None);
    }
    let no_oid = TokenClaims {
        oid: None,
        ..claims
    };
    assert!(proven_identity(ProviderKind::Microsoft, &no_oid, &domains()).is_err());
}

#[test]
fn google_proves_only_workspace_accounts_of_declared_domains() {
    let claims = TokenClaims {
        subject: "1234567890",
        email: Some("nadia@neo-maintenance.example"),
        email_verified: Some(true),
        hosted_domain: Some("neo-maintenance.example"),
        ..TokenClaims::default()
    };
    assert_eq!(
        proven_identity(ProviderKind::Google, &claims, &domains())
            .unwrap()
            .email
            .as_deref(),
        Some("nadia@neo-maintenance.example")
    );
    let consumer = TokenClaims {
        hosted_domain: None,
        ..claims.clone()
    };
    assert!(proven_identity(ProviderKind::Google, &consumer, &domains()).is_err());
    let elsewhere = TokenClaims {
        hosted_domain: Some("autre.example"),
        ..claims.clone()
    };
    assert!(proven_identity(ProviderKind::Google, &elsewhere, &domains()).is_err());
    let unverified = TokenClaims {
        email_verified: Some(false),
        ..claims
    };
    assert_eq!(
        proven_identity(ProviderKind::Google, &unverified, &domains())
            .unwrap()
            .email,
        None
    );
}

#[test]
fn a_generic_provider_vouches_only_for_verified_addresses() {
    let claims = TokenClaims {
        subject: "kc-42",
        email: Some("Nadia@neo-maintenance.example"),
        email_verified: Some(true),
        ..TokenClaims::default()
    };
    assert_eq!(
        proven_identity(ProviderKind::Oidc, &claims, &domains()).unwrap(),
        ProvenIdentity {
            subject: "kc-42".into(),
            email: Some("nadia@neo-maintenance.example".into()),
        }
    );
    for email_verified in [None, Some(false)] {
        let unverified = TokenClaims {
            email_verified,
            ..claims.clone()
        };
        assert_eq!(
            proven_identity(ProviderKind::Oidc, &unverified, &domains())
                .unwrap()
                .email,
            None
        );
    }
    let nobody = TokenClaims {
        subject: "",
        ..claims
    };
    assert!(proven_identity(ProviderKind::Oidc, &nobody, &domains()).is_err());
}

#[test]
fn a_saml_provider_is_designated_by_its_entity_id() {
    let entity = "https://sts.windows.net/72f988bf-86f1-41af-91ab-2d7cd011db47/";
    assert_eq!(
        declared_issuer(ProviderKind::Saml, None, Some(entity)).unwrap(),
        entity
    );
    // Not an address: any identifier, Microsoft's own included.
    assert_eq!(
        declared_issuer(ProviderKind::Saml, None, Some("urn:neo:idp")).unwrap(),
        "urn:neo:idp"
    );
    for (directory, issuer) in [
        (None, None),
        (Some(DIRECTORY), Some("urn:neo:idp")),
        (None, Some("")),
        (None, Some("urn:neo idp")),
        (None, Some(&*"u".repeat(1025))),
    ] {
        assert!(
            declared_issuer(ProviderKind::Saml, directory, issuer).is_err(),
            "{directory:?} {issuer:?}"
        );
    }
    assert!(declared_name(ProviderKind::Saml, None).is_err());
}

#[test]
fn a_saml_assertion_proves_its_subject_s_address() {
    // Entra ID: the sign-in name as subject, the object ID beside it.
    let claims = TokenClaims {
        subject: "Nadia.Benali@neo-maintenance.example",
        preferred_username: Some("Nadia.Benali@neo-maintenance.example"),
        email: Some("autre@neo-maintenance.example"),
        oid: Some("00000000-0000-0000-0000-0000000000aa"),
        ..TokenClaims::default()
    };
    assert_eq!(
        proven_identity(ProviderKind::Saml, &claims, &domains()).unwrap(),
        ProvenIdentity {
            subject: "00000000-0000-0000-0000-0000000000aa".into(),
            email: Some("nadia.benali@neo-maintenance.example".into()),
        }
    );
    // A persistent identifier as subject: its e-mail attribute.
    let persistent = TokenClaims {
        subject: "a1b2c3",
        preferred_username: Some("a1b2c3"),
        email: Some("Nadia@neo-maintenance.example"),
        oid: None,
        ..TokenClaims::default()
    };
    assert_eq!(
        proven_identity(ProviderKind::Saml, &persistent, &domains()).unwrap(),
        ProvenIdentity {
            subject: "a1b2c3".into(),
            email: Some("nadia@neo-maintenance.example".into()),
        }
    );
    let nothing = TokenClaims {
        email: None,
        ..persistent
    };
    assert_eq!(
        proven_identity(ProviderKind::Saml, &nothing, &domains())
            .unwrap()
            .email,
        None
    );
    let empty = TokenClaims::default();
    assert!(proven_identity(ProviderKind::Saml, &empty, &domains()).is_err());
}
