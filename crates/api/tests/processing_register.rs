//! The company's record of processing activities (ADR-217): drawn from its
//! settings each time it is read, naming where its data go now (the
//! operator's model on its key, then the company's own, its review and
//! embedding models at a listed endpoint, its gateway, its identity
//! provider, SCIM and its SIEM), with the durations in effect, a shortening
//! that waits and its second factor; TypeSafe named only while one of the
//! usages set sends Jev something; never a key. That another company's
//! settings are never in it: `processing_register_isolation.rs`.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::{FakeJev, JEV_KEY};
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::gateways::{NewGateway, create_gateway};
use mimorya_db::repositories::identity_providers::{NewIdentityProvider, create_identity_provider};
use serde_json::{Value, json};
use support::TestApi;
use support::decisions::{USAGES, settings};
use support::embeddings::ENDPOINT;
use support::llm::{API_KEY, REVIEW_MODEL};
use support::processing_register::{activity, keys, lines, of, read, recipients, start};

/// Paul's company declares, written as its administration would leave them,
/// a Telegram bot and an OpenID Connect provider.
async fn declare_gateway_and_provider(api: &TestApi) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    create_gateway(
        &mut tx,
        NewGateway {
            tenant_id: tenant::ID,
            kind: "telegram",
            name: "Bot Néo",
            enabled: true,
            workspace_id: "4242",
            workspace_name: "neo_bot",
            bot_user_id: "4242",
            granted_scopes: None,
        },
    )
    .await
    .unwrap();
    create_identity_provider(
        &mut tx,
        NewIdentityProvider {
            tenant_id: tenant::ID,
            kind: "oidc",
            name: "Keycloak Néo",
            issuer: "https://SSO.neo.example/realms/neo",
            client_id: "mimorya",
            enabled: true,
            required: false,
            create_accounts: false,
            saml_metadata: None,
            created_by_user_id: user::PAUL,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_register_names_where_the_company_s_data_go_now() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;

    // The demonstration company: the operator's model, a fictitious
    // connector, nothing else of its own.
    let register = read(&paul).await;
    assert_eq!(register["controller"], tenant::NAME);
    assert_eq!(
        keys(&register),
        [
            "accounts",
            "assistance",
            "memory",
            "documents",
            "connectors",
            "audit",
            "usage"
        ]
    );
    assert_eq!(
        recipients(&register, "assistance"),
        [of("Anthropic", "outsideEuropeanUnion", "operator")]
    );
    assert_eq!(
        recipients(&register, "connectors"),
        [of("Anthropic", "outsideEuropeanUnion", "operator")]
    );
    let notes = lines(&activity(&register, "connectors")["notes"]);
    assert!(
        notes.len() == 1 && notes[0].contains("données fictives"),
        "{notes:?}"
    );
    assert!(recipients(&register, "audit").is_empty());
    let assistance = lines(&activity(&register, "assistance")["retention"]);
    assert!(assistance[0].contains("depuis 24 mois :"), "{assistance:?}");
    assert_eq!(lines(&register["toComplete"]).len(), 7);

    // Its own models: Anthropic on its key, a review model and embeddings
    // at the endpoint the operator lists.
    let own = json!({ "provider": "anthropic", "model": "claude-opus-5-5", "apiKey": API_KEY });
    assert_eq!(paul.put("/admin/llm", own).await.status, StatusCode::OK);
    let review = json!({ "provider": "ollama", "model": REVIEW_MODEL, "baseUrl": ENDPOINT });
    let saved = paul.put("/admin/llm/verification", review).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let embeddings = json!({ "provider": "ollama", "model": "concepts", "baseUrl": ENDPOINT });
    let saved = paul.put("/admin/embedding", embeddings).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    declare_gateway_and_provider(&api).await;
    support::scim::made(&paul, "Entra ID").await;
    support::audit_exports::made(&paul, "SIEM Néo").await;
    let version = paul.get("/admin/retention").await.json()["version"].clone();
    let shortened = json!({
        "conversationMonths": 12,
        "episodeMonths": 12,
        "departedDays": 30,
        "replaces": version,
    });
    assert_eq!(
        paul.put("/admin/retention", shortened).await.status,
        StatusCode::OK
    );

    let register = read(&paul).await;
    assert_eq!(
        keys(&register),
        [
            "accounts",
            "assistance",
            "memory",
            "documents",
            "connectors",
            "gateways",
            "audit",
            "usage"
        ]
    );
    let listed = "Serveur de modèles désigné par l’exploitant (embeddings.test), API Ollama";
    assert_eq!(
        recipients(&register, "assistance"),
        [
            of("Anthropic", "outsideEuropeanUnion", "company"),
            of(listed, "operatorServer", "company"),
        ]
    );
    assert_eq!(
        recipients(&register, "memory"),
        [
            of("Anthropic", "outsideEuropeanUnion", "company"),
            of(listed, "operatorServer", "company"),
        ]
    );
    assert_eq!(
        recipients(&register, "gateways"),
        [of("Telegram", "outsideEuropeanUnion", "company")]
    );
    assert_eq!(
        recipients(&register, "accounts"),
        [
            of(
                "Fournisseur OpenID Connect (sso.neo.example)",
                "declaredByCompany",
                "company"
            ),
            of(
                "Fournisseur d’identité de l’entreprise",
                "declaredByCompany",
                "company"
            ),
        ]
    );
    assert_eq!(
        recipients(&register, "audit"),
        [of("SIEM de l’entreprise", "declaredByCompany", "company")]
    );
    // The shortening waits a week: the purge keeps 24 months until then.
    let assistance = lines(&activity(&register, "assistance")["retention"]);
    assert!(
        assistance[0].contains("depuis 24 mois (12 mois à partir du "),
        "{assistance:?}"
    );
    let measures = lines(&register["measures"]);
    assert!(measures.iter().any(|m| m.contains("exigé de personne")));
    assert!(
        measures
            .iter()
            .any(|m| m.contains("fournisseur d’identité"))
    );

    // Its second factor, as the company requires it now.
    let required = json!({ "requirement": "administrators" });
    let saved = paul.put("/admin/second-factor", required).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let measures = lines(&read(&paul).await["measures"]);
    assert!(
        measures
            .iter()
            .any(|m| m.contains("exigé des administrateurs"))
    );
    // Paul signs in again without one in the tests after.
    let optional = json!({ "requirement": "optional" });
    assert_eq!(
        paul.put("/admin/second-factor", optional).await.status,
        StatusCode::OK
    );
}

/// The activities TypeSafe is named in.
fn sent_to_typesafe(register: &Value) -> Vec<String> {
    keys(register)
        .into_iter()
        .filter(|key| {
            recipients(register, key).iter().any(|(name, place, by)| {
                name == "TypeSafe (Jev)" && place == "outsideEuropeanUnion" && by == "company"
            })
        })
        .collect()
}

/// `engine`, the sensitive data of memories observed.
fn guarding_memories(engine: &str, key: Option<&str>) -> Value {
    let mut body = settings(engine, key);
    let guard = USAGES.iter().position(|u| *u == "memoryGuard").unwrap();
    body["usages"][guard]["mode"] = json!("observe");
    body
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn typesafe_is_named_while_jev_receives_something() {
    let jev = FakeJev::serve().await;
    let api = TestApi::start_with_jev(&jev, true).await;
    let paul = api.sign_in(user::PAUL).await;

    // Jev chosen with its key, every usage off: nothing goes to it.
    support::decisions::set(&paul, settings("jev", Some(JEV_KEY))).await;
    assert!(sent_to_typesafe(&read(&paul).await).is_empty());

    // The sensitive data of memories: the memories only.
    support::decisions::set(&paul, guarding_memories("jev", None)).await;
    assert_eq!(sent_to_typesafe(&read(&paul).await), ["memory"]);

    // Instructions looked for in what a tool brings: a connector's contents
    // and a deposited document's passages.
    let mut body = guarding_memories("jev", None);
    let screen = USAGES
        .iter()
        .position(|u| *u == "injectionDetection")
        .unwrap();
    body["usages"][screen]["mode"] = json!("active");
    support::decisions::set(&paul, body).await;
    assert_eq!(
        sent_to_typesafe(&read(&paul).await),
        ["memory", "documents", "connectors"]
    );

    // The same usage on the main model: Jev no more.
    support::decisions::set(&paul, guarding_memories("mainModel", None)).await;
    assert!(sent_to_typesafe(&read(&paul).await).is_empty());
    assert!(jev.asked().is_empty());
}
