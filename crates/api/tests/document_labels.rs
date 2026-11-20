//! Deposited documents classified by the company's decision service
//! (ADR-213, D4), Jev the engine: active, a stricter level is applied at
//! once and audited, the depositor beyond its clearance no longer reading
//! it; a looser one and a type are only proposed, shown to those who may
//! write the document. Only the title and the first pages are sent. A
//! person sets the labels only where they may write the document at both
//! levels, at the version they saw; anyone else is refused or answered as
//! absent, and another company reaches nothing. Observed, failed or off,
//! nothing changes.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_connectors::testing::{FakeJev, JEV_KEY, JevBehaviour};
use mimorya_core::demo::{tenant, unit, user};
use mimorya_extract::testing::pdf;
use serde_json::{Value, json};
use support::decisions::{decision_calls, forget_decision_calls, set, settings};
use support::documents::{Target, deposited, replace};
use support::refusals::{entries_since, latest_entry};
use support::{Client, TestApi, companies};
use uuid::Uuid;

/// The documents' labels set to `mode` at 0.5, Jev the engine.
fn labelling(mode: &str) -> Value {
    let mut body = settings("jev", Some(JEV_KEY));
    body["usages"][3] = json!({ "key": "documentLabels", "mode": mode, "threshold": 0.5 });
    body
}

/// A contract of five pages, the fourth saying what must never be sent.
fn contract() -> Vec<u8> {
    pdf(&[
        "Contrat de maintenance entre Neo Maintenance et son client.",
        "Article 2 : duree du contrat.",
        "Article 3 : prix.",
        "Annexe : PAGE-QUATRE-JAMAIS-ENVOYEE.",
        "Annexe : signatures.",
    ])
}

async fn start(jev: &FakeJev, mode: &str) -> (TestApi, Uuid) {
    let api = TestApi::start_with_jev(jev, true).await;
    let paul = api.sign_in(user::PAUL).await;
    set(&paul, labelling(mode)).await;
    (api, tenant::ID)
}

/// Classifies what awaits it, as the background pass does.
async fn classify(api: &TestApi) {
    let labels = &api.app.services.document_labels;
    labels.classify_pending().await.unwrap();
}

/// The document's level and type, as stored.
async fn stored(api: &TestApi, id: Uuid) -> (String, Option<String>) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let row = sqlx::query_as("select sensitivity, document_type from documents where id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    row
}

/// The requests awaiting a classification, and the proposals kept, of
/// `ids` (the tests of the binary share its database).
async fn pending(api: &TestApi, ids: &[Uuid]) -> (i64, i64) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let counts = sqlx::query_as(
        "select (select count(*) from document_label_requests where document_id = any($1)), \
                (select count(*) from document_label_proposals where document_id = any($1))",
    )
    .bind(ids)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    counts
}

async fn item(client: &Client<'_>, id: Uuid) -> Value {
    let read = client.get(&format!("/documents/{id}")).await;
    assert_eq!(read.status, StatusCode::OK, "{}", read.text());
    read.json()
}

