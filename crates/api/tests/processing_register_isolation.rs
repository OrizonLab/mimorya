//! One company's register never names another's settings (ADR-217); a
//! disabled connector sends nothing, its activity noted for what it still
//! keeps of people, as counted, nothing for one that keeps nothing
//! (ADR-220).

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::user;
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
    // An enabled API; a disabled one, keeping nothing of anyone; a disabled
    // mail server, keeping Jeanne's account.
    let mut created = Vec::new();
    for (kind, name, host, enabled) in [
        ("rest", "Facturation Acme", "facturation", true),
        ("rest", "Paie Acme", "paie", false),
        ("mail_server", "Messagerie Acme", "mail", false),
    ] {
        let config = json!({ "prefix": host, "url": format!("https://{host}.acme.example/api") });
        let id = create_connector_instance(
            &mut tx,
            NewConnectorInstance {
                tenant_id: acme.tenant_id,
                connector_type: kind,
                name,
                enabled,
                config: &config,
            },
        )
        .await
        .unwrap();
        created.push(id);
    }
    let sealed = StoredSecret {
        ciphertext: vec![1; 16],
        iv: vec![2; 12],
        auth_tag: vec![3; 16],
        key_version: 1,
    };
    put_mail_account(
        &mut tx,
        acme.tenant_id,
        created[2],
        acme.admin_id,
        "jeanne",
        &sealed,
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
    // The disabled ones send nothing: no recipient but the enabled API. The
    // mail server is noted for the account it keeps, the API keeping no one's
    // data not at all.
    assert_eq!(
        recipients(&register, "connectors"),
        [of(
            "API REST (facturation.acme.example)",
            "declaredByCompany",
            "company"
        )]
    );
    assert_eq!(
        lines(&activity(&register, "connectors")["notes"]),
        [
            "Connecteur « Messagerie Acme » désactivé : aucun run ne l’utilise ; restent stockés, \
             jusqu’à leur retrait, la désactivation du compte de chacun, un changement de la \
             déclaration du connecteur ou sa suppression : 1 compte(s) de messagerie, mots de \
             passe scellés"
        ]
    );
}
