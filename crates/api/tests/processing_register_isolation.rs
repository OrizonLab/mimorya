//! One company's register never names another's settings (ADR-217); a
//! disabled connector sends nothing, its activity noted for what it still
//! keeps of people, as counted, nothing for one that keeps nothing
//! (ADR-220).

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::user;
use mimorya_db::repositories::account_links::{NewAccountLink, put_account_link};
use mimorya_db::repositories::connector_person_authorizations::{
    NewPersonAuthorization, put_person_authorization,
};
use mimorya_db::repositories::connectors::{NewConnectorInstance, create_connector_instance};
use mimorya_db::repositories::mail_accounts::put_mail_account;
use mimorya_db::repositories::secrets::StoredSecret;
use serde_json::json;
use support::accounts::{INVITED_PASSWORD, accept};
use support::companies;
use support::processing_register::{REGISTER, activity, lines, of, read, recipients, start};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn one_company_s_settings_are_never_in_another_s_register() {
    let api = start().await;
    let acme = companies::create(&api, "acme-registre", "jeanne@acme-registre.example").await;
    let mut tx = api.app.services.db.begin(acme.tenant_id).await.unwrap();
    let config = json!({ "prefix": "facturation", "url": "https://facturation.acme.example/api" });
    create_connector_instance(
        &mut tx,
        NewConnectorInstance {
            tenant_id: acme.tenant_id,
            connector_type: "rest",
            name: "Facturation Acme",
            enabled: true,
            config: &config,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let paul = api.sign_in(user::PAUL).await;
    let neo = paul.get(REGISTER).await.text();
    assert!(!neo.contains("acme") && !neo.contains("Acme"), "{neo}");

    // Acme's administrator reads its own, and nothing of Néo's.
    let accepted = accept(&api, &acme.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    let jeanne = api
        .sign_in_with("jeanne@acme-registre.example", INVITED_PASSWORD)
        .await;
    let register = read(&jeanne).await;
    assert_eq!(register["controller"], "Acme Industrie");
    assert!(!register.to_string().contains("Néo"));
    assert_eq!(
        recipients(&register, "connectors"),
        [of(
            "API REST (facturation.acme.example)",
            "declaredByCompany",
            "company"
        )]
    );
}

/// A sealed value the tests store: never read back.
fn sealed() -> StoredSecret {
    StoredSecret {
        ciphertext: vec![1; 16],
        iv: vec![2; 12],
        auth_tag: vec![3; 16],
        key_version: 1,
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_disabled_connector_is_noted_for_what_it_keeps_of_people() {
    let api = start().await;
    let address = "jeanne@acme-desactive.example";
    let acme = companies::create(&api, "acme-desactive", address).await;
    let (tenant_id, jeanne_id) = (acme.tenant_id, acme.admin_id);
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    // Disabled, each keeping one kind of Jeanne's data; an API keeping no
    // one's.
    let mut created = Vec::new();
    for (kind, name) in [
        ("microsoft365", "Microsoft Acme"),
        ("mail_server", "Messagerie Acme"),
        ("mcp", "Wiki Acme"),
        ("rest", "Paie Acme"),
    ] {
        let config = json!({ "url": "https://service.acme.example/api" });
        let id = create_connector_instance(
            &mut tx,
            NewConnectorInstance {
                tenant_id,
                connector_type: kind,
                name,
                enabled: false,
                config: &config,
            },
        )
        .await
        .unwrap();
        created.push(id);
    }
    let token = sealed();
    put_account_link(
        &mut tx,
        NewAccountLink {
            tenant_id,
            connector_id: created[0],
            user_id: jeanne_id,
            subject: "sujet-jeanne",
            address,
            scopes: "Mail.Read",
            refresh_token: &token,
        },
    )
    .await
    .unwrap();
    put_mail_account(&mut tx, tenant_id, created[1], jeanne_id, "jeanne", &token)
        .await
        .unwrap();
    put_person_authorization(
        &mut tx,
        NewPersonAuthorization {
            tenant_id,
            connector_id: created[2],
            user_id: jeanne_id,
            issuer: "https://auth.acme.example",
            client_id: "mimorya",
            resource: "https://service.acme.example/api",
            scopes: "wiki.read",
            access_token: &token,
            access_expires_at: None,
            refresh_token: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let accepted = accept(&api, &acme.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    let jeanne = api.sign_in_with(address, INVITED_PASSWORD).await;
    let register = read(&jeanne).await;
    // No recipient: no run uses them. Each kept kind noted in its own
    // connector's note; the API keeping no one's data, not at all.
    assert!(recipients(&register, "connectors").is_empty());
    let notes = lines(&activity(&register, "connectors")["notes"]);
    assert_eq!(notes.len(), 3, "{notes:?}");
    let noted = |name: &str, what: &str| {
        assert!(
            notes.iter().any(|note| {
                note.starts_with(&format!("Connecteur « {name} » désactivé")) && note.contains(what)
            }),
            "{name}: {notes:?}"
        );
    };
    noted(
        "Microsoft Acme",
        "1 compte(s) relié(s), jetons de renouvellement scellés",
    );
    noted(
        "Messagerie Acme",
        "1 compte(s) de messagerie, mots de passe scellés",
    );
    noted(
        "Wiki Acme",
        "1 autorisation(s) OAuth données par chacun, jetons scellés",
    );
    assert!(!notes.iter().any(|note| note.contains("Paie Acme")));

    // None of it counted in another company's register.
    let paul = api.sign_in(user::PAUL).await;
    let neo = paul.get(REGISTER).await.text();
    assert!(!neo.contains("Acme"), "{neo}");
}