fn labels(version: i32, kind: Option<&str>, sensitivity: &str) -> Value {
    json!({ "version": version, "documentType": kind, "sensitivity": sensitivity })
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_stricter_level_is_raised_at_once_a_looser_one_only_proposed() {
    forget_decision_calls().await;
    let jev = FakeJev::serve().await;
    let (api, _) = start(&jev, "active").await;
    let claire = api.sign_in(user::CLAIRE).await;
    let julien = api.sign_in(user::JULIEN).await;
    let since = latest_entry(&api).await;

    // Said confidential: raised at once, the level it held kept for Julien.
    jev.score_at(2);
    let north = Target::unit(unit::TEAM_NORTH);
    let raised = deposited(&julien, "Contrat maintenance.pdf", north, &contract()).await;
    classify(&api).await;
    assert_eq!(stored(&api, raised).await, ("confidential".into(), None));
    let shown = item(&julien, raised).await;
    assert_eq!(shown["sensitivity"], "confidential");
    assert_eq!(shown["documentType"], Value::Null);
    assert_eq!(
        shown["labelProposal"],
        json!({ "version": 1, "documentType": "contract", "sensitivity": null,
                "raisedFrom": "internal" })
    );
    // Its title and first pages alone were sent.
    let asked = jev.asked().last().unwrap().body.clone();
    let state = asked["state"].as_object().unwrap();
    assert_eq!(state.keys().collect::<Vec<_>>(), ["titre", "debut"]);
    assert_eq!(state["titre"], "Contrat maintenance");
    let first = state["debut"].as_str().unwrap();
    assert!(first.contains("Article 3 : prix."), "{first}");
    assert!(!asked.to_string().contains("PAGE-QUATRE"), "{asked}");
    assert!(!asked.to_string().contains(".pdf"), "{asked}");

    // Said restricted: raised beyond Julien's clearance, he no longer reads it.
    jev.score_at(3);
    let beyond = deposited(&julien, "Contrat salaires.pdf", north, &contract()).await;
    classify(&api).await;
    assert_eq!(stored(&api, beyond).await, ("restricted".into(), None));
    let gone = julien.get(&format!("/documents/{beyond}")).await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND, "{}", gone.text());

    // Said public of a confidential document: proposed only.
    jev.score_at(0);
    let company = Target::default().sensitive("confidential");
    let looser = deposited(&claire, "Contrat client.pdf", company, &contract()).await;
    classify(&api).await;
    assert_eq!(stored(&api, looser).await, ("confidential".into(), None));
    assert_eq!(
        item(&claire, looser).await["labelProposal"],
        json!({ "version": 1, "documentType": "contract", "sensitivity": "public",
                "raisedFrom": null })
    );
    assert_eq!(pending(&api, &[raised, beyond, looser]).await, (0, 3));

    let audit = entries_since(&api, since).await;
    for said in [
        "document.sensitivity_raised",
        "\"from\":\"internal\"",
        "\"to\":\"confidential\"",
        "\"to\":\"restricted\"",
        "document.labels_proposed",
        "\"raisedFrom\":\"internal\"",
    ] {
        assert!(audit.contains(said), "{said}: {audit}");
    }
    for never in ["Contrat maintenance", "Article", "prix"] {
        assert!(!audit.contains(never), "{never} audited: {audit}");
    }
    // Each decision counted with the role « décision », on the company's key.
    let calls = decision_calls(&api).await;
    assert_eq!(calls.len(), 3, "{calls:?}");
    assert!(calls.iter().all(|call| call.0 == "typesafe"), "{calls:?}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_a_person_who_may_write_both_levels_sets_them_at_the_version_seen() {
    let jev = FakeJev::serve().await;
    let (api, _) = start(&jev, "active").await;
    let julien = api.sign_in(user::JULIEN).await;
    let lucas = api.sign_in(user::LUCAS).await;
    jev.score_at(0);
    let north = Target::unit(unit::TEAM_NORTH);
    let id = deposited(&julien, "Contrat equipe.pdf", north, &contract()).await;
    classify(&api).await;
    let url = format!("/documents/{id}/labels");
    let dismiss = format!("/documents/{id}/proposal/dismiss");

    // Lucas reads it, but writes nothing: no proposal shown, absent to him.
    assert_eq!(item(&lucas, id).await["labelProposal"], Value::Null);
    let applied = lucas.put(&url, labels(1, Some("contract"), "public")).await;
    assert_eq!(applied.status, StatusCode::NOT_FOUND, "{}", applied.text());
    let dismissed = lucas.post(&dismiss, json!({ "version": 1 })).await;
    assert_eq!(dismissed.status, StatusCode::NOT_FOUND);
    // Julien may not give a level beyond his clearance, nor at a version
    // other than the one he saw; nothing else is read.
    let beyond = julien.put(&url, labels(1, None, "restricted")).await;
    assert_eq!(beyond.status, StatusCode::FORBIDDEN, "{}", beyond.text());
    let stale = julien.put(&url, labels(2, None, "public")).await;
    assert_eq!(stale.status, StatusCode::CONFLICT, "{}", stale.text());
    let mut unknown = labels(1, None, "public");
    unknown["raisedFrom"] = json!("restricted");
    assert_eq!(
        julien.put(&url, unknown).await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(stored(&api, id).await, ("internal".into(), None));

    // He applies what was proposed: lowered by him, the proposal dropped.
    let since = latest_entry(&api).await;
    let applied = julien
        .put(&url, labels(1, Some("contract"), "public"))
        .await;
    assert_eq!(applied.status, StatusCode::OK, "{}", applied.text());
    let shown = item(&julien, id).await;
    assert_eq!(
        (
            &shown["sensitivity"],
            &shown["documentType"],
            &shown["labelProposal"]
        ),
        (&json!("public"), &json!("contract"), &Value::Null)
    );
    let audit = entries_since(&api, since).await;
    assert!(audit.contains("document.labels_set"), "{audit}");
    assert!(audit.contains("\"proposed\":true"), "{audit}");
    let none = julien.post(&dismiss, json!({ "version": 1 })).await;
    assert_eq!(none.status, StatusCode::NOT_FOUND, "{}", none.text());

    // Replaced, it is classified again: said internal, raised from public.
    jev.score_at(1);
    let replaced = replace(&julien, id, "Contrat equipe v2.pdf", 1, &contract()).await;
    assert_eq!(replaced.status, StatusCode::OK, "{}", replaced.text());
    classify(&api).await;
    assert_eq!(
        stored(&api, id).await,
        ("internal".into(), Some("contract".into()))
    );
    let proposal = &item(&julien, id).await["labelProposal"];
    assert_eq!(
        proposal,
        &json!({ "version": 2, "documentType": null, "sensitivity": null,
                 "raisedFrom": "public" })
    );
    // A dismissal names the version classified; the level stays.
    let stale = julien.post(&dismiss, json!({ "version": 1 })).await;
    assert_eq!(stale.status, StatusCode::CONFLICT, "{}", stale.text());
    let dismissed = julien.post(&dismiss, json!({ "version": 2 })).await;
    assert_eq!(dismissed.status, StatusCode::OK, "{}", dismissed.text());
    assert_eq!(item(&julien, id).await["labelProposal"], Value::Null);
    assert_eq!(stored(&api, id).await.0, "internal");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn observed_failed_or_off_nothing_changes() {
    let jev = FakeJev::serve().await;
    let (api, _) = start(&jev, "off").await;
    let paul = api.sign_in(user::PAUL).await;
    let claire = api.sign_in(user::CLAIRE).await;
    jev.score_at(3);

    // Off: nothing awaits, nothing is asked.
    let off = deposited(&claire, "Note.pdf", Target::default(), &contract()).await;
    assert_eq!(pending(&api, &[off]).await, (0, 0));
    classify(&api).await;
    assert!(jev.asked().is_empty());

    // Observed: asked and journaled, without effect.
    set(&paul, labelling("observe")).await;
    let observed = deposited(&claire, "Note observee.pdf", Target::default(), &contract()).await;
    classify(&api).await;
    assert_eq!(jev.asked().len(), 1);
    assert_eq!(pending(&api, &[observed]).await, (0, 0));

    // Failed, Jev overloaded and the main model too: the depositor's level
    // stays, its request is not asked again.
    set(&paul, labelling("active")).await;
    jev.behave(JevBehaviour::Overloaded);
    api.script(vec![ScriptedStep::fails("surchargé")]);
    let failed = deposited(&claire, "Note echouee.pdf", Target::default(), &contract()).await;
    classify(&api).await;
    assert_eq!(api.model.as_ref().unwrap().remaining(), 0);
    assert_eq!(pending(&api, &[failed]).await, (0, 0));

    // Requested, then the usage turned off: dropped without asking.
    jev.behave(JevBehaviour::Answers);
    let asked = jev.asked().len();
    let late = deposited(&claire, "Note tardive.pdf", Target::default(), &contract()).await;
    assert_eq!(pending(&api, &[late]).await, (1, 0));
    set(&paul, labelling("off")).await;
    classify(&api).await;
    assert_eq!(jev.asked().len(), asked);
    assert_eq!(pending(&api, &[late]).await, (0, 0));

    for id in [off, observed, failed, late] {
        assert_eq!(stored(&api, id).await, ("internal".into(), None));
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_company_reaches_neither_the_requests_nor_the_proposals() {
    let jev = FakeJev::serve().await;
    let (api, _) = start(&jev, "active").await;
    let claire = api.sign_in(user::CLAIRE).await;
    jev.score_at(0);
    let company = Target::default().sensitive("confidential");
    let id = deposited(&claire, "Contrat prive.pdf", company, &contract()).await;
    let other = companies::create(&api, "acme-etiquettes", "jeanne@acme-etiquettes.example").await;
    let db = &api.app.services.db;
    let mut tx = db.begin(other.tenant_id).await.unwrap();
    let seen: i64 = sqlx::query_scalar(
        "select (select count(*) from document_label_requests) \
         + (select count(*) from document_label_proposals)",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(seen, 0);
    let lowered = sqlx::query("update documents set sensitivity = 'public' where id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap()
        .rows_affected();
    assert_eq!(lowered, 0);
    let written = sqlx::query(
        "insert into document_label_proposals (tenant_id, document_id, version, sensitivity, engine) \
         values ($1, $2, 1, 'public', 'jev')",
    )
    .bind(tenant::ID)
    .bind(id)
    .execute(&mut *tx)
    .await;
    assert!(written.is_err(), "written in another company's name");
    drop(tx);
    classify(&api).await;
    assert_eq!(stored(&api, id).await.0, "confidential");
    assert_eq!(pending(&api, &[id]).await, (0, 1));
}
