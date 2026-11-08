//! The export of a person's own data (ADR-187). Marc downloads a JSON file
//! of his account and units, his conversations with their messages, his
//! personal memories, scheduled tasks, triggers, missions and absences;
//! never a colleague's conversation or memory, nor the company's, nor
//! another company's; never a token nor a password hash. The export is
//! traced in the company's audit log by its counts alone. A process makes
//! a few exports at once, one at a time for a person.

mod support;

use mimorya_api::services::personal_export::EXPORTS_AT_ONCE;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::audit_exports::audit_log;
use support::idle_conversations::conversation;
use support::personal_export::{absence, mission};
use support::queue::API;
use support::retention::memory;
use support::{TestApi, companies};
use uuid::Uuid;

const EXPORT: &str = "/account/export";

/// The titles of `items`, read at `field`.
fn titles(items: &Value, field: &str) -> Vec<String> {
    items
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item[field].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_person_exports_their_own_data_and_nobody_else_s() {
    let api = TestApi::start_with(API).await;
    let marc = api.sign_in(user::MARC).await;
    let paul = api.sign_in(user::PAUL).await;
    let neo = tenant::ID;

    let own = conversation(neo, user::MARC, "export conversation de Marc", 1).await;
    conversation(neo, user::PAUL, "export conversation de Paul", 1).await;
    let acme = companies::create(&api, "acme-export", "admin@acme-export.example").await;
    conversation(
        acme.tenant_id,
        acme.admin_id,
        "export conversation d’Acme",
        1,
    )
    .await;
    memory(
        neo,
        "export mémoire de Marc",
        "preference",
        Some(user::MARC),
        1,
        false,
    )
    .await;
    memory(
        neo,
        "export mémoire de Paul",
        "preference",
        Some(user::PAUL),
        1,
        false,
    )
    .await;
    memory(
        neo,
        "export mémoire de l’entreprise",
        "fact",
        None,
        1,
        false,
    )
    .await;
    mission(neo, user::MARC, own, "export mission de Marc").await;
    absence(neo, user::MARC, user::PAUL).await;
    let task = marc
        .post(
            "/scheduled-tasks",
            json!({ "title": "export tâche de Marc", "instruction": "Faire le point",
                    "weekdays": [1], "time": "08:30", "timeZone": "Europe/Paris" }),
        )
        .await;
    assert_eq!(task.status.as_u16(), 201, "{}", task.text());
    let trigger = marc
        .post(
            "/inbound-hooks",
            json!({ "title": "export déclencheur de Marc", "instruction": "Lire l’envoi" }),
        )
        .await;
    assert_eq!(trigger.status.as_u16(), 201, "{}", trigger.text());
    let token = trigger.json()["token"].as_str().unwrap().to_owned();

    let exported = marc.get(EXPORT).await;
    assert_eq!(exported.status.as_u16(), 200, "{}", exported.text());
    let disposition = exported.headers["content-disposition"].to_str().unwrap();
    assert!(
        disposition.starts_with("attachment; filename=\"mimorya-mes-donnees-20"),
        "{disposition}"
    );
    let export = exported.json();

    // His account, his units.
    assert_eq!(export["account"]["id"], json!(user::MARC));
    assert_eq!(
        export["account"]["email"],
        "marc.lefevre@neo-maintenance.example"
    );
    assert!(!export["account"]["units"].as_array().unwrap().is_empty());
    // His conversation and its messages; no one else's.
    let conversations = titles(&export["conversations"], "title");
    assert!(conversations.contains(&"export conversation de Marc".to_owned()));
    assert!(
        !conversations
            .iter()
            .any(|title| title.contains("de Paul") || title.contains("Acme"))
    );
    let his = export["conversations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|conversation| conversation["id"] == json!(own))
        .unwrap();
    assert_eq!(titles(&his["messages"], "content"), ["question", "réponse"]);
    // His personal memory alone.
    let memories = titles(&export["memories"], "title");
    assert!(memories.contains(&"export mémoire de Marc".to_owned()));
    assert!(
        !memories
            .iter()
            .any(|title| title.contains("de Paul") || title.contains("entreprise"))
    );
    // What he set up, and his absence with who stands in.
    assert!(
        titles(&export["scheduledTasks"], "title").contains(&"export tâche de Marc".to_owned())
    );
    assert!(
        titles(&export["triggers"], "title").contains(&"export déclencheur de Marc".to_owned())
    );
    assert!(titles(&export["missions"], "title").contains(&"export mission de Marc".to_owned()));
    assert_eq!(titles(&export["absences"], "delegate"), ["Paul Renaud"]);
    // Never a token, nor a password hash.
    let text = exported.text();
    assert!(!text.contains(&token));
    assert!(!text.contains("$argon2"));

    // Traced in the company's log, by its counts alone.
    let traced: Vec<_> = audit_log(&paul)
        .await
        .into_iter()
        .filter(|entry| entry["action"] == "account.exported")
        .collect();
    assert_eq!(traced.len(), 1, "{traced:?}");
    assert_eq!(traced[0]["actorUserId"], json!(user::MARC));
    let messages: usize = export["conversations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|conversation| conversation["messages"].as_array().unwrap().len())
        .sum();
    assert_eq!(
        traced[0]["metadata"],
        json!({
            "conversations": export["conversations"].as_array().unwrap().len(),
            "messages": messages,
            "memories": export["memories"].as_array().unwrap().len(),
        })
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_process_makes_a_few_exports_one_at_a_time_for_a_person() {
    let api = TestApi::start_with(API).await;
    let marc = api.sign_in(user::MARC).await;
    let paul = api.sign_in(user::PAUL).await;
    let exports = &api.app.services.exports;

    // Marc's export under way: his next one waits; Paul's goes.
    let held = exports.take(user::MARC).unwrap();
    let again = marc.get(EXPORT).await;
    assert_eq!(again.status.as_u16(), 409, "{}", again.text());
    assert!(again.error_message().contains("déjà en cours"));
    assert_eq!(paul.get(EXPORT).await.status.as_u16(), 200);

    // Every place taken: Paul's waits too.
    let others: Vec<_> = (1..EXPORTS_AT_ONCE as u128)
        .map(|n| exports.take(Uuid::from_u128(n)).unwrap())
        .collect();
    let busy = paul.get(EXPORT).await;
    assert_eq!(busy.status.as_u16(), 503, "{}", busy.text());
    drop(others);
    drop(held);
    assert_eq!(marc.get(EXPORT).await.status.as_u16(), 200);
}
